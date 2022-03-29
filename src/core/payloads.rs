use either::Either;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct SignPayload {
    pub request_id: String,
    pub public_key: String,
    pub participant_public_keys: Vec<String>,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct KeygenPayload {
    pub request_id: String,
    pub public_keys: Vec<String>,
    pub t: u16,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResponsePayload {
    pub request_id: String,
    pub result: Option<String>,
    pub request_type: String,
    pub request_status: String,
}

pub type Payload = Either<KeygenPayload, SignPayload>;
