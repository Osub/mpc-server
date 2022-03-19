mod signer;
mod player;
mod coordinator;
mod messages;
mod cli;
mod transport;

extern crate env_logger;

#[macro_use]
extern crate log;

use std::borrow::Borrow;
use std::rc::Rc;
use std::sync::Arc;
use std::{thread, time};
use std::path::PathBuf;
use actix::prelude::*;
use actix_web::{get, web, http, middleware, App, HttpServer, Responder, HttpRequest, HttpResponse, Error};
use curv::elliptic::curves::Secp256k1;
use either::Either;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::LocalKey;
use secp256k1::SecretKey;
use work_queue::{LocalQueue, Queue};
use serde::{Serialize, Deserialize};
use crate::coordinator::Coordinator;
use crate::messages::{IncomingEnvelope, KeygenRequest, SignRequest};
use cli::Cli;
use structopt::StructOpt;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use crate::transport::join_computation;
use anyhow::{Context, Result};

#[derive(Serialize, Deserialize, Debug)]
struct SignPayload {
    message: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct KeygenPayload {
    i: u16,
    t: u16,
    n: u16,
}

type Payload = Either<KeygenPayload, SignPayload>;

struct AppState {
    tx: UnboundedSender<Payload>,
    i: u16,
    s_l: Vec<u16>,
    local_key: LocalKey<Secp256k1>,
}

struct Task(Box<dyn Fn(&mut LocalQueue<Task>) + Send>);


async fn keygen(data: web::Data<AppState>, req: web::Json<KeygenPayload>) -> impl Responder {
    log::info!("handling");
    data.tx.send(Either::Left(req.0));
    format!("Hi there!")
}


async fn sign(data: web::Data<AppState>, req: web::Json<SignPayload>) -> impl Responder {
    log::info!("handling");
    data.tx.send(Either::Right(req.0));
    format!("Hi there!")
}

fn get_secret_key(path: PathBuf) -> Result<SecretKey> {
    let sk_hex = std::fs::read_to_string(path)?;
    let sk_bytes = hex::decode(sk_hex)?;

    let sk = SecretKey::parse_slice(&sk_bytes)?;
    Ok(sk)
}

fn main() -> std::io::Result<()> {
    ::std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let args: Cli = Cli::from_args();
    let (tx, mut rx) = unbounded_channel::<Payload>();
    let sys = actix::System::new();
    let sk = get_secret_key(args.secret_key_path.clone()).context("Can't find secret key path.").unwrap();
    let local_share = std::fs::read(args.local_share).unwrap();
    let local_share = serde_json::from_slice::<LocalKey<Secp256k1>>(&local_share).unwrap();

    let i = args.index.clone();
    let local_share1 = local_share.clone();
    let s = async move {
        match join_computation(args.messenger_address, sk).await {
            Ok((incoming, outgoing)) => {
                let coordinator = Coordinator::new(incoming, outgoing);
                while let Some(payload) = rx.recv().await {
                    log::info!("Received request {:?}", payload);
                    match payload {
                         Either::Left(KeygenPayload{i, t, n}) => {
                             let result = coordinator.do_send(KeygenRequest {
                                 room: "2345".to_string(),
                                 i, t, n
                             });
                        }
                        Either::Right(SignPayload{message}) => {

                            let result = coordinator.do_send(SignRequest {
                                message: message.clone(),
                                room: "1234".to_string(),
                                i,
                                s_l: vec![1, 2],
                                local_key: local_share1.clone(),
                            });
                        }
                    }
                }
            }
            Err(_) => {}
        }
    };

    Arbiter::new().spawn(s);

    let server = move || { HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppState {
                tx: tx.clone(),
                i: args.index,
                s_l: Vec::try_from([1, 2]).unwrap(),
                local_key: local_share.clone(),
            }))
            .wrap(middleware::Logger::default())
            .route("/hello", web::get().to(|| async { "Hello World!" }))
            .route("/keygen", web::post().to(keygen))
            .route("/sign", web::post().to(sign))
        // .service(greet)
    })
        .bind(("127.0.0.1", args.port))
        .unwrap()
        .run()
    };

    sys.block_on(server())
}
