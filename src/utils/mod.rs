use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Result};
use thiserror::Error;

pub(crate) fn current_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards").as_millis()
}

const ROOM_SEPARATOR: char = '@';

pub(crate) fn extract_group_id(room: &str) -> Result<&str> {
    let split = room.split(ROOM_SEPARATOR).collect::<Vec<&str>>();
    if split.len() < 2 {
        return Err(RoomParseError::FailedToParseGroupId.into());
    }
    Ok(split[1])
}

pub(crate) fn make_room_id(request_id:&str, group_id : &str) -> String {
let mut s =     String::from(request_id);
    s.push(ROOM_SEPARATOR);
    s.push_str(group_id);
    s
}

#[derive(Debug, Error)]
enum RoomParseError {
    #[error("Cannot parse the group id.")]
    FailedToParseGroupId,
}
