use regex::Regex;
use sha2::Digest;
use serde::{Deserialize, Serialize};
use crate::actors::messages::GenericProtocolMessage;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct MsgDescriptor {
    pub sender: u16,
    pub receiver: Option<u16>,
    pub round: MessageRound,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) enum MessageRound {
    KG1,
    KG2,
    KG3,
    KG4,
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S7,
    UN,
}

pub fn describe_message(msg: &String) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(msg);
    let hash = hasher.finalize();

    let msg = serde_json::from_str::<GenericProtocolMessage>(msg);
    match msg {
        Ok(msg) => {
            let body = msg.body.to_string().replace('\\', "");

            let re = Regex::new(r#"^\{"(\w+)".*"#).unwrap();
            let body_round = re.captures(&body);
            match body_round {
                Some(round) => {
                    let round = match round.get(1).unwrap().as_str() {
                        "Round1" => { MessageRound::KG1 }
                        "Round2" => { MessageRound::KG2 }
                        "Round3" => { MessageRound::KG3 }
                        "Round4" => { MessageRound::KG4 }
                        "M1" => { MessageRound::S1 }
                        "M2" => { MessageRound::S2 }
                        "M3" => { MessageRound::S3 }
                        "M4" => { MessageRound::S4 }
                        "M5" => { MessageRound::S5 }
                        "M6" => { MessageRound::S6 }
                        "scalar" => { MessageRound::S7 }
                        "curve" => { MessageRound::S7 }
                        _ => { MessageRound::UN }
                    };
                    let desc = MsgDescriptor {
                        sender: msg.sender,
                        receiver: msg.receiver,
                        round,
                        hash: hex::encode(hash),
                    };
                    serde_json::to_string(&desc).unwrap()
                }
                None => { "".to_string() }
            }
        }
        Err(_) => { "".to_string() }
    }
}
