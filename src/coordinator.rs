use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use crate::messages::*;
use actix::prelude::*;
use actix_interop::{critical_section, FutureInterop, with_ctx};
use crate::player::MpcPlayer;
use crate::signer::Signer;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{OfflineStage, CompletedOfflineStage, OfflineProtocolMessage, SignManual, PartialSignature};
use round_based::{Msg, StateMachine};
use crate::transport::join_computation;
use anyhow::{Context as AnyhowContext, Error, Result};
use curv::arithmetic::Converter;
use curv::BigInt;
use serde_json::ser::State;
use futures::{future, Sink};
use futures_util::SinkExt;
use surf::Url;
use futures_util::TryStreamExt;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::party_i::SignatureRecid;

pub struct Coordinator {
    offline_state_runners: HashMap<String, Addr<MpcPlayer<SignRequest, OfflineStage, <OfflineStage as StateMachine>::Err, <OfflineStage as StateMachine>::Output>>>,
    signers: HashMap<String, Addr<Signer<SignRequest>>>,
    sink: Option<Pin<Box<dyn Sink<Envelope, Error=anyhow::Error>>>>,
}

impl Coordinator {
    pub fn new<Si, St>(stream: St, sink: Si) -> Addr<Self>
        where
            St: Stream<Item=Result<Envelope>> + 'static,
            Si: Sink<Envelope, Error=anyhow::Error> + 'static,
    {
        let stream = stream.and_then(|msg| async move {
            Ok(IncomingEnvelope {
                room: msg.room,
                message: msg.message,
            })
        });
        let sink: Box<dyn Sink<Envelope, Error=anyhow::Error>> = Box::new(sink);

        Self::create(|ctx| {
            ctx.add_stream(stream);
            Self {
                offline_state_runners: HashMap::new(),
                signers: HashMap::new(),
                sink: Some(sink.into()),
            }
        })
    }
    fn retry_later(&mut self, msg: IncomingEnvelope, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::from_secs(1), move|a, _ctx| {
            _ctx.notify(RetryEnvelope {
                room: msg.room.clone(),
                message: msg.message.clone(),
            });
        });
    }
    fn handle_incoming_offline(&mut self, msg: IncomingEnvelope, ctx: &mut Context<Self>) ->bool {
        let room = msg.room;
        match self.offline_state_runners.get(&room).context("Not found room.") {
            Ok(addr)=> {
                let msg = serde_json::from_str::<Msg<OfflineProtocolMessage>>(&msg.message).context("deserialize message");
                match msg {
                    Ok(msg)=> {
                        addr.do_send(IncomingMessage {
                            room: room.clone(),
                            message: msg,
                        });
                        true
                    }
                    Err(_) => {
                        false
                    }
                }
            }
            Err(_) => {false}
        }
    }
    fn handle_incoming_sign(&mut self, msg: IncomingEnvelope, ctx: &mut Context<Self>) ->bool {
        let room = msg.room;
        match self.signers.get(&room).context("Not found room.") {
            Ok(addr)=> {
                let msg = serde_json::from_str::<Msg<PartialSignature>>(&msg.message).context("deserialize message");
                match msg {
                    Ok(msg)=> {
                        addr.do_send(IncomingMessage {
                            room: room.clone(),
                            message: msg,
                        });
                        true
                    }
                    Err(_) => {
                        false
                    }
                }
            }
            Err(_) => {false}
        }
    }
    fn handle_incoming(&mut self, msg: IncomingEnvelope, ctx: &mut Context<Self>) {
        let h1= self.handle_incoming_offline(msg.clone(), ctx);
        let h2 = self.handle_incoming_sign(msg.clone(), ctx);
        if !(h1 || h2) {
            ctx.run_later(Duration::from_secs(1), move|a, _ctx| {
                _ctx.notify(RetryEnvelope {
                    room: msg.room.clone(),
                    message: msg.message.clone(),
                });
            });
        }
        // let newMsg = msg.clone();
        // let newMsg1 = msg.clone();
        //     let msg = Rc::new(msg);
        //     let offline_addr =  self.offline_state_runners.get(&msg.as_ref().room).context("Not found room.");
        //     let signer_addr = self.signers.get(&msg.as_ref().room).context("Not found room.");
        //     let offline_msg = serde_json::from_str::<Msg<OfflineProtocolMessage>>(&msg.as_ref().message).context("deserialize message");
        //     let partial_sig_msg = serde_json::from_str::<Msg<PartialSignature>>(&msg.as_ref().message).context("deserialize message");
        //     match (offline_addr, offline_msg) {
        //         (Ok(addr), Ok(offline_msg)) => {
        //             addr.do_send(IncomingMessage {
        //                 room: msg.as_ref().room.clone(),
        //                 message: offline_msg,
        //             });
        //         }
        //         (Err(e), _)=> {
        //             self.retry_later(newMsg.clone(), ctx);
        //         }
        //         (_, Err(_)) => {
        //             // Ignore invalid message
        //         }
        //     };
        //
        //     match (signer_addr, partial_sig_msg) {
        //         (Ok(addr), Ok(partial_sig)) => {
        //             addr.do_send(IncomingMessage {
        //                 room: msg.room.clone(),
        //                 message: partial_sig,
        //             });
        //         }
        //         (Err(e), Ok(_)) => {
        //             self.retry_later(newMsg1.clone(), ctx);
        //         }
        //         (_, Err(_)) => {
        //             // Ignore invalid message
        //         }
        //     };
    }

    fn send_one(envelope: OutgoingEnvelope) -> impl Future<Output=()> {
        async move {
            critical_section::<Self, _>(async {
                let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
                    .expect("Sink to be present");

                // Send the request
                sink.send(Envelope {
                    room: envelope.room,
                    message: envelope.message,
                }).await;

                // Put the sink back, and if the send was successful,
                // record the in-flight request.
                with_ctx(|actor: &mut Self, _| {
                    actor.sink = Some(sink);
                });
            })
                .await;
        }
    }
}

