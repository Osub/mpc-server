use std::borrow::Borrow;
use std::collections::HashMap;
use crate::messages::*;
use actix::prelude::*;
use anyhow::{Context, Result};
use futures::{AsyncWriteExt, Sink, SinkExt, TryStreamExt};
use crate::player::MpcPlayer;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{OfflineStage, CompletedOfflineStage, OfflineProtocolMessage};
use round_based::{Msg, StateMachine};
use crate::transport::join_computation;

pub struct MsgHub {
    pub(crate) runners: HashMap<String, Recipient<ProtocolMessage<Msg<<OfflineStage as StateMachine>::MessageBody>>>>,
    pub(crate) outgoings: HashMap<(String, u16), Box<Sink<Msg<OfflineStage>, Error = anyhow::Error>>>,
    pub(crate) url: surf::Url,
}

impl MsgHub {
    fn new(url: surf::Url) -> Self {
        Self {
            runners: HashMap::new(),
            outgoings: HashMap::new(),
            url
        }
    }
}

impl Actor for MsgHub {
    type Context = SyncContext<Self>;
}

impl MsgHub {
    async fn join(&mut self, room: String, recipient: Recipient<IncomingMessage<Msg<OfflineProtocolMessage>>>) -> Result<()> {
        let (i, incoming, outgoing) =
            join_computation::<OfflineProtocolMessage>(self.url.clone(), &format!("{}-offline", room))
                .await
                .context("join offline computation")?;
        incoming.and_then(move |message|{
            recipient.clone().do_send(IncomingMessage{ room, message});
            futures::future::ready(Ok(()))
        });
        self.outgoings.insert((room.clone(), i), Box::pin(outgoing));
        Ok(())
    }
}

impl Handler<IncomingMessage<Msg<OfflineProtocolMessage>>> for MsgHub
{
    type Result = ();

    fn handle(&mut self, msg: IncomingMessage<Msg<OfflineProtocolMessage>>, ctx: &mut SyncContext<Self>) {
        println!("broadcast {:?}", msg.message);
    }
}

impl Handler<OutgoingMessage<Msg<OfflineProtocolMessage>>> for MsgHub
{
    type Result = ();

    fn handle(&mut self, msg: OutgoingMessage<Msg<OfflineProtocolMessage>>, ctx: &mut SyncContext<Self>) {
        println!("broadcast {:?}", msg.message);
        let key = (msg.room, msg.message.sender);
        let mut toSend = [msg.message];
        match self.outgoings.get(&key) {
            Some(outgoing) => {outgoing.as_mut().send_all(&mut toSend);}
        };
    }
}
