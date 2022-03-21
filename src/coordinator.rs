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
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{OfflineStage, Error as OfflineStageError, CompletedOfflineStage, OfflineProtocolMessage, SignManual, PartialSignature};
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
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::{Keygen, Error as KeygenError, ProtocolMessage as KeygenProtocolMessage};
use crate::group::{MpcGroup, PublicKeyGroup};
use thiserror::Error;
use crate::coordinator::GroupError::WrongPublicKey;

#[derive(Debug, Error)]
enum GroupError {
    #[error("Public key doesn't belong to the group.")]
    WrongPublicKey,
}

pub struct Coordinator {
    groups: HashMap<String, PublicKeyGroup>,
    keygen_runners: HashMap<String, Addr<MpcPlayer<KeygenRequest, Keygen, <Keygen as StateMachine>::Err, <Keygen as StateMachine>::Output>>>,
    offline_state_runners: HashMap<String, Addr<MpcPlayer<SignRequest, OfflineStage, <OfflineStage as StateMachine>::Err, <OfflineStage as StateMachine>::Output>>>,
    signers: HashMap<String, Addr<Signer<SignRequest>>>,
    sink: Option<Pin<Box<dyn Sink<Envelope, Error=anyhow::Error>>>>,
}

impl Coordinator {
    pub fn new<Si, St>(stream: St, sink: Si) -> Addr<Self>
        where
            St: Stream<Item=Result<SignedEnvelope<String>>> + 'static,
            Si: Sink<Envelope, Error=anyhow::Error> + 'static,
    {
        let stream = stream.and_then(|msg| async move {
            Ok(IncomingEnvelope {
                room: msg.room,
                message: msg.message,
                sender_public_key: msg.sender_public_key,
            })
        });
        let sink: Box<dyn Sink<Envelope, Error=anyhow::Error>> = Box::new(sink);

        Self::create(|ctx| {
            ctx.add_stream(stream);
            Self {
                groups: HashMap::new(),
                keygen_runners: HashMap::new(),
                offline_state_runners: HashMap::new(),
                signers: HashMap::new(),
                sink: Some(sink.into()),
            }
        })
    }

    fn valid_sender(&mut self, msg: IncomingEnvelope) -> Result<()> {
        let room = msg.room;
        let group = self.groups.get(&room).context("Can't found group.")?;
        group.get_index(&msg.sender_public_key).map_or(Err(WrongPublicKey), |_|Ok(())).context("Validate sender.")?;
        Ok(())
    }

    fn handle_incoming_keygen(&mut self, msg: IncomingEnvelope, ctx: &mut Context<Self>) -> Result<()> {
        let room = msg.room;
        let addr = self.keygen_runners.get(&room).context("Can't found mpc player.")?;
        let msg = serde_json::from_str::<Msg<KeygenProtocolMessage>>(&msg.message).context("deserialize message")?;
        addr.do_send(IncomingMessage {
            room: room.clone(),
            message: msg,
        });
        Ok(())
    }

    fn handle_incoming_offline(&mut self, msg: IncomingEnvelope, ctx: &mut Context<Self>) -> Result<()> {
        let room = msg.room;
        let addr = self.offline_state_runners.get(&room).context("Not found mpc player.")?;
        let msg = serde_json::from_str::<Msg<OfflineProtocolMessage>>(&msg.message).context("deserialize message")?;
        addr.do_send(IncomingMessage {
            room: room.clone(),
            message: msg,
        });

        Ok(())
    }

    fn handle_incoming_sign(&mut self, msg: IncomingEnvelope, ctx: &mut Context<Self>) -> Result<()> {
        let room = msg.room;
        let addr = self.signers.get(&room).context("Not found signer.")?;
        let msg = serde_json::from_str::<Msg<PartialSignature>>(&msg.message).context("deserialize message")?;
        addr.do_send(IncomingMessage {
            room: room.clone(),
            message: msg,
        });
        Ok(())
    }

    fn handle_incoming(&mut self, msg: IncomingEnvelope, ctx: &mut Context<Self>) {
        match self.valid_sender(msg.clone()) {
            Ok(()) => {
                let h1= self.handle_incoming_offline(msg.clone(), ctx);
                let h2 = self.handle_incoming_sign(msg.clone(), ctx);
                let h3 = self.handle_incoming_keygen(msg.clone(), ctx);
                if h1.or(h2).or(h3).is_err() {
                    ctx.run_later(Duration::from_secs(1), move|a, _ctx| {
                        _ctx.notify(RetryEnvelope {
                            room: msg.room.clone(),
                            message: msg.message.clone(),
                            sender_public_key: msg.sender_public_key.clone()
                        });
                    });
                }
            }
            Err(_)=>{
                // Do nothing
            }
        }

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

impl Handler<KeygenRequest> for Coordinator {
    type Result = Result<()>;

    fn handle(&mut self, req: KeygenRequest, ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Received request {:?}", req);
        let KeygenRequest{public_keys, t, own_public_key} = req.clone();
        let group = PublicKeyGroup::new(public_keys, t , own_public_key);
        let group_id = group.get_group_id();

        let state = Keygen::new(group.get_i(), group.get_t(), group.get_n()).context("Create state machine")?;
        let player = MpcPlayer::new(
            req.clone(),
            group_id.clone(),
            group.get_i(),
            state,
            ctx.address().recipient(),
            ctx.address().recipient(),
            ctx.address().recipient(),
        ).start();
        self.groups.insert(group_id.clone(), group);
        self.keygen_runners.insert(group_id.clone(), player);
        Ok(())
    }
}

impl Handler<SignRequest> for Coordinator {
    type Result = Result<()>;

    fn handle(&mut self, req: SignRequest, ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Received request {:?}", req);

        let state = OfflineStage::new(req.i, req.s_l.clone(), req.local_key.clone()).context("Create state machine")?;
        let player = MpcPlayer::new(
            req.clone(),
            req.room.clone(),
            req.i,
            state,
            ctx.address().recipient(),
            ctx.address().recipient(),
            ctx.address().recipient(),
        ).start();
        self.offline_state_runners.insert(req.room.to_owned(), player);
        Ok(())
    }
}

impl Handler<ProtocolError<KeygenError>> for Coordinator {
    type Result = ();

    fn handle(&mut self, msg: ProtocolError<KeygenError>, _: &mut Context<Self>) {
        log::info!("Error {:?}", msg.error);
    }
}

impl Handler<ProtocolError<OfflineStageError>> for Coordinator {
    type Result = ();

    fn handle(&mut self, msg: ProtocolError<OfflineStageError>, _: &mut Context<Self>) {
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

impl Handler<ProtocolOutput<KeygenRequest, <Keygen as StateMachine>::Output>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutput<KeygenRequest, <Keygen as StateMachine>::Output>, ctx: &mut Context<Self>) {
        log::info!("Public key is {:?}", msg.output.public_key());
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
        self.handle_incoming(IncomingEnvelope {
            room: msg.room,
            message: msg.message,
            sender_public_key: msg.sender_public_key,
        }, ctx);
    }
}
