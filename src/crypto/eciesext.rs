use ecies::{decrypt, encrypt, SecpError};
use secp256k1::{PublicKey, SecretKey};
use zeroize::Zeroize;

pub trait SafeDecryptExt {
    fn decrypt(&self, msg: &[u8]) -> Result<Vec<u8>, SecpError>;
}

pub trait SafeEncryptExt {
    fn encrypt(&self, msg: &[u8]) -> Result<Vec<u8>, SecpError>;
}

impl SafeDecryptExt for SecretKey {
    fn decrypt(&self, msg: &[u8]) -> Result<Vec<u8>, SecpError> {
        let mut sk = self.serialize();
        let res = decrypt(sk.as_slice(), msg);
        sk.zeroize(); // zeroize to prevent secret leakage
        return res;
    }
}

impl SafeEncryptExt for PublicKey {
    fn encrypt(&self, msg: &[u8]) -> Result<Vec<u8>, SecpError> {
        let mut pk = self.serialize();
        let res = encrypt(pk.as_slice(), msg);
        pk.zeroize();
        return res;
    }
}