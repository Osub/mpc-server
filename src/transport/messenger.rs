use std::convert::TryInto;

use anyhow::{Context, Result};
use futures::{Sink, Stream, StreamExt};
use secp256k1::{Message, PublicKey, SecretKey, sign, Signature, verify};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::actors::messages::{Envelope, SignedEnvelope};
use crate::transport::sign_envelope;
use crate::transport::parse_signed;

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
    let own_pub_key = pub_key.as_ref().clone();

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
        })
        .filter(
            move |res:&Result<SignedEnvelope<String>>| {
                futures_util::future::ready(match res {
                    Ok(envelope) => {
                        envelope.sender_public_key != own_pub_key
                    }
                    Err(_) => { true }
                })
            }
        );
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
