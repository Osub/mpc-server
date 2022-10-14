use std::ops::Deref;
use thiserror::Error;
use anyhow::{Context, Result};

use crate::core::StoredLocalShare;

pub trait LocalShareStore {
    fn save_local_share(&mut self, local_share: StoredLocalShare) -> Result<()>;
    fn retrieve_local_share(&mut self, public_key: String) -> Result<StoredLocalShare>;
}

pub struct SledDbLocalShareStore {
    db: sled::Db,
}

impl SledDbLocalShareStore {
    pub fn new(db: sled::Db) -> Self {
        Self { db }
    }
}


impl LocalShareStore for SledDbLocalShareStore {
    fn save_local_share(&mut self, local_share: StoredLocalShare) -> Result<()> {
        let out = serde_json::to_vec_pretty(&local_share).context("serialize local share")?;
        let sum_pk_bytes = local_share.share.public_key().to_bytes(true);
        let sum_pk = hex::encode(sum_pk_bytes.deref());
        let ov: Option<&[u8]> = None;
        let nv: Option<&[u8]> = Some(out.as_slice()); // TODO: Encrypt payload
        let _ = self.db.compare_and_swap(
            sum_pk.as_bytes(),      // key
            ov, // old value, None for not present
            nv, // new value, None for delete
        ).context("Save to db.")?;
        Ok(())
    }
    fn retrieve_local_share(&mut self, public_key: String) -> Result<StoredLocalShare> {
        let local_share = self.db.get(public_key.as_bytes())?
            .ok_or_else(|| LocalShareStoreError::LocalShareNotFound(public_key.clone()))
            .context("Retrieving local share.")?;
        let local_share = serde_json::from_slice::<StoredLocalShare>(local_share.as_ref())
            .context("Decode local share.")?;
        Ok(local_share)
    }
}


#[derive(Debug, Error)]
enum LocalShareStoreError {
    #[error("Couldn't find the local share for {0}.")]
    LocalShareNotFound(String),
}
