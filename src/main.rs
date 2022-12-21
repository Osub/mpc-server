extern crate base64;
extern crate json_env_logger;


use std::path::PathBuf;

use ::redis::Client;
use actix::prelude::*;
use actix_web::{App, HttpResponse, HttpServer, middleware, Responder, web};
use anyhow::{Context, Result};


use prometheus::{Encoder, TextEncoder};
use secp256k1::{PublicKey, SecretKey};
use structopt::StructOpt;
use thiserror::Error;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use cli::Cli;

use crate::actors::{Coordinator, handle};
use crate::pb::types::request::Request;
use crate::key::decrypt;
use crate::pb::mpc::CheckResultResponse;
use crate::pb::mpc::mpc_server::MpcServer;
use crate::server::MpcImp;
use crate::transport::{join_computation_via_messenger, join_computation_via_redis};

mod core;
mod actors;
mod cli;
mod transport;
mod key;
mod prom;
mod utils;
mod storage;
mod crypto;
mod server;
mod pb;

#[derive(Debug, Error)]
enum AppError {
    #[error("Metrics server error.")]
    MetricsServerError,

    #[error("Mpc server error.")]
    MpcServerError,
}

#[derive(Debug, Error)]
enum SetupError {
    #[error("Unable to connect to message queue.")]
    NoMessageQueueConnection,

    #[error("Multiple message queue has been configured.")]
    MultipleMessageQueueConfigured,
}

struct AppState {}

fn save_result(db: &sled::Db, response: CheckResultResponse) -> Result<()> {
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

async fn bootstrap(args: Cli, rx: UnboundedReceiver<Request>, tx_res: UnboundedSender<CheckResultResponse>) {
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

async fn use_messenger(args: Cli, mut rx: UnboundedReceiver<Request>, tx_res: UnboundedSender<CheckResultResponse>) {
    let (sk, _) = get_secret_key(args.secret_key_path.clone(), args.password.clone()).context("Can't get secret key.").unwrap();
    let local_shares_path = args.db_path.join("local_shares");
    let local_share_db: sled::Db = sled::open(local_shares_path).unwrap();

    if let Ok((incoming, outgoing)) = join_computation_via_messenger(args.messenger_address.unwrap(), sk.clone()).await {
        let coordinator = Coordinator::new(sk, tx_res, local_share_db, incoming, outgoing);
        while let Some(req) = rx.recv().await {
            handle(&coordinator, req).await;
        }
    };
}


async fn use_redis(args: Cli, mut rx: UnboundedReceiver<Request>, tx_res: UnboundedSender<CheckResultResponse>) {
    let (sk, _) = get_secret_key(args.secret_key_path.clone(), args.password.clone()).context("Can't get secret key.").unwrap();
    let local_shares_path = args.db_path.join("local_shares");
    let local_share_db: sled::Db = sled::open(local_shares_path).unwrap();

    if let Ok((incoming, outgoing)) = join_computation_via_redis(args.redis_url.unwrap(), sk.clone()).await {
        let coordinator = Coordinator::new(sk, tx_res, local_share_db, incoming, outgoing);
        while let Some(payload) = rx.recv().await {
            handle(&coordinator, payload).await;
        }
    };
}

async fn handle_response(results_db: sled::Db, mut rx_res: UnboundedReceiver<CheckResultResponse>) {
    while let Some(response) = rx_res.recv().await {
        let _ = save_result(&results_db, response);
    }
}

fn main() -> Result<(), AppError> {
    json_env_logger::init();
    let args: Cli = Cli::from_args();
    let (tx, rx) = unbounded_channel::<Request>();
    let (tx_res, rx_res) = unbounded_channel::<CheckResultResponse>();

    let results_path = args.db_path.join("results");
    let results_db: sled::Db = sled::open(results_path).unwrap();

    let sys = actix::System::new();
    sys.block_on(async {
        precheck(args.clone()).await.unwrap();
    });

    Arbiter::new().spawn(bootstrap(args.clone(), rx, tx_res.clone()));
    Arbiter::new().spawn(handle_response(results_db.clone(), rx_res));

    let args0 = args.clone();
    let metrics_server = move || {
        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(AppState {}))
                .wrap(middleware::Logger::default())
                .route("/metrics", web::get().to(metrics))
        })
            .bind((args0.address, args0.port + 100))
            .unwrap()
            .run()
    };
    let mpc_server = move || {
        let instance = MpcImp::new(tx.clone(), tx_res.clone(), results_db.clone());
        let addr = format!("{}:{}", args.address, args.port).parse().unwrap();
        tonic::transport::Server::builder()
            .add_service(MpcServer::new(instance))
            .serve(addr)
    };
    let combined = || async {
        let res = futures::future::join(
            metrics_server(), mpc_server(),
        ).await;
        match res {
            (Ok(_), Ok(_)) => {
                Ok(())
            }
            (_, Err(_)) => {
                Err(AppError::MpcServerError)
            }
            (Err(_), _) => {
                Err(AppError::MetricsServerError)
            }
        }
    };

    sys.block_on(combined())
}
