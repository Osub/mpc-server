mod signer;
mod player;
mod coordinator;
pub mod messages;
mod msg_utils;

use actix::Addr;
pub use signer::*;
pub use player::*;
pub use coordinator::*;
use crate::core::{Payload, KeygenPayload, SignPayload};
use either::Either;
use messages::{KeygenRequest, SignRequest};

use kv_log_macro as log;

fn handle_keygen(coordinator: &Addr<Coordinator>, own_public_key: String, payload: KeygenPayload){
    let KeygenPayload { request_id, public_keys, t } = payload;
    coordinator.do_send(KeygenRequest {
        request_id,
        public_keys,
        t,
        own_public_key: own_public_key.clone(),
    });
}

async fn handle_sign(coordinator: &Addr<Coordinator>, own_public_key: String, payload: SignPayload) {
    let SignPayload { request_id, message, public_key, participant_public_keys } = payload;
    let res = coordinator.send(SignRequest {
        request_id: request_id.clone(),
        participant_public_keys,
        public_key,
        message,
        own_public_key: own_public_key.clone(),
    }).await;
    match res {
        Ok(res) => {
            match res {
                Ok(_) => {
                    log::info!("Request sent {:}", request_id);
                }
                Err(e) => {
                    log::error!("Failed send {:}: {:}", request_id, e);
                }
            }
        }
        Err(e) => {
            log::error!("Failed send {:}: {:}", request_id, e);
        }
    }
}

pub async fn handle(coordinator: &Addr<Coordinator>, own_public_key: String, payload: Payload) {
    match payload {
        Either::Left(payload) => {
            log::info!("Received request", { payload: serde_json::to_string(&payload).unwrap()});
            handle_keygen(coordinator, own_public_key, payload);
        }
        Either::Right(payload) => {
            log::info!("Received request", { payload: serde_json::to_string(&payload).unwrap()});
            handle_sign(coordinator, own_public_key, payload).await;
        }
    }
}