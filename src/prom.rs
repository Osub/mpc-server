use prometheus::{self, IntCounter};

use prometheus::register_int_counter;
use structopt::lazy_static::lazy_static;

lazy_static! {
    pub static ref COUNTER_REQUESTS_SIGN_RECEIVED: IntCounter =
        register_int_counter!("requests_sign_received", "Number of sign requests received").unwrap();
    pub static ref COUNTER_REQUESTS_SIGN_DONE: IntCounter =
        register_int_counter!("requests_sign_done", "Number of sign requests done").unwrap();
    pub static ref COUNTER_REQUESTS_KEYGEN_RECEIVED: IntCounter =
        register_int_counter!("requests_keygen_received", "Number of keygen requests received").unwrap();
    pub static ref COUNTER_REQUESTS_KEYGEN_DONE: IntCounter =
        register_int_counter!("requests_keygen_done", "Number of keygen requests done").unwrap();
    pub static ref COUNTER_MESSAGES_TOTAL_RECEIVED: IntCounter =
        register_int_counter!("messages_total_received", "Number of messages received").unwrap();
    pub static ref COUNTER_MESSAGES_INVALID: IntCounter =
        register_int_counter!("messages_invalid", "Number of messages received that are invalid").unwrap();
    pub static ref COUNTER_MESSAGES_INVALID_ENCRYPTION: IntCounter =
        register_int_counter!("messages_invalid_encryption", "Number of messages received that have invalid encrypted payload").unwrap();
    pub static ref COUNTER_MESSAGES_KEYGEN_HANDLED: IntCounter =
        register_int_counter!("messages_keygen_handled", "Number of messages handled by keygen protocol").unwrap();
    pub static ref COUNTER_MESSAGES_OFFLINE_HANDLED: IntCounter =
        register_int_counter!("messages_offline_handled", "Number of messages handled by offline stage").unwrap();
    pub static ref COUNTER_MESSAGES_SIGN_HANDLED: IntCounter =
        register_int_counter!("messages_sign_handled", "Number of messages handled by sign protocol").unwrap();
    pub static ref COUNTER_MESSAGES_DROPPED: IntCounter =
        register_int_counter!("messages_total_dropped", "Number of messages dropped").unwrap();
}
