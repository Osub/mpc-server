use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use libsecp256k1::{Message, PublicKey, SecretKey, sign};
use rand::rngs::OsRng;
use sha2::{Sha256, Digest};
use structopt::StructOpt;
// use curv::cryptographic_primitives::hashing::{Digest, DigestExt};
use curv::elliptic::curves::{Secp256k1};
use libsecp256k1::curve::Scalar;
// use rand::rngs::OsRng;
// use secp256k1::rand::rngs::OsRng;
// use secp256k1::{Message, Secp256k1};
// use secp256k1::hashes::sha256;

#[derive(StructOpt, Debug)]
pub struct Cli {
    #[structopt(short, long)]
    pub secret_key_path: PathBuf,
    #[structopt(short, long)]
    pub public_key_path: PathBuf,
}

fn main() -> std::io::Result<()> {
    // let mut rng = OsRng::new().expect("OsRng");
    // let mut rng = OsRng::
    // let (secret_key, public_key) = secp.generate_keypair(&mut rng);
    let secret_key = SecretKey::random(&mut OsRng);
    let public_key = PublicKey::from_secret_key(&secret_key);

    println!("secret key is: {:?}", hex::encode(secret_key.serialize()));
    println!("public key is: {:?}", hex::encode(public_key.serialize_compressed()));

    // let args: Cli = Cli::from_args();
    // let mut s_file = File::create(args.secret_key_path)?;
    // s_file.write_all(secret_key.to_string().as_bytes())?;
    //
    // let mut p_file = File::create(args.public_key_path)?;
    // p_file.write_all(public_key().as_bytes())?;

    let mut hasher = Sha256::new();
    hasher.update(
        b"abc"
    );
    let result = hasher.finalize();
    let mut scalar = Scalar::default();
    scalar.set_b32(result.as_ref());
    let (sig, recId) = sign(&Message(scalar), &secret_key);
    println!("sig r {:?}", hex::encode(sig.r.b32()));
    println!("sig s {:?}", hex::encode(sig.s.b32()));
    println!("sig rec {:?}", hex::encode([recId.serialize()]));

    Ok(())
}
