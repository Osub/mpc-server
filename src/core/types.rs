use curv::elliptic::curves::Secp256k1;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::LocalKey;
use serde::{Deserialize, Serialize};
use crate::core::PublicKeyGroup;
use anyhow::{Context, Result};
use secp256k1::{PublicKey, SecretKey};
use zeroize::Zeroize;
use crate::crypto::{SafeDecryptExt, SafeEncryptExt};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StoredLocalShare {
    pub public_keys: Vec<String>,
    pub own_public_key: String,
    pub share: LocalKey<Secp256k1>,
}

impl StoredLocalShare {
    fn get_own_public_key(&self) -> Result<PublicKey> {
        let own_pk = hex::decode(&self.own_public_key).context("decode own public key")?;
        let own_pk = PublicKey::parse_slice(own_pk.as_slice(), None).context("parse public key")?;
        Ok(own_pk)
    }
    pub(crate) fn encrypt(&self) -> Result<EncryptedStoredLocalShare> {
        let pk = self.get_own_public_key().context("get own public key")?;
        let share_bytes = serde_json::to_vec(&self.share).context("encoding share")?;
        let ciphertext = pk.encrypt(share_bytes.as_slice()).context("encrypt share")?;
        let encrypted = EncryptedStoredLocalShare {
            public_keys: self.public_keys.clone(),
            own_public_key: self.own_public_key.clone(),
            encrypted_share: hex::encode(ciphertext),
        };
        Ok(encrypted)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct EncryptedStoredLocalShare {
    pub public_keys: Vec<String>,
    pub own_public_key: String,
    pub encrypted_share: String,
}

impl EncryptedStoredLocalShare {
    pub(crate) fn decrypt(&self, sk: &SecretKey) -> Result<StoredLocalShare> {
        let ciphertext = hex::decode(&self.encrypted_share)
            .context("decode share")?;
        let mut plaintext = sk.decrypt(ciphertext.as_slice())
            .context("decrypt share")?;
        let local_key_secret = serde_json::from_slice::<LocalKey<Secp256k1>>(plaintext.as_slice())
            .context("deserialize local key")?;
        plaintext.zeroize(); // to avoid secret leak
        Ok(StoredLocalShare {
            public_keys: self.public_keys.clone(),
            own_public_key: self.own_public_key.clone(),
            share: local_key_secret,
        })
    }
}

pub trait GroupStore {
    fn retrieve_group(&mut self, group_id: String) -> Result<PublicKeyGroup>;
    fn save_group(&mut self, group: PublicKeyGroup) -> Result<()>;
}

pub(crate) trait LocalShareStore {
    fn save(&mut self, local_share: StoredLocalShare) -> Result<()>;
    fn retrieve(&mut self, public_key: String) -> Result<StoredLocalShare>;
}
