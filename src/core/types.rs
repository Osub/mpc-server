use serde::{Deserialize, Serialize};
use crate::api::{KeygenPayload, SignPayload};

#[derive(Serialize, Deserialize, Clone)]
pub struct CoreMessage {
    pub room: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Request {
    Keygen(KeygenPayload),
    Sign(SignPayload),
}
