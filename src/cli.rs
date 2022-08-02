use std::net::IpAddr;
use std::path::PathBuf;

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct Cli {
    #[structopt(short, long, default_value = "http://localhost:8000/")]
    pub messenger_address: surf::Url,

    #[structopt(short, long, default_value = "127.0.0.1")]
    pub address: IpAddr,

    #[structopt(short, long, default_value = "8080")]
    pub port: u16,

    #[structopt(short, long)]
    pub secret_key_path: PathBuf,

    #[structopt(long)]
    pub password: String,

    #[structopt(short, long)]
    pub db_path: PathBuf,
}
