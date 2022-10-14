
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignPayload {
    pub request_id: String,
    pub public_key: String,
    pub participant_public_keys: Vec<String>,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeygenPayload {
    pub request_id: String,
    pub public_keys: Vec<String>,
    pub t: u16,
}

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum RequestType {
    KEYGEN,
    SIGN,
}

#[allow(non_camel_case_types)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum RequestStatus {
    RECEIVED,
    PROCESSING,
    OFFLINE_STAGE_DONE,
    DONE,
    ERROR
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ResponsePayload {
    pub request_id: String,
    pub result: Option<String>,
    pub request_type: RequestType,
    pub request_status: RequestStatus,
}
