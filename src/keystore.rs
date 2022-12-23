use std::path::PathBuf;
use anyhow::{Context, Result};
use secp256k1::{PublicKey, SecretKey};
use eth_keystore::decrypt_key;

use kv_log_macro as log;

pub(crate) fn get_secret_key(keystore_path: PathBuf, password_path: PathBuf) -> Result<(SecretKey, PublicKey)> {
    let password = std::fs::read_to_string(password_path).context("read password file")?;
    let password = password.trim();
    let sk = decrypt_key(&keystore_path, password).context("decrypt keystore")?;
    let sk = SecretKey::parse_slice(sk.as_slice()).context("parse secret key")?;
    let pk = PublicKey::from_secret_key(&sk);
    Ok((sk, pk))
}