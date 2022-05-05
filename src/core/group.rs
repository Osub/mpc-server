use round_based::Msg;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub trait MpcGroup {
    fn valid_msg<T>(&self, sender_public_key: &String, msg: &Msg<T>) -> bool;
    fn get_i(&self) -> u16;
    fn get_t(&self) -> u16;
    fn get_n(&self) -> u16;
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PublicKeyGroup {
    group_id: String,
    t: u16,
    public_keys: Vec<String>,
    own_public_key: String,
}

impl PublicKeyGroup {
    pub fn new(public_keys: Vec<String>, t: u16, own_public_key: String) -> Self {
        let mut hasher = Sha256::new();
        for pk in public_keys.iter() {
            let pk = hex::decode(pk).unwrap();
            hasher.update(pk.as_slice());
        }
        let hash = hasher.finalize();
        let hbytes: [u8; 32] = hash.as_slice().try_into().unwrap();
        let group_id = hex::encode(hbytes);
        Self {
            group_id,
            public_keys,
            t,
            own_public_key,
        }
    }
    pub fn get_group_id(&self) -> String {
        self.group_id.clone()
    }

    pub fn get_index(&self, public_key: &String) -> Option<usize> {
        self.public_keys.iter().position(|pk| *pk == *public_key).map(|i| i + 1)
    }
}

impl MpcGroup for PublicKeyGroup {
    fn valid_msg<T>(&self, sender_public_key: &String, msg: &Msg<T>) -> bool {
        let ind = self.get_index(sender_public_key);
        // ind.map_or(false, |i| i == (msg.sender as usize))
        // TODO: How do we ensure claimed index is correct?
        ind.is_some()
    }

    fn get_i(&self) -> u16 {
        self.get_index(&self.own_public_key).unwrap() as u16
    }

    fn get_t(&self) -> u16 {
        self.t
    }

    fn get_n(&self) -> u16 {
        self.public_keys.len() as u16
    }
}
