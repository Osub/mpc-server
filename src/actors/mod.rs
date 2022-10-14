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

pub async fn handle(coordinator: &Addr<Coordinator>, request: Request) {
    match request {
        Request::Keygen(request) => {
            let j_str = serde_json::to_string(&request).unwrap_or("".to_string());
            log::info!("Received request", { request: j_str });
            coordinator.do_send(CoordinatorMessage::KeygenRequest(request));
        }
        Request::Sign(request) => {
            let j_str = serde_json::to_string(&request).unwrap_or("".to_string());
            log::info!("Received request", { request: j_str});
            coordinator.do_send(CoordinatorMessage::SignRequest(request));
        }
    }
}