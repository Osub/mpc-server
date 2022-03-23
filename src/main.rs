mod signer;
mod player;
mod coordinator;
mod messages;
mod cli;
mod transport;
mod group;

extern crate env_logger;

#[macro_use]
extern crate log;

use std::borrow::Borrow;
use std::rc::Rc;
use std::sync::Arc;
use std::{thread, time};
use std::path::PathBuf;
use std::str::from_utf8;
use actix::prelude::*;
use actix_web::{get, web, http, middleware, App, HttpServer, Responder, HttpRequest, HttpResponse, Error};
use curv::elliptic::curves::Secp256k1;
use either::Either;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::LocalKey;
use secp256k1::{PublicKey, SecretKey};
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
    request_id: String,
    public_key: String,
    participant_public_keys: Vec<String>,
    message: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct KeygenPayload {
    request_id: String,
    public_keys: Vec<String>,
    t: u16,
}

type Payload = Either<KeygenPayload, SignPayload>;

#[derive(Serialize, Deserialize, Debug)]
pub struct ResponsePayload {
    request_id: String,
    result: Option<String>,
    request_type: String,
    request_status: String,
}

struct AppState {
    tx: UnboundedSender<Payload>,
    results_db: sled::Db,
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

async fn result(data: web::Data<AppState>, request_id: web::Path<String>) -> impl Responder {
    let request_id = request_id.into_inner();
    let response = data.results_db.get(request_id.as_bytes());
    let NOT_FOUND = "{\"error\": \"Not found\"}";
    let NOT_FOUND = HttpResponse::NotFound().content_type("application/json").body(NOT_FOUND);
    if response.is_err() {
        return NOT_FOUND
    }
    if response.as_ref().unwrap().is_none() {
        return NOT_FOUND
    }
    let response = response.unwrap().unwrap();
    let response = String::from_utf8(response.to_vec());

    match response {
        Ok(r) => {HttpResponse::Ok().content_type("application/json").body(r)}
        Err(_) => NOT_FOUND
    }
}

fn get_secret_key(path: PathBuf) -> Result<(SecretKey, PublicKey)> {
    let sk_hex = std::fs::read_to_string(path).context("Read secret key file.")?;
    let sk_bytes = hex::decode(sk_hex).context("Decode hex secret key.")?;

    let sk = SecretKey::parse_slice(sk_bytes.as_slice()).context("Parse secret key.")?;
    let pk = PublicKey::from_secret_key(&sk);
    Ok((sk, pk))
}

fn main() -> std::io::Result<()> {
    ::std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let args: Cli = Cli::from_args();
    let (tx, mut rx) = unbounded_channel::<Payload>();
    let (tx_res, mut rx_res) = unbounded_channel::<ResponsePayload>();
    let sys = actix::System::new();
    let (sk, pk) = get_secret_key(args.secret_key_path.clone()).context("Can't find secret key path.").unwrap();
    let own_public_key = hex::encode(pk.serialize_compressed());
    let results_path = args.db_path.join("results");
    let results_db: sled::Db = sled::open(results_path).unwrap();
    let local_shares_path = args.db_path.join("local_shares");
    let local_share_db: sled::Db = sled::open(local_shares_path).unwrap();
    let s = async move {
        match join_computation(args.messenger_address, sk).await {
            Ok((incoming, outgoing)) => {
                let coordinator = Coordinator::new(tx_res, local_share_db, incoming, outgoing);
                while let Some(payload) = rx.recv().await {
                    log::info!("Received request {:?}", payload);
                    match payload {
                         Either::Left(KeygenPayload{request_id, public_keys, t}) => {
                             let result = coordinator.do_send(KeygenRequest {
                                 request_id, public_keys, t,
                                 own_public_key: own_public_key.clone()
                             });
                        }
                        Either::Right(SignPayload{request_id, message, public_key, participant_public_keys}) => {

                            let result = coordinator.do_send(SignRequest {
                                request_id,
                                participant_public_keys,
                                public_key,
                                message,
                                own_public_key: own_public_key.clone()
                            });
                        }
                    }
                }
            }
            Err(_) => {}
        }
    };

    let results_db1 = results_db.clone();
    let handle_response = async move {
        while let Some(response) = rx_res.recv().await {
            let key = response.request_id.clone();
            let value = serde_json::to_vec_pretty(&response);
            match value {
                Ok(value) => {
                    results_db1.insert(
                        key.as_bytes(),
                        value.as_slice(),
                    );
                }
                Err(_) => {}
            }
        }
    };

    Arbiter::new().spawn(s);
    Arbiter::new().spawn(handle_response);

    let server = move || { HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppState {
                tx: tx.clone(),
                results_db: results_db.clone(),
            }))
            .wrap(middleware::Logger::default())
            .route("/hello", web::get().to(|| async { "Hello World!" }))
            .route("/keygen", web::post().to(keygen))
            .route("/sign", web::post().to(sign))
            .route("/result/{request_id}", web::post().to(result))
        // .service(greet)
    })
        .bind(("127.0.0.1", args.port))
        .unwrap()
        .run()
    };

    sys.block_on(server())
}