impl Actor for Coordinator {
    type Context = Context<Self>;
}

impl Handler<SignRequest> for Coordinator {
    type Result = Result<()>;

    fn handle(&mut self, req: SignRequest, ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Received request {:?}", req);

        let req0 = Arc::new(SignRequest {
            message: req.message.clone(),
            room: req.room.clone(),
            i: req.i.clone(),
            s_l: req.s_l.clone(),
            local_key: req.local_key.clone(),
        });

        let state = OfflineStage::new(req0.i, req0.s_l.clone(), req0.local_key.clone()).context("Create state machine")?;
        let player = MpcPlayer::new(
            req.clone(),
            req0.room.clone(),
            req0.i,
            state,
            ctx.address().recipient(),
            ctx.address().recipient(),
            ctx.address().recipient(),
        ).start();
        self.offline_state_runners.insert(req0.room.to_owned(), player);
        Ok(())
    }
}

impl Handler<ProtocolError<<OfflineStage as StateMachine>::Err>> for Coordinator {
    type Result = ();

    fn handle(&mut self, msg: ProtocolError<<OfflineStage as StateMachine>::Err>, _: &mut Context<Self>) {
        log::info!("Error {:?}", msg.error);
    }
}

impl Handler<OutgoingEnvelope> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: OutgoingEnvelope, ctx: &mut Context<Self>) {
        ctx.spawn(Self::send_one(msg).interop_actor(self));
    }
}

impl Handler<ProtocolOutput<SignRequest, CompletedOfflineStage>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutput<SignRequest, CompletedOfflineStage>, ctx: &mut Context<Self>) {
        log::info!("result {:?}", msg.output.public_key());
        let do_it = ||-> Result<()>{

            let message = BigInt::from_bytes(msg.input.message.as_bytes());
            let completed_offline_stage = msg.output;
            let input = msg.input.clone();
            let signer = Signer::new(
                input.clone(),
                input.room,
                input.i,
                input.s_l.len()-1,
                message,
                completed_offline_stage,
                ctx.address().recipient(),
                ctx.address().recipient(),
            ).start();
            self.signers.insert(msg.input.room.to_owned(), signer);
            Ok(())
        };
        do_it();
    }

}

impl Handler<ProtocolOutput<SignRequest, SignatureRecid>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutput<SignRequest, SignatureRecid>, ctx: &mut Context<Self>) {
        serde_json::to_string(&msg).context("serialize signature").map(
            |serialized|log::info!("Sign request done {:?}", serialized)
        );
    }

}

impl StreamHandler<Result<IncomingEnvelope>> for Coordinator
{
    fn handle(&mut self, msg: Result<IncomingEnvelope>, ctx: &mut Context<Self>) {
        match msg.context("Invalid IncomingEnvlope") {
            Ok(msg) => { self.handle_incoming(msg, ctx); }
            Err(_) => {}
        }
    }
}

impl Handler<RetryEnvelope> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: RetryEnvelope, ctx: &mut Context<Self>) {
        self.handle_incoming(IncomingEnvelope { room: msg.room, message: msg.message }, ctx);
    }
}
