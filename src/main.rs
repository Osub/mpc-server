extern crate env_logger;

use std::path::PathBuf;

use actix::prelude::*;
use actix_web::{App, HttpResponse, HttpServer, middleware, Responder, web};
use anyhow::{Context, Result};
use curv::arithmetic::Converter;
use curv::BigInt;
use either::Either;
use secp256k1::{PublicKey, SecretKey};
use structopt::StructOpt;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use cli::Cli;

use crate::actors::{Coordinator, handle};
use crate::core::{KeygenPayload, Payload, ResponsePayload, SignPayload};
use crate::transport::join_computation;

mod core;
mod actors;
mod cli;
mod transport;


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

async fn keygen(data: web::Data<AppState>, req: web::Json<KeygenPayload>) -> impl Responder {
    let exists_already = data.results_db.contains_key(req.request_id.as_bytes()).map_or(false, |x|x);
    if exists_already {
        return HttpResponse::BadRequest().body(format!("A request of id \"{}\" already exists.", req.request_id));
    }
    let _ = data.tx_res.send(ResponsePayload{
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
    let exists_already = data.results_db.contains_key(req.request_id.as_bytes()).map_or(false, |x|x);
    if exists_already {
        return HttpResponse::BadRequest().body(format!("A request of id \"{}\" already exists.", req.request_id));
    }
    match  BigInt::from_hex(req.message.as_str()) {
        Ok(_) => {
            let _ = data.tx_res.send(ResponsePayload{
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
    let tx_res1 = tx_res.clone();
    let s = async move {
        match join_computation(args.messenger_address, sk).await {
            Ok((incoming, outgoing)) => {
                let coordinator = Coordinator::new(tx_res1, local_share_db, incoming, outgoing);
                while let Some(payload) = rx.recv().await {
                    handle(&coordinator, own_public_key.clone(), payload);
                }
            }
            Err(_) => {}
        }
    };

    let results_db1 = results_db.clone();
    let handle_response = async move {
        while let Some(response) = rx_res.recv().await {
            let _ = save_result(&results_db1, response);
        }
    };

    Arbiter::new().spawn(s);
    Arbiter::new().spawn(handle_response);

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
        })
            .bind(("127.0.0.1", args.port))
            .unwrap()
            .run()
    };

    sys.block_on(server())
}
