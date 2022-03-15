use std::sync::mpsc::{channel, sync_channel};
use std::time::Duration;
use actix::prelude::*;
use actix_interop::{critical_section, FutureInterop, FutureInteropWrap, StreamInterop, with_ctx};
use actix_web::body::MessageBody;
use actix_web::cookie::time::Month::May;
use actix_web::error::ErrorNotFound;
use futures::{Sink, SinkExt, StreamExt, TryStream};
use tokio::time::{self};
use crate::messages::{Envelope, IncomingMessage, MaybeProceed, OutgoingEnvelope, OutgoingMessage, ProtocolError, ProtocolMessage, ProtocolOutput};
use round_based::{Msg, StateMachine};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use anyhow::{Context as AnyhowContext, Error, Result};
use futures::stream;
use futures_util::TryStreamExt;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::party_i::SignatureRecid;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{CompletedOfflineStage, PartialSignature, SignManual};
use curv::BigInt;

pub struct Signer<I: Send> {
    input: I,
    room: String,
    index: u16,
    t: usize,
    message: BigInt,
    completed_offline_stage: CompletedOfflineStage,
    partial_sigs: Vec<PartialSignature>,
    result_collector: Recipient<ProtocolOutput<I, SignatureRecid>>,
    message_broker: Recipient<OutgoingEnvelope>,
}

impl<I> Signer<I>
    where
        I: Send + Clone + Unpin + 'static,
{
    pub fn new(
        input: I,
        room: String,
        index: u16,
        t: usize,
        message: BigInt,
        completed_offline_stage: CompletedOfflineStage,
        result_collector: Recipient<ProtocolOutput<I, SignatureRecid>>,
        message_broker: Recipient<OutgoingEnvelope>,
    ) -> Self
    {
        Self {
            input,
            room,
            index,
            message,
            completed_offline_stage,
            t,
            partial_sigs: Vec::with_capacity(t),
            result_collector,
            message_broker,
        }
    }
    fn maybe_complete(&mut self, ctx: &mut Context<Self>) -> Result<()> {
        let message = self.message.clone();
        let completed_offline_stage = self.completed_offline_stage.clone();
        let (state, partial_sig) = SignManual::new(
            message,
            completed_offline_stage
        )?;
        match serde_json::to_string(&partial_sig) {
            Ok(serialized) => {
                log::debug!("Sending message {:?}", serde_json::to_string(&partial_sig));
                self.message_broker.do_send(OutgoingEnvelope {
                    room: self.room.clone(),
                    message: serialized,
                });
            }
            Err(_) => {}
        };
        if self.partial_sigs.len() == self.t {

            match state.complete(&self.partial_sigs) {
                Ok(signature) => {
                    self.result_collector.do_send(ProtocolOutput {
                        input: self.input.clone(),
                        output: signature
                    });
                }
                Err(e) => {
                    //TODO: Handle error.
                }
            };
        }
        Ok(())
    }
}

impl<I> Actor for Signer<I>
    where
        I: Send + Clone + Unpin + 'static,
{
    type Context = Context<Self>;
}

impl<I> Handler<IncomingMessage<Msg<PartialSignature>>> for Signer<I>
    where
        I: Send + Clone + Unpin + 'static,
{
    type Result = ();

    fn handle(&mut self, msg: IncomingMessage<Msg<PartialSignature>>, ctx: &mut Context<Self>) {
        self.partial_sigs.push(msg.message.body);
        self.maybe_complete(ctx);
    }
}
