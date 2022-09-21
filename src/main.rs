extern crate base64;
extern crate json_env_logger;


use std::path::PathBuf;

use ::redis::Client;
use actix::prelude::*;
use actix_web::{App, HttpResponse, HttpServer, middleware, Responder, web};
use anyhow::{Context, Result};
use curv::arithmetic::Converter;
use curv::BigInt;
use either::Either;
use prometheus::{Encoder, TextEncoder};
use secp256k1::{PublicKey, SecretKey};
use structopt::StructOpt;
use thiserror::Error;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use cli::Cli;

use crate::actors::{Coordinator, handle};
use crate::core::{KeygenPayload, Payload, ResponsePayload, SignPayload};
use crate::key::decrypt;
use crate::transport::{join_computation, join_computation_via_redis};

mod core;
mod actors;
mod cli;
mod transport;
mod key;
mod prom;
mod utils;

#[derive(Debug, Error)]
enum SetupError {
    #[error("Unable to connect to message queue.")]
    NoMessageQueueConnection,

    #[error("Multiple message queue has been configured.")]
    MultipleMessageQueueConfigured,
}

struct AppState {
    tx: UnboundedSender<Payload>,
    tx_res: UnboundedSender<ResponsePayload>,
    results_db: sled::Db,
}

fn save_result(db: &sled::Db, response: ResponsePayload) -> Result<()> {
    let key = response.request_id.clone();
    let value = serde_json::to_vec_pretty(&response)?;
    db.insert(
        key.as_bytes(),
        value.as_slice(),
    )?;
    Ok(())
}


async fn metrics() -> impl Responder {
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();

    let metric_families = prometheus::gather();
    match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => {
            match String::from_utf8(buffer.clone()) {
                Ok(output) => {
                    HttpResponse::Ok().body(output)
                }
                Err(_) => {
                    HttpResponse::InternalServerError().body("Failed to encode metrics.")
                }
            }
        }
        Err(_) => {
            HttpResponse::InternalServerError().body("Failed to encode metrics.")
        }
    }
}

async fn keygen(data: web::Data<AppState>, req: web::Json<KeygenPayload>) -> impl Responder {
    let exists_already = data.results_db.contains_key(req.request_id.as_bytes()).map_or(false, |x| x);
    if exists_already {
        return HttpResponse::BadRequest().body(format!("A request of id \"{}\" already exists.", req.request_id));
    }
    let _ = data.tx_res.send(ResponsePayload {
        request_id: req.0.request_id.clone(),
        result: None,
        request_type: "KEYGEN".to_owned(),
        request_status: "RECEIVED".to_owned(),
    });
    match data.tx.send(Either::Left(req.0)) {
        Ok(_) => {
            HttpResponse::Ok().body("Request received!")
        }
        Err(_) => {
            HttpResponse::InternalServerError().body("Failed to queue the request.")
        }
    }
}

async fn sign(data: web::Data<AppState>, req: web::Json<SignPayload>) -> impl Responder {
    let exists_already = data.results_db.contains_key(req.request_id.as_bytes()).map_or(false, |x| x);
    if exists_already {
        return HttpResponse::BadRequest().body(format!("A request of id \"{}\" already exists.", req.request_id));
    }
    match BigInt::from_hex(req.message.as_str()) {
        Ok(_) => {
            let _ = data.tx_res.send(ResponsePayload {
                request_id: req.0.request_id.clone(),
                result: None,
                request_type: "SIGN".to_owned(),
                request_status: "RECEIVED".to_owned(),
            });
            match data.tx.send(Either::Right(req.0)) {
                Ok(_) => {
                    HttpResponse::Ok().body("Request received!")
                }
                Err(_) => {
                    HttpResponse::InternalServerError().body("Failed to queue the request.")
                }
            }
        }
        Err(_) => {
            HttpResponse::BadRequest().body("Message is not a valid hash.")
        }
    }
}

async fn result(data: web::Data<AppState>, request_id: web::Path<String>) -> impl Responder {
    let request_id = request_id.into_inner();
    let response = data.results_db.get(request_id.as_bytes());
    let not_found = "{\"error\": \"Not found\"}";
    let not_found = HttpResponse::NotFound().content_type("application/json").body(not_found);
    if response.is_err() {
        return not_found;
    }
    if response.as_ref().unwrap().is_none() {
        return not_found;
    }
    let response = response.unwrap().unwrap();
    let response = String::from_utf8(response.to_vec());

    match response {
        Ok(r) => { HttpResponse::Ok().content_type("application/json").body(r) }
        Err(_) => not_found
    }
}

