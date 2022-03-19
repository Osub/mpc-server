use std::convert::TryInto;
use std::ops::Deref;
use anyhow::{Context, Result};
use curv::elliptic::curves::ECScalar;
use curv::elliptic::curves::secp256_k1::Secp256k1Scalar;
// use curv::elliptic::curves::Scalar;
use futures::{Sink, Stream, StreamExt, TryStreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize, Serializer};
use structopt::StructOpt;

use round_based::Msg;
use crate::messages::{EcdsaSignature, Envelope, SignedEnvelope};
use secp256k1::{SecretKey, Message, sign, verify, PublicKey, Signature};
use secp256k1::curve::Scalar;
use serde::ser::SerializeStruct;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
enum VerifyError {
    #[error("Verification failed.")]
    Failed,
}

fn parse_signed(msg: String)->Result<SignedEnvelope<String>> {
    let signed = serde_json::from_str::<SignedEnvelope<String>>(&msg).context("deserialize message")?;
    let send_pk_str = signed.sender_public_key.clone();
    let bytes = hex::decode(send_pk_str).context("Wrong pub key")?;

    let sender_pub = PublicKey::parse_slice(bytes.as_slice(), None).context("Failed to parse pub key")?;
    let sbytes = hex::decode(signed.signature.clone()).context("Wrong signature")?;
    let sig = Signature::parse_slice(sbytes.as_slice())?;

    let mut hasher = Sha256::new();
    hasher.update(signed.message.as_bytes());
    let hash = hasher.finalize();
    let hbytes = hash.as_slice().try_into().context("Create hash")?;
    let message = Message::parse(hbytes);
    let verified = verify(&message, &sig, &sender_pub);
    let out = if verified {
        Ok(signed)
    } else {
        Err(VerifyError::Failed)
    }?;
    Ok(out)
}

fn sign_envelope(key: &SecretKey, pub_key: &String, envelope: Envelope) -> Result<SignedEnvelope<String>> {
    let mut hasher = Sha256::new();
    hasher.update(envelope.message.as_bytes());
    let hash = hasher.finalize();
    let hbytes = hash.as_slice().try_into().context("Create hash")?;
    let message = Message::parse(hbytes);
    let (sig, recid) = sign(&message, key);
    let signature = hex::encode(sig.serialize());
    let signed = SignedEnvelope {
        room: envelope.room,
        message: envelope.message,
        sender_public_key: pub_key.clone(),
        signature,
    };
    Ok(signed)
}

pub async fn join_computation(
    url: surf::Url,
    key: SecretKey,
) -> Result<(
    impl Stream<Item=Result<SignedEnvelope<String>>>,
    impl Sink<Envelope, Error=anyhow::Error>,
)>
{
    let key = Box::new(key);
    let pub_key = PublicKey::from_secret_key(&key);
    let pub_key = Box::new(hex::encode(pub_key.serialize_compressed()));
    let client = SmClient::new(url).context("construct SmClient")?;

    // Construct channel of incoming messages
    let incoming = client
        .subscribe()
        .await
        .context("subscribe")?
        .filter_map(|msg| async move {
            match msg {
                Ok(msg) => {Some(parse_signed(msg))}
                Err(_) => {None}
            }
        });

    // Construct channel of outgoing messages
    let outgoing = futures::sink::unfold((client, key, pub_key), |(client, key, pub_key), unsigned: Envelope| async move {
        let signed = sign_envelope(key.as_ref(), pub_key.as_ref(), unsigned).context("Failed to sign")?;
        let serialized = serde_json::to_string(&signed)?;
        client
            .broadcast(&serialized)
            .await
            .context("broadcast message")?;
        Ok::<_, anyhow::Error>((client, key, pub_key))
    });

    Ok((incoming, outgoing))
}

pub struct SmClient {
    http_client: surf::Client,
}

impl SmClient {
    pub fn new(url: surf::Url) -> Result<Self> {
        let config = surf::Config::new()
            .set_base_url(url)
            .set_timeout(None);
        Ok(Self {
            http_client: config.try_into()?,
        })
    }

    pub async fn broadcast(&self, message: &str) -> Result<()> {
        self.http_client
            .post("broadcast")
            .body(message)
            .await
            .map_err(|e| e.into_inner())?;
        Ok(())
    }

    pub async fn subscribe(&self) -> Result<impl Stream<Item=Result<String>>> {
        let response = self
            .http_client
            .get("subscribe")
            .await
            .map_err(|e| e.into_inner())?;
        let events = async_sse::decode(response);
        Ok(events.filter_map(|msg| async {
            match msg {
                Ok(async_sse::Event::Message(msg)) => Some(
                    String::from_utf8(msg.into_bytes())
                        .context("SSE message is not valid UTF-8 string"),
                ),
                Ok(_) => {
                    // ignore other types of events
                    None
                }
                Err(e) => Some(Err(e.into_inner())),
            }
        }))
    }
}
