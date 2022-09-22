use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct WireMessage {
    pub room: String,
    pub message: String,
    pub sender_public_key: String,
    pub signature: String,
}
