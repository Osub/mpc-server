use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use crate::messages::*;
use actix::prelude::*;
use actix_interop::{critical_section, FutureInterop, with_ctx};
use crate::player::MpcPlayer;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{OfflineStage, CompletedOfflineStage, OfflineProtocolMessage};
use round_based::{Msg, StateMachine};
use crate::transport::join_computation;
use anyhow::{Context as AnyhowContext, Error, Result};
use serde_json::ser::State;
use futures::{future, Sink};
use futures_util::SinkExt;
use surf::Url;
use futures_util::TryStreamExt;

pub struct Coordinator {
    pub(crate) runners: HashMap<String, Addr<MpcPlayer<OfflineStage, <OfflineStage as StateMachine>::Err, <OfflineStage as StateMachine>::Output>>>,
    sink: Option<Pin<Box<dyn Sink<Envelope, Error = anyhow::Error>>>>,
}

impl Coordinator {
    pub fn new<Si, St>(stream: St, sink: Si) -> Addr<Self>
        where
            St: Stream<Item = Result<Envelope>> + 'static,
            Si: Sink<Envelope, Error = anyhow::Error> + 'static,
    {
        let stream = stream.and_then(|msg| async move {
            Ok(IncomingEnvelope {
                room: msg.room,
                message: msg.message,
            })
        });
        let sink: Box<dyn Sink<Envelope, Error = anyhow::Error>> = Box::new(sink);

        Self::create(|ctx| {
            ctx.add_stream(stream);
            Self {
                runners: HashMap::new(),
                sink: Some(sink.into()),
            }
        })

    }
    fn forward_one( envelope: Envelope)-> impl Future<Output=()> {
        async move {
            critical_section::<Self, _>(async {

                let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
                    .expect("Sink to be present");

                // Send the request
                sink.send(Envelope{
                    room: envelope.room,
                    message: envelope.message
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
    fn handle_incoming(&mut self,  msg: Result<IncomingEnvelope>,  ctx: &mut Context<Self>)->Result<()>{
            let m = msg.context("Invalid IncomingEnvlope")?;

            let addr = self.runners.get(&m.room).context("Not found room.")?;
        let offlineMsg = serde_json::from_str::<Msg<OfflineProtocolMessage>>(&m.message).context("deserialize message")?;
        log::debug!("Received msg {:?}", offlineMsg);
        addr.send(IncomingMessage {
            room: m.room.clone(),
            message: offlineMsg
        });
        Ok(())
    }
    fn send_one( envelope: OutgoingEnvelope)-> impl Future<Output=()> {
        async move {
            critical_section::<Self, _>(async {

                let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
                    .expect("Sink to be present");

                // Send the request
                sink.send(Envelope{
                    room: envelope.room,
                    message: envelope.message
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

        let req0 = Arc::new(SignRequest{
            room: req.room.clone(),
            i: req.i.clone(),
            s_l: req.s_l.clone(),
            local_key: req.local_key.clone(),
        });

        let state = OfflineStage::new(req0.i, req0.s_l.clone(), req0.local_key.clone()).context("Create state machine")?;
        let player = MpcPlayer::new(
            req0.room.clone(),
            req0.i,
            state,
            ctx.address().recipient(),
            ctx.address().recipient(),
            ctx.address().recipient(),
        ).start();
        self.runners.insert(req0.room.to_owned(), player);
        Ok(())
    }
}

impl Handler<ProtocolError<<OfflineStage as StateMachine>::Err>> for Coordinator {
    type Result = ();

    fn handle(&mut self, msg: ProtocolError<<OfflineStage as StateMachine>::Err>, ctx: &mut Context<Self>) {
        log::info!("Error {:?}", msg.error);
    }
}

impl Handler<OutgoingMessage<Msg<OfflineProtocolMessage>>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: OutgoingMessage<Msg<OfflineProtocolMessage>>, ctx: &mut Context<Self>) {
        log::info!("broadcast {:?}", msg.message);
    }
}

impl Handler<OutgoingEnvelope> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: OutgoingEnvelope, ctx: &mut Context<Self>) {
        ctx.spawn(Self::send_one(msg).interop_actor(self));
    }
}

impl Handler<ProtocolOutput<CompletedOfflineStage>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutput<CompletedOfflineStage>, ctx: &mut Context<Self>) {
        log::info!("result {:?}", msg.output.public_key());
    }
}

impl StreamHandler<Result<IncomingEnvelope>> for Coordinator
{
    fn handle(&mut self, msg: Result<IncomingEnvelope>, ctx: &mut Context<Self>) {

        self.handle_incoming(msg, ctx);
    }
}
