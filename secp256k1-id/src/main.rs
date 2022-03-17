use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use libsecp256k1::{Message, PublicKey, SecretKey, sign};
use rand::rngs::OsRng;
use sha2::{Sha256, Digest};
use structopt::StructOpt;
use curv::elliptic::curves::{Secp256k1};
use libsecp256k1::curve::Scalar;

#[derive(StructOpt, Debug)]
pub struct Cli {
    #[structopt(short, long)]
    pub secret_key_path: PathBuf,
    #[structopt(short, long)]
    pub public_key_path: PathBuf,
}

fn main() -> std::io::Result<()> {
    let secret_key = SecretKey::random(&mut OsRng);
    let public_key = PublicKey::from_secret_key(&secret_key);

    println!("secret key is: {:?}", hex::encode(secret_key.serialize()));
    println!("public key is: {:?}", hex::encode(public_key.serialize_compressed()));

    let args: Cli = Cli::from_args();
    let mut s_file = File::create(args.secret_key_path)?;
    s_file.write_all(hex::encode(secret_key.serialize()).as_bytes())?;

    let mut p_file = File::create(args.public_key_path)?;
    p_file.write_all(hex::encode(public_key.serialize_compressed()).as_bytes())?;

    Ok(())
}
