use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct WireMessage {
    pub room: String,
    pub payload: String,
    pub sender_public_key: String,
    pub signature: String,
}
