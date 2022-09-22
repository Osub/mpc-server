mod signer;
mod player;
mod coordinator;
pub mod messages;
mod msg_utils;
mod types;
mod aliases;

use actix::Addr;
pub use signer::*;
pub use player::*;
pub use coordinator::*;
use crate::core::Request;

use kv_log_macro as log;
use crate::actors::messages::CoordinatorMessage;
use crate::api::{KeygenPayload, SignPayload};

fn handle_keygen(coordinator: &Addr<Coordinator>, payload: KeygenPayload){
    coordinator.do_send(CoordinatorMessage::KeygenRequest(payload));
}

async fn handle_sign(coordinator: &Addr<Coordinator>, payload: SignPayload) {
    coordinator.do_send(CoordinatorMessage::SignRequest (payload));
}

pub async fn handle(coordinator: &Addr<Coordinator>, request: Request) {
    match request {
        Request::Keygen(request) => {
            log::info!("Received request", { request: serde_json::to_string(&request).unwrap()});
            handle_keygen(coordinator, request);
        }
        Request::Sign(request) => {
            log::info!("Received request", { request: serde_json::to_string(&request).unwrap()});
            handle_sign(coordinator, request).await;
        }
    }
}