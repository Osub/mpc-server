use anyhow::{Context, Result};
use thiserror::Error;
use crate::core::{GroupStore, PublicKeyGroup};

pub struct SledDbGroupStore {
    db: sled::Db,
}

impl SledDbGroupStore {
    pub fn new(db: sled::Db) -> Self {
        Self { db }
    }
}

impl GroupStore for SledDbGroupStore {
    fn retrieve_group(&mut self, group_id: String) -> Result<PublicKeyGroup> {
        let group = self.db.get(group_id.as_bytes())?
            .ok_or_else(|| GroupStoreError::GroupNotFound(group_id.clone()))
            .context("Retrieving group.")?;
        let group = serde_json::from_slice::<PublicKeyGroup>(group.as_ref())
            .context("Decode group.")?;
        Ok(group)
    }

    fn save_group(&mut self, group: PublicKeyGroup) -> Result<()> {
        let value = serde_json::to_vec_pretty(&group).context("serialize local share")?;
        let key = group.get_group_id();
        let ov: Option<&[u8]> = None;
        let nv: Option<&[u8]> = Some(value.as_slice()); // TODO: Encrypt payload
        let _ = self.db.compare_and_swap(
            key.as_bytes(),      // key
            ov, // old value, None for not present
            nv, // new value, None for delete
        ).context("Save to db.")?;
        Ok(())
    }
}


#[derive(Debug, Error)]
enum GroupStoreError {
    #[error("Couldn't find the group of id {0}.")]
    GroupNotFound(String),
}
