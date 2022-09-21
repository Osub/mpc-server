use std::convert::TryInto;

use anyhow::{Context, Result};

use secp256k1::{Message, PublicKey, SecretKey, sign, Signature, verify};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::actors::messages::{Envelope, SignedEnvelope};

#[derive(Debug, Error)]
enum VerifyError {
    #[error("Verification failed.")]
    Failed,
}

pub fn parse_signed(msg: String)->Result<SignedEnvelope<String>> {
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

pub fn sign_envelope(key: &SecretKey, pub_key: &str, envelope: Envelope) -> Result<SignedEnvelope<String>> {
    let mut hasher = Sha256::new();
    hasher.update(envelope.message.as_bytes());
    let hash = hasher.finalize();
    let hbytes = hash.as_slice().try_into().context("Create hash")?;
    let message = Message::parse(hbytes);
    let (sig, _recid) = sign(&message, key);
    let signature = hex::encode(sig.serialize());
    let signed = SignedEnvelope {
        room: envelope.room,
        message: envelope.message,
        sender_public_key: pub_key.to_string(),
        signature,
    };
    Ok(signed)
}
