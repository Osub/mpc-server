mod signer;
mod player;
mod coordinator;
pub mod messages;

use actix::Addr;
pub use signer::*;
pub use player::*;
pub use coordinator::*;
use crate::core::{Payload, KeygenPayload, SignPayload};
use either::Either;
use messages::{KeygenRequest, SignRequest};

fn handle_keygen(coordinator: &Addr<Coordinator>, own_public_key: String, payload: KeygenPayload){
    let KeygenPayload { request_id, public_keys, t } = payload;
    coordinator.do_send(KeygenRequest {
        request_id,
        public_keys,
        t,
        own_public_key: own_public_key.clone(),
    });
}

fn handle_sign(coordinator: &Addr<Coordinator>, own_public_key: String, payload: SignPayload) {
    let SignPayload { request_id, message, public_key, participant_public_keys } = payload;
    coordinator.do_send(SignRequest {
        request_id,
        participant_public_keys,
        public_key,
        message,
        own_public_key: own_public_key.clone(),
    });
}

pub fn handle(coordinator: &Addr<Coordinator>, own_public_key: String, payload: Payload) {
    log::info!("Received request {:?}", payload);
    match payload {
        Either::Left(_) => {
            handle_keygen(coordinator, own_public_key, payload.left().unwrap());
        }
        Either::Right(_) => {
            handle_sign(coordinator, own_public_key, payload.right().unwrap());
        }
    }
}