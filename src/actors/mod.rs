mod signer;
mod player;
mod coordinator;
pub mod messages;
mod msg_utils;
mod types;
mod aliases;

use actix::Addr;
pub use signer::*;
pub(crate) use player::*;
pub use coordinator::*;
use crate::pb::types::request::Request;

use kv_log_macro as log;
use crate::actors::messages::CoordinatorMessage;

use crate::prom;

pub async fn handle(coordinator: &Addr<Coordinator>, request: Request) {
    match request {
        Request::Keygen(request) => {
            let j_str = serde_json::to_string(&request).unwrap_or("".to_string());
            log::info!("Received request", { request: j_str });
            prom::COUNTER_REQUESTS_KEYGEN_RECEIVED.inc();
            coordinator.do_send(CoordinatorMessage::KeygenRequest(request));
        }
        Request::Sign(request) => {
            let j_str = serde_json::to_string(&request).unwrap_or("".to_string());
            log::info!("Received request", { request: j_str});
            prom::COUNTER_REQUESTS_SIGN_RECEIVED.inc();
            coordinator.do_send(CoordinatorMessage::SignRequest(request));
        }
    }
}