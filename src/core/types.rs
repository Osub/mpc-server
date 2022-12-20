use curv::elliptic::curves::Secp256k1;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::LocalKey;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StoredLocalShare {
    pub public_keys: Vec<String>,
    pub own_public_key: String,
    pub share: LocalKey<Secp256k1>,
}
