use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct WireMessage<S> {
    pub room: String,
    pub message: String,
    pub sender_public_key: String,
    pub signature: S,
}
