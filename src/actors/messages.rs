use actix::prelude::*;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use anyhow::{Result};
use serde_json::value::RawValue;
use crate::core::CoreMessage;
use crate::pb::types::WireMessage;
use crate::pb::mpc;

#[derive(Message)]
#[rtype(result = "()")]
pub struct MaybeProceed;


#[derive(Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub struct RetryMessage {
    pub initial_timestamp: u128,
    pub attempts: u16,
    pub check_passed: bool,
    pub message: WireMessage,
}

#[derive(Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub(crate) enum CoordinatorMessage {
    Incoming(WireMessage),
    Retry(RetryMessage),
    Outgoing(CoreMessage),
    SignRequest(mpc::SignRequest),
    KeygenRequest(mpc::KeygenRequest),
}

#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct IncomingMessage<M: Clone> {
    pub room: String,
    pub wire_message: WireMessage,
    pub message: M,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProtocolError<E, M> {
    pub room: String,
    pub error: E,
    pub message: M,
}

#[derive(Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub struct ProtocolOutput<I, O> {
    pub input: I,
    pub output: O,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GenericProtocolMessage {
    pub sender: u16,
    pub receiver: Option<u16>,
    pub body: Box<RawValue>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Encrypted {
    pub encrypted: String,
}
