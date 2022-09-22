
use serde::{Deserialize, Serialize};

use curv::BigInt;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::CompletedOfflineStage;
use crate::SignPayload;

pub(crate) struct SignTask {
    pub room: String,
    pub index: u16,
    pub t: usize,
    pub message: BigInt,
    pub completed_offline_stage: CompletedOfflineStage,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EnrichedSignRequest {
    pub inner: SignPayload,
    pub room: String,
    pub i: u16,
    pub s_l: Vec<u16>,
}
