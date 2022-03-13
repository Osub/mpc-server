use std::collections::HashMap;
use std::sync::Arc;
use crate::messages::*;
use actix::prelude::*;
use crate::player::MpcPlayer;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{OfflineStage, CompletedOfflineStage, OfflineProtocolMessage};
use round_based::{Msg, StateMachine};
use crate::transport::join_computation;
use anyhow::{Context as AnyhowContext, Error, Result};
use serde_json::ser::State;
use futures::future;
use surf::Url;

pub struct Coordinator {
    pub(crate) messenger_address: surf::Url,
    pub(crate) runners: HashMap<String, Addr<MpcPlayer<OfflineStage, <OfflineStage as StateMachine>::Err,  <OfflineStage as StateMachine>::MessageBody,  <OfflineStage as StateMachine>::Output>>>,
}

impl Coordinator {
    pub fn new(messenger_address: surf::Url) -> Self {
        Self {
            messenger_address,
            runners: HashMap::new(),
        }
    }
}

impl Actor for Coordinator {
    type Context = Context<Self>;
}

impl Handler<SignRequest> for Coordinator {
    type Result = ResponseActFuture<Self, Result<()>>;

    fn handle(&mut self, req: SignRequest, ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Received request {:?}", req);
        let addr = self.messenger_address.clone();

        let req0 = Arc::new(SignRequest{
            room: req.room.clone(),
            i: req.i.clone(),
            s_l: req.s_l.clone(),
            local_key: req.local_key.clone(),
        });
        let url = addr.join(&format!("rooms/{}/", req.room.clone())).unwrap();
        let jc = actix::fut::wrap_future::<_, Self>(join_computation::<<OfflineStage as StateMachine>::MessageBody>(url));

        let update_self = jc.map(move |result, actor, _ctx| {

            let (i, incoming, outgoing) = result.context("Join computation")?;
            log::info!("Index is {}", i);
            let state = OfflineStage::new(req0.i, req0.s_l.clone(), req0.local_key.clone()).context("Create state machine")?;
            let player = MpcPlayer::new(
                state,
                outgoing,
                incoming,
                _ctx.address().recipient(),
                _ctx.address().recipient(),
            );
            actor.runners.insert(req0.room.to_owned(), player);
            Ok(())
        });
        Box::pin(update_self)
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

impl Handler<ProtocolOutput<CompletedOfflineStage>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutput<CompletedOfflineStage>, ctx: &mut Context<Self>) {
        log::info!("result {:?}", msg.output.public_key());
    }
}