fn get_secret_key(path: PathBuf, password: String) -> Result<(SecretKey, PublicKey)> {
    let sk_hex = std::fs::read_to_string(path).context("Read secret key file.")?;
    let sk_bytes = hex::decode(sk_hex).context("Decode hex secret key.")?;

    let sk = decrypt(password.as_bytes(), sk_bytes.as_slice())?;

    let sk = SecretKey::parse_slice(sk.as_slice()).context("Parse secret key.")?;
    let pk = PublicKey::from_secret_key(&sk);
    Ok((sk, pk))
}

async fn precheck(args: Cli) -> Result<()> {
    let args0 = args.clone();
    match (args0.messenger_address, args0.redis_url) {
        (Some(url), None) => {
            let url = url.join("healthcheck")?;
            let res = surf::get(url).await;
            res.map_err(|_| {
                SetupError::NoMessageQueueConnection
            })?;
            Ok(())
        }
        (None, Some(url)) => {
            let client = Client::open(url)?;
            client.get_connection().map_err(|_| {
                SetupError::NoMessageQueueConnection
            })?;
            Ok(())
        }
        _ => {
            Err(SetupError::MultipleMessageQueueConfigured.into())
        }
    }
}

async fn bootstrap(args: Cli, rx: UnboundedReceiver<Payload>, tx_res: UnboundedSender<ResponsePayload>) {
    let args0 = args.clone();
    match (args0.messenger_address, args0.redis_url) {
        (Some(_), None) => {
            use_messenger(args, rx, tx_res).await;
        }
        (None, Some(_)) => {
            use_redis(args, rx, tx_res).await;
        }
        _ => {
            panic!("no message queue configured!");
        }
    }
}

async fn use_messenger(args: Cli, mut rx: UnboundedReceiver<Payload>, tx_res: UnboundedSender<ResponsePayload>) {
    let (sk, pk) = get_secret_key(args.secret_key_path.clone(), args.password.clone()).context("Can't get secret key.").unwrap();
    let own_public_key = hex::encode(pk.serialize_compressed());
    let local_shares_path = args.db_path.join("local_shares");
    let local_share_db: sled::Db = sled::open(local_shares_path).unwrap();

    if let Ok((incoming, outgoing)) = join_computation(args.messenger_address.unwrap(), sk.clone()).await {
        let coordinator = Coordinator::new(sk, tx_res, local_share_db, incoming, outgoing);
        while let Some(payload) = rx.recv().await {
            handle(&coordinator, own_public_key.clone(), payload).await;
        }
    };
}


async fn use_redis(args: Cli, mut rx: UnboundedReceiver<Payload>, tx_res: UnboundedSender<ResponsePayload>) {
    let (sk, pk) = get_secret_key(args.secret_key_path.clone(), args.password.clone()).context("Can't get secret key.").unwrap();
    let own_public_key = hex::encode(pk.serialize_compressed());
    let local_shares_path = args.db_path.join("local_shares");
    let local_share_db: sled::Db = sled::open(local_shares_path).unwrap();

    if let Ok((incoming, outgoing)) = join_computation_via_redis(args.redis_url.unwrap(), sk.clone()).await {
        let coordinator = Coordinator::new(sk, tx_res, local_share_db, incoming, outgoing);
        while let Some(payload) = rx.recv().await {
            handle(&coordinator, own_public_key.clone(), payload).await;
        }
    };
}

async fn handle_response(results_db: sled::Db, mut rx_res: UnboundedReceiver<ResponsePayload>) {
    while let Some(response) = rx_res.recv().await {
        let _ = save_result(&results_db, response);
    }
}

fn main() -> std::io::Result<()> {
    json_env_logger::init();
    let args: Cli = Cli::from_args();
    let (tx, rx) = unbounded_channel::<Payload>();
    let (tx_res, rx_res) = unbounded_channel::<ResponsePayload>();

    let results_path = args.db_path.join("results");
    let results_db: sled::Db = sled::open(results_path).unwrap();

    let sys = actix::System::new();
    sys.block_on(async {
        precheck(args.clone()).await.unwrap();
    });

    Arbiter::new().spawn(bootstrap(args.clone(), rx, tx_res.clone()));
    Arbiter::new().spawn(handle_response(results_db.clone(), rx_res));

    let server = move || {
        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(AppState {
                    tx: tx.clone(),
                    tx_res: tx_res.clone(),
                    results_db: results_db.clone(),
                }))
                .wrap(middleware::Logger::default())
                .route("/keygen", web::post().to(keygen))
                .route("/sign", web::post().to(sign))
                .route("/result/{request_id}", web::post().to(result))
                .route("/metrics", web::get().to(metrics))
        })
            .bind((args.address, args.port))
            .unwrap()
            .run()
    };

    sys.block_on(server())
}
