
use std::{thread, time};
use std::collections::HashMap;
use actix::prelude::*;
use actix_web::{get, web, http, middleware, App, HttpServer, Responder, HttpRequest, HttpResponse};
use curv::elliptic::curves::Secp256k1;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::LocalKey;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::OfflineStage;
use work_queue::{LocalQueue, Queue};
use serde::{Serialize, Deserialize};

#[derive(Message)]
#[rtype(result = "()")]
pub struct KeygenRequest {
    pub i: u16,
    pub t: u16,
    pub n: u16,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SignRequest {
    pub room: String,
    pub i: u16,
    pub s_l: Vec<u16>,
    pub local_key: LocalKey<Secp256k1>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProtocolMessage<M> {
    pub task_id: String,
    pub message: M,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProtocolMessageAck {
    pub task_id: String,
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

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProtocolOutput<M> {
    pub output: M,
}
