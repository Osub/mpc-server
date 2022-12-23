use std::ops::Deref;
use thiserror::Error;
use secp256k1::SecretKey;
use anyhow::{Context, Result};

use crate::core::{EncryptedStoredLocalShare, LocalShareStore, StoredLocalShare};

pub(crate) struct SledDbLocalShareStore {
    sk: SecretKey,
    db: sled::Db,
}

impl SledDbLocalShareStore {
    pub(crate) fn new(sk: SecretKey, db: sled::Db) -> Self
    {
        Self { sk, db }
    }
}

impl LocalShareStore for SledDbLocalShareStore {
    fn save(&mut self, local_share: StoredLocalShare) -> Result<()> {
        let sum_pk_bytes = local_share.share.public_key().to_bytes(true);
        let sum_pk = hex::encode(sum_pk_bytes.deref());
        let ov: Option<&[u8]> = None;

        let encrypted_share = local_share.encrypt().context("encrypt share")?;
        let out = serde_json::to_vec(&encrypted_share).context("serialize local share")?;
        let nv: Option<&[u8]> = Some(out.as_slice());
        let _ = self.db.compare_and_swap(
            sum_pk.as_bytes(),      // key
            ov, // old value, None for not present
            nv, // new value, None for delete
        ).context("Save to db.")?;
        Ok(())
    }
    fn retrieve(&mut self, public_key: String) -> Result<StoredLocalShare> {
        let stored = self.db.get(public_key.as_bytes())?
            .ok_or_else(|| LocalShareStoreError::LocalShareNotFound(public_key.clone()))
            .context("Retrieving local share.")?;
        let encrypted = serde_json::from_slice::<EncryptedStoredLocalShare>(stored.as_ref())
            .context("Decode local share.")?;
        let local_share= encrypted.decrypt(&self.sk).context("decrypt stored share")?;
        Ok(local_share)
    }
}


#[derive(Debug, Error)]
enum LocalShareStoreError {
    #[error("Couldn't find the local share for {0}.")]
    LocalShareNotFound(String),
}
