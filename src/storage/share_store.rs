use std::ops::Deref;
use thiserror::Error;
use curv::elliptic::curves::Secp256k1;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::LocalKey;
use secp256k1::{PublicKey, SecretKey};
use anyhow::{Context, Result};

use crate::core::{EncryptedStoredLocalShare, LocalShareStore, StoredLocalShare};
use crate::crypto::{SafeDecryptExt, SafeEncryptExt};

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
    fn save_local_share(&mut self, local_share: StoredLocalShare) -> Result<()> {
        let sum_pk_bytes = local_share.share.public_key().to_bytes(true);
        let sum_pk = hex::encode(sum_pk_bytes.deref());
        let ov: Option<&[u8]> = None;
        let share_bytes = serde_json::to_vec(&local_share.share).context("encoding share")?;

        let own_pk = hex::decode(&local_share.own_public_key).context("decode own public key")?;
        let own_pk = PublicKey::parse_slice(own_pk.as_slice(), None).context("parse public key")?;
        let ciphertext = own_pk.encrypt(share_bytes.as_slice()).context("encrypt share")?;
        let encrypted = EncryptedStoredLocalShare {
            public_keys: local_share.public_keys,
            own_public_key: local_share.own_public_key,
            encrypted_share: hex::encode(ciphertext),
        };

        let out = serde_json::to_vec(&encrypted).context("serialize local share")?;
        let nv: Option<&[u8]> = Some(out.as_slice());
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
        let encrypted = serde_json::from_slice::<EncryptedStoredLocalShare>(local_share.as_ref())
            .context("Decode local share.")?;
        let encrypted_share = hex::decode(encrypted.encrypted_share)
            .context("decode share")?;
        let decrypted = self.sk.decrypt(encrypted_share.as_slice())
            .context("decrypt share")?;
        let lk = serde_json::from_slice::<LocalKey<Secp256k1>>(decrypted.as_slice())
            .context("deserialize local key")?;
        let local_share = StoredLocalShare{
            public_keys: encrypted.public_keys,
            own_public_key:encrypted.own_public_key,
            share: lk,
        };
        Ok(local_share)
    }
}


#[derive(Debug, Error)]
enum LocalShareStoreError {
    #[error("Couldn't find the local share for {0}.")]
    LocalShareNotFound(String),
}
