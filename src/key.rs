use aes::Aes256;
use aes::cipher::{AsyncStreamCipher, IvSizeUser, KeyIvInit};
use aes::cipher::Unsigned;
use anyhow::Result;
use cfb_mode::Decryptor;
use sha2::{Digest, Sha256};

type Aes256CfbDec = Decryptor<Aes256>;

pub fn decrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let hey_bytes: &[u8] = &Sha256::digest(key);
    let iv_size = <Aes256CfbDec as IvSizeUser>::IvSize::to_usize();
    let iv: Vec<u8> = data.iter().take(iv_size).copied().collect::<Vec<u8>>();
    let mut buf: Vec<u8> = data.iter().skip(iv_size).copied().collect::<Vec<u8>>();
    let dec = Aes256CfbDec::new_from_slices(hey_bytes, &iv)?;
    dec.decrypt(&mut buf);

    Ok(buf[iv_size..].to_vec())
}

#[test]
fn test_decrypt() {
    let key = b"RBuCJbmWY1Mtcl5LoMRqkQQpT5GJmCEvbuRR7ewCPDATBzFtm9a6jhIovftgddmL";
    let cipher = hex::decode("f6826fc16547130848ea32196c95457b4698feded1a8f109eb224ddcd27d66af7d0f88b44d2765a4a567a8999a4410852510deacdcb87e0c7cfe23fa1d0090a8").unwrap();
    let plaintext = decrypt(key, cipher.as_slice()).unwrap();
    assert_eq!(hex::decode("59d1c6956f08477262c9e827239457584299cf583027a27c1d472087e8c35f21").unwrap(), plaintext);
}
