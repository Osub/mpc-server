use anyhow::{Context, Result};

use crate::actors::messages::{Encrypted, GenericProtocolMessage};
use crate::core::GroupStore;
use crate::pb::types::CoreMessage;
use crate::storage::SledDbGroupStore;
use crate::utils::extract_group_id;

pub(crate) trait Encrypter {
    fn maybe_encrypt(&mut self, msg: &mut CoreMessage) -> Result<()>;
}

pub(crate) struct SimpleEncrypter {
    group_store: Box<dyn GroupStore>,
}

impl SimpleEncrypter {
    pub fn new(db: sled::Db) -> Self {
        let group_store = Box::new( SledDbGroupStore::new(db));
        Self { group_store }
    }
}

impl Encrypter for SimpleEncrypter {
    fn maybe_encrypt(&mut self, msg: &mut CoreMessage) -> Result<()> {
        let mut m = serde_json::from_str::<GenericProtocolMessage>(&msg.message).context("parse GenericProtocolMessage")?;
        match m.receiver {
            Some(receiver) => {
                let group_id = extract_group_id(&msg.room).context("extract group_id")?;
                let group = self.group_store.retrieve_group(group_id.to_string()).context("Retrieve group.")?;
                let encrypted = group.encrypt(receiver as usize, m.body.to_string().as_str()).context("encrypt")?;
                let encrypted_message = Encrypted {
                    encrypted,
                };
                let body = serde_json::to_string(&encrypted_message).context("stringify Encrypted")?;
                let body = serde_json::value::RawValue::from_string(body).context("convert to RawValue")?;
                m.body = body;
                let m = serde_json::to_string(&m).context("convert to string")?;
                msg.message = m;
                Ok(())
            }
            None => {
                Ok(())
            }
        }
    }
}