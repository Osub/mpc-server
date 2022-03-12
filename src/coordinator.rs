use std::collections::HashMap;
use crate::messages::*;
use actix::prelude::*;
use crate::player::MpcPlayer;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{OfflineStage, CompletedOfflineStage, OfflineProtocolMessage};
use round_based::{Msg, StateMachine};

#[derive(Default)]
pub struct Coordinator {
    pub(crate) runners: HashMap<String, Recipient<ProtocolMessage<Msg<<OfflineStage as StateMachine>::MessageBody>>>>,
}

impl Coordinator {
    fn new() -> Self {
        Self {
            runners: HashMap::new()
        }
    }
}

impl Actor for Coordinator {
    type Context = SyncContext<Self>;
}

impl Handler<SignRequest> for Coordinator {
    type Result = ();

    fn handle(&mut self, req: SignRequest, ctx: &mut SyncContext<Self>) {
        let state = OfflineStage::new(req.i, req.s_l, req.local_key);
        match state {
            Ok(state) => {
                let player = MpcPlayer::new(
                    state,
                    req.room.clone(),
                    ctx.address().recipient(),
                    ctx.address().recipient(),
                    ctx.address().recipient(),
                );
                let addr = player.start();
                self.runners.insert("abc".to_owned(), addr.recipient());
            }
            Err(e) => {}
        }
    }
}

impl Handler<ProtocolError<<OfflineStage as StateMachine>::Err>> for Coordinator {
    type Result = ();

    fn handle(&mut self, msg: ProtocolError<<OfflineStage as StateMachine>::Err>, ctx: &mut SyncContext<Self>) {
        println!("Error {:?}", msg.error);
    }
}

impl Handler<OutgoingMessage<Msg<OfflineProtocolMessage>>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: OutgoingMessage<Msg<OfflineProtocolMessage>>, ctx: &mut SyncContext<Self>) {
        println!("broadcast {:?}", msg.message);
    }
}

impl Handler<ProtocolOutput<CompletedOfflineStage>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutput<CompletedOfflineStage>, ctx: &mut SyncContext<Self>) {
        println!("result {:?}", msg.output.public_key());
    }
}