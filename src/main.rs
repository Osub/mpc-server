mod player;
mod coordinator;
mod messages;
mod cli;
mod transport;
mod message_proxy;

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
use crate::messages::{IncomingEnvelope, SignRequest};
use cli::Cli;
use structopt::StructOpt;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use crate::transport::join_computation;

#[derive(Serialize, Deserialize, Debug)]
struct SignPayload {
    message: String,
}

struct AppState {
    sender: UnboundedSender<SignPayload>,
    i: u16,
    s_l: Vec<u16>,
    local_key: LocalKey<Secp256k1>,
}

struct Task(Box<dyn Fn(&mut LocalQueue<Task>) + Send>);

async fn sign(data: web::Data<AppState>, req: web::Json<SignPayload>) -> impl Responder {
    log::info!("handling");
    data.sender.send(req.0);
    format!("Hi there!")
}

fn main() -> std::io::Result<()> {
    ::std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    let args: Cli = Cli::from_args();
    let (sender, mut receiver) = unbounded_channel::<SignPayload>();
    let sys = actix::System::new();

    // let c = Coordinator::new(args.messenger_address);
    let local_share = std::fs::read(args.local_share).unwrap();
    let local_share = serde_json::from_slice::<LocalKey<Secp256k1>>(&local_share).unwrap();
    // let coordinator = SyncArbiter::start(1, || { c });
    // let coordinator = c.start();

    let i = args.index.clone();
    let local_share1 = local_share.clone();
    let s = async move {
        match join_computation(args.messenger_address).await {
            Ok((incoming, outgoing)) => {
                let coordinator = Coordinator::new(incoming, outgoing);
                while let Some(r) = receiver.recv().await {
                    log::info!("Received request {:?}", r);
                    let result = coordinator.do_send(SignRequest {
                        room: "1234".to_string(),
                        i,
                        s_l: vec![1, 2],
                        local_key: local_share1.clone(),
                    });
                    log::info!("Result is {:?}", result);
                }
            }
            Err(_) => {}
        }
    };

    Arbiter::new().spawn(s);

    let server = move || { HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppState {
                sender: sender.clone(),
                i: args.index,
                s_l: Vec::try_from([1, 2]).unwrap(),
                local_key: local_share.clone(),
            }))
            .wrap(middleware::Logger::default())
            .route("/hello", web::get().to(|| async { "Hello World!" }))
            .route("/sign", web::post().to(sign))
        // .service(greet)
    })
        .bind(("127.0.0.1", args.port))
        .unwrap()
        .run()
    };

    sys.block_on(server())
}
