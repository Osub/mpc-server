use curv::elliptic::curves::Secp256k1;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::LocalKey;
use serde::{Deserialize, Serialize};
use crate::core::PublicKeyGroup;
use anyhow::Result;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StoredLocalShare {
    pub public_keys: Vec<String>,
    pub own_public_key: String,
    pub share: LocalKey<Secp256k1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct EncryptedStoredLocalShare {
    pub public_keys: Vec<String>,
    pub own_public_key: String,
    pub encrypted_share: String,
}

pub trait GroupStore {
    fn retrieve_group(&mut self, group_id: String) -> Result<PublicKeyGroup>;
    fn save_group(&mut self, group: PublicKeyGroup) -> Result<()>;
}

pub(crate) trait LocalShareStore {
    fn save_local_share(&mut self, local_share: StoredLocalShare) -> Result<()>;
    fn retrieve_local_share(&mut self, public_key: String) -> Result<StoredLocalShare>;
}
