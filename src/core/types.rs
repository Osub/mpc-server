use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct CoreMessage {
    pub room: String,
    pub message: String,
}
