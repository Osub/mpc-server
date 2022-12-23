use std::fmt;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use anyhow::{Context, Result};
use secp256k1::{PublicKey, SecretKey};
use eth_keystore::decrypt_key;
use thiserror::Error;

#[derive(Debug, Error)]
enum KeystoreError {
    #[error("the file mode is too broad")]
    FileModeTooBroad,
}

pub(crate) fn get_secret_key(keystore_path: PathBuf, password_path: PathBuf) -> Result<(SecretKey, PublicKey)> {
    let metadata = password_path.metadata().context("find file")?;
    (if (metadata.mode() & 0o777) != 0o400 {
         Err(KeystoreError::FileModeTooBroad)
    } else {
        Ok(())
    }).context(format!("password file mode is not 0o400: {:?}", metadata.mode()))?;
    let password = std::fs::read_to_string(password_path).context("read password file")?;
    let password = password.trim();
    let sk = decrypt_key(&keystore_path, password).context("decrypt keystore")?;
    let sk = SecretKey::parse_slice(sk.as_slice()).context("parse secret key")?;
    let pk = PublicKey::from_secret_key(&sk);
    Ok((sk, pk))
}