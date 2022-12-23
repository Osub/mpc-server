use anyhow::{Context, Result};
use secp256k1::SecretKey;
use crate::actors::messages::{Encrypted, GenericProtocolMessage};
use crate::core::PublicKeyGroup;
use crate::pb::types::WireMessage;

pub(crate) trait Decrypter {
    fn maybe_decrypt(&mut self, msg: &mut WireMessage) -> Result<()>;
}

pub(crate) struct SimpleDecrypter {
    sk: SecretKey,
}

impl Decrypter for SimpleDecrypter {
    fn maybe_decrypt(&mut self, msg: &mut WireMessage) -> Result<()> {
        let mut m = serde_json::from_str::<GenericProtocolMessage>(&msg.payload).context("parse GenericProtocolMessage")?;
        match m.receiver {
            Some(_) => {
                let encrypted_message =
                    serde_json::from_str::<Encrypted>(m.body.to_string().as_ref()).context("parse encrypted")?;

                let decrypted =
                    PublicKeyGroup::decrypt(&self.sk, encrypted_message.encrypted.as_str()).context("decrypt")?;

                let decrypted = serde_json::value::RawValue::from_string(decrypted).context("convert to RawValue")?;
                m.body = decrypted;
                let m = serde_json::to_string(&m).context("convert to string")?;
                msg.payload = m;
                Ok(())
            }
            None => {
                Ok(())
            }
        }
    }
}

impl SimpleDecrypter {
    pub fn new(sk: SecretKey) -> Self {
        Self { sk }
    }
}
