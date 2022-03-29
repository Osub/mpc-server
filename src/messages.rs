use actix::prelude::*;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use anyhow::{Result};

#[derive(Message)]
#[rtype(result = "()")]
pub struct MaybeProceed;

#[derive(Message, Serialize, Deserialize, Clone, Debug)]
#[rtype(result = "Result<()>")]
pub struct KeygenRequest {
    pub request_id: String,
    pub public_keys: Vec<String>,
    pub t: u16,
    pub own_public_key: String,
}

#[derive(Message, Serialize, Deserialize, Clone, Debug)]
#[rtype(result = "Result<()>")]
pub struct SignRequest {
    pub request_id: String,
    pub public_key: String,
    pub participant_public_keys: Vec<String>,
    pub message: String,
    pub own_public_key: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EnrichedSignRequest {
    pub inner: SignRequest,
    pub group_id: String,
    pub i: u16,
    pub s_l: Vec<u16>,
}

#[derive(Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub struct Envelope {
    pub room: String,
    pub message: String,
}

#[derive(Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub struct EcdsaSignature {
    pub r: String,
    pub s: String,
}

#[derive(Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub struct SignedEnvelope<S> {
    pub room: String,
    pub message: String,
    pub sender_public_key: String,
    pub signature: S,
}

#[derive(Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub struct RetryEnvelope {
    pub room: String,
    pub message: String,
    pub sender_public_key: String,
}

#[derive(Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub struct IncomingEnvelope {
    pub room: String,
    pub message: String,
    pub sender_public_key: String,
}

#[derive(Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub struct OutgoingEnvelope {
    pub room: String,
    pub message: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProtocolMessage<M> {
    pub room: String,
    pub message: M,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProtocolMessageAck {
    pub room: String,
    pub message_id: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct IncomingMessage<M> {
    pub room: String,
    pub message: M,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct OutgoingMessage<M> {
    pub room: String,
    pub message: M,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProtocolError<M> {
    pub error: M,
}

#[derive(Message, Serialize, Deserialize, Clone)]
#[rtype(result = "()")]
pub struct ProtocolOutput<I, O> {
    pub input: I,
    pub output: O,
}
