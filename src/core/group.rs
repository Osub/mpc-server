extern crate base64;

use anyhow::Context;
#[allow(unused_imports)]
use anyhow::Result;
use ecies::{encrypt, decrypt};
use round_based::Msg;
use serde::{Deserialize, Serialize};

use thiserror::Error;

pub trait MpcGroup {
    fn valid_msg<T>(&self, sender_public_key: &str, msg: &Msg<T>) -> bool;
    fn get_i(&self) -> u16;
    fn get_t(&self) -> u16;
    fn get_n(&self) -> u16;
}

#[derive(Debug, Error)]
enum GroupError {
    #[error("The index is not in the range [1, n].")]
    InvalidIndex,
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
        let mut group_id: String = "".to_owned();

        for pk in public_keys.iter() {
            group_id.push_str(&pk[2..10]);
        }
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

    pub fn get_public_key(&self, index: usize) -> Result<String> {
        if index == 0 || index > self.public_keys.len() {
            return Err(GroupError::InvalidIndex.into());
        }
        Ok(self.public_keys[index - 1].clone())
    }

    pub fn encrypt(&self, index: usize, plaintext: &str) -> Result<String> {
        let pk = self.get_public_key(index)?;
        let pk = hex::decode(pk)?;
        let ciphertext = encrypt(pk.as_slice(), plaintext.as_bytes())?;
        Ok(base64::encode(ciphertext))
    }

    pub fn decrypt(sk:&[u8], ciphertext: &str) -> Result<String> {
        let ciphertext = base64::decode(ciphertext).context("decode base64 ciphertext")?;
        let plaintext = decrypt(sk, ciphertext.as_slice()).context("decrypt")?;
        let plaintext = std::str::from_utf8(plaintext.as_slice()).context("decode utf8")?;
        Ok(plaintext.to_string())
    }

    pub fn get_index(&self, public_key: &str) -> Option<usize> {
        self.public_keys.iter().position(|pk| *pk == *public_key).map(|i| i + 1)
    }
}

impl MpcGroup for PublicKeyGroup {
    fn valid_msg<T>(&self, sender_public_key: &str, _msg: &Msg<T>) -> bool {
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
