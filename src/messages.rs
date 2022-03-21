
use std::collections::HashMap;
use actix::prelude::*;
use actix_web::{get, web, http, middleware, App, HttpServer, Responder, HttpRequest, HttpResponse};
use curv::elliptic::curves::Secp256k1;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::LocalKey;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::OfflineStage;
use work_queue::{LocalQueue, Queue};
use serde::{Serialize, Deserialize};
use anyhow::{Result};

#[derive(Message)]
#[rtype(result = "()")]
pub struct MaybeProceed;

#[derive(Message, Serialize, Deserialize, Clone, Debug)]
#[rtype(result = "Result<()>")]
pub struct KeygenRequest {
    pub public_keys: Vec<String>,
    pub t: u16,
    pub own_public_key: String,
}

#[derive(Message, Serialize, Deserialize, Clone, Debug)]
#[rtype(result = "Result<()>")]
pub struct SignRequest {
    pub message: String,
    pub room: String,
    pub i: u16,
    pub s_l: Vec<u16>,
    pub local_key: LocalKey<Secp256k1>,
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
