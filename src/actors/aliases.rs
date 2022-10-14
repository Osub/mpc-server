use curv::elliptic::curves::Secp256k1;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::party_i::SignatureRecid;
pub(crate) use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::{Error as KeygenError, Keygen, LocalKey, ProtocolMessage as KeygenProtocolMessage};
pub(crate) use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{CompletedOfflineStage, Error as OfflineStageError, OfflineProtocolMessage, OfflineStage};
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::PartialSignature;
use round_based::{Msg, StateMachine};

use crate::actors::messages::{IncomingMessage, ProtocolError, ProtocolOutput};
use crate::actors::MpcPlayer;
use crate::actors::types::EnrichedSignRequest;
use crate::KeygenPayload;

pub(crate) type StateMachineKG = Keygen;
pub(crate) type StateMachineO = OfflineStage;
pub(crate) type ProtocolErrorO = ProtocolError<OfflineStageError, IncomingMessage<Msg<OfflineProtocolMessage>>>;
pub(crate) type ProtocolErrorKG = ProtocolError<KeygenError, IncomingMessage<Msg<KeygenProtocolMessage>>>;
pub(crate) type ProtocolOutputKG = ProtocolOutput<KeygenPayload, LocalKey<Secp256k1>>;
pub(crate) type ProtocolOutputO = ProtocolOutput<EnrichedSignRequest, CompletedOfflineStage>;
pub(crate) type ProtocolOutputS = ProtocolOutput<EnrichedSignRequest, SignatureRecid>;
pub(crate) type MpcPlayerKG = MpcPlayer<KeygenPayload, Keygen, <Keygen as StateMachine>::MessageBody, <Keygen as StateMachine>::Err, <Keygen as StateMachine>::Output>;
pub(crate) type MpcPlayerO = MpcPlayer<EnrichedSignRequest, OfflineStage, <OfflineStage as StateMachine>::MessageBody, <OfflineStage as StateMachine>::Err, <OfflineStage as StateMachine>::Output>;
pub(crate) type MsgS = Msg<PartialSignature>;
pub(crate) type MsgO = Msg<OfflineProtocolMessage>;
pub(crate) type MsgKG = Msg<KeygenProtocolMessage>;
