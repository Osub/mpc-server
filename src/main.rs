mod player;
mod coordinator;
mod messages;
mod cli;
mod transport;
mod message_hub;

extern crate env_logger;

#[macro_use]
extern crate log;

use std::borrow::Borrow;
use std::rc::Rc;
use std::sync::Arc;
use std::{thread, time};
use actix::prelude::*;
use actix_web::{get, web, http, middleware, App, HttpServer, Responder, HttpRequest, HttpResponse, Error};
use curv::elliptic::curves::Secp256k1;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::LocalKey;
use work_queue::{LocalQueue, Queue};
use serde::{Serialize, Deserialize};
use crate::coordinator::Coordinator;
use crate::messages::SignRequest;
use cli::Cli;
use structopt::StructOpt;

#[derive(Serialize, Deserialize, Debug)]
struct SignPayload {
    message: String,
}

struct AppState {
    coordinator: Addr<Coordinator>,
    i: u16,
    s_l: Vec<u16>,
    local_key: LocalKey<Secp256k1>,
}

struct Task(Box<dyn Fn(&mut LocalQueue<Task>) + Send>);

async fn sign(data: web::Data<AppState>, req: web::Json<SignPayload>) -> impl Responder {
    println!("handling");
    data.coordinator.do_send(SignRequest {
        room: "1234".to_string(),
        i: data.i.clone(),
        s_l: data.s_l.clone(),
        local_key: data.local_key.clone(),
    });
    format!("Hi there!")
}

fn main() -> std::io::Result<()> {
    ::std::env::set_var("RUST_LOG", "info");
    env_logger::init();
    let args: Cli = Cli::from_args();

    let sys = actix::System::new();
    let coordinator = SyncArbiter::start(1, move || Coordinator::default());
    let local_share = std::fs::read(args.local_share).unwrap();
    let local_share = serde_json::from_slice::<LocalKey<Secp256k1>>(&local_share).unwrap();
    sys.block_on(
        HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(AppState {
                    coordinator: coordinator.clone(),
                    i: args.index,
                    s_l: Vec::try_from([1, 2]).unwrap(),
                    local_key: local_share.clone(),
                }))
                .wrap(middleware::Logger::default())
                .route("/hello", web::get().to(|| async { "Hello World!" }))
                .route("/sign", web::post().to(sign))
                // .service(greet)
        })
            .bind(("127.0.0.1", args.port))?
            .run()
    )
}
