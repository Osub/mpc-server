use curv::BigInt;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::CompletedOfflineStage;

pub(crate) struct SignTask {
    pub room: String,
    pub index: u16,
    pub t: usize,
    pub message: BigInt,
    pub completed_offline_stage: CompletedOfflineStage,
}