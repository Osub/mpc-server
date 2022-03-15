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

pub struct MpcPlayer<SM, E: Send, O: Send> {
    room: String,
    index: u16,
    msg_count: u16,
    initial_poller: Option<Box<SpawnHandle>>,
    coordinator: Recipient<ProtocolError<E>>,
    message_broker: Recipient<OutgoingEnvelope>,
    state: SM,
    deadline: Option<time::Instant>,
    current_round: Option<u16>,
    result_collector: Recipient<ProtocolOutput<O>>,
}

impl<SM> MpcPlayer<SM, SM::Err, SM::Output>
    where
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize,
        SM::Output: Send,
{
    pub fn new(
        room: String,
        index: u16,
        state: SM,
        coordinator: Recipient<ProtocolError<SM::Err>>,
        result_collector: Recipient<ProtocolOutput<SM::Output>>,
        message_broker: Recipient<OutgoingEnvelope>,
    ) -> Self
        where
            SM: StateMachine + Unpin,
    {
        Self {
            room,
            index,
            msg_count: 0,
            coordinator,
            result_collector,
            deadline: None,
            current_round: None,
            state,
            initial_poller: None,
            message_broker,
        }
    }
    fn send_error(&mut self, error: SM::Err) {
        log::debug!("Sending error");
        self.coordinator.do_send(ProtocolError { error });
    }
}

impl<SM> Actor for MpcPlayer<SM, SM::Err, SM::Output>
    where
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize,
        SM::Output: Send,
{
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.notify(MaybeProceed {});
    }
}

impl<SM> MpcPlayer<SM, SM::Err, SM::Output>
    where
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize,
        SM::Output: Send,
{
    fn handle_incoming(&mut self, msg: Msg<SM::MessageBody>) {
        match self.state.handle_incoming(msg) {
            Ok(()) => {}
            Err(e) => { self.send_error(e) }
        }
    }

    fn proceed_if_needed(&mut self) {
        if self.state.wants_to_proceed() {
            match self.state.proceed() {
                Ok(()) => {}
                Err(e) => { self.send_error(e) }
            }
        }
    }

    fn send_outgoing(&mut self, _ctx: &mut Context<Self>) {
        if !self.state.message_queue().is_empty() {
            log::debug!("Message queue size is {}", self.state.message_queue().len());
        }

        for msg in self.state.message_queue().drain(..) {
            match serde_json::to_string(&msg) {
                Ok(serialized) => {
                    log::debug!("Sending message {:?}", serde_json::to_string(&msg));
                    self.message_broker.do_send(OutgoingEnvelope {
                        room: self.room.clone(),
                        message: serialized,
                    });
                }
                Err(_) => {}
            }
        }
    }

    fn send_result(&mut self, result: SM::Output) {
        self.result_collector.do_send(ProtocolOutput { output: result });
    }
    fn finish_if_possible(&mut self) {
        if self.state.is_finished() {
            match self.state.pick_output() {
                Some(Ok(result)) => { self.send_result(result); }
                Some(Err(err)) => { self.send_error(err); }
                None => {}
            }
        }
    }

    fn refresh_timer(&mut self) {
        let round_n = self.state.current_round();
        if self.current_round != Some(round_n) {
            self.current_round = Some(round_n);
            self.deadline = match self.state.round_timeout() {
                Some(timeout) => Some(time::Instant::now() + timeout),
                None => None,
            }
        }
    }


    fn maybe_proceed(&mut self, ctx: &mut Context<Self>) {
        self.send_outgoing(ctx);
        self.refresh_timer();

        self.proceed_if_needed();
        self.send_outgoing(ctx);
        self.refresh_timer();
        self.finish_if_possible();
    }
}


impl<SM> Handler<IncomingMessage<Msg<SM::MessageBody>>> for MpcPlayer<SM, SM::Err, SM::Output>
    where
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize,
        SM::Output: Send,
{
    type Result = ();

    fn handle(&mut self, msg: IncomingMessage<Msg<SM::MessageBody>>, ctx: &mut Context<Self>) {
        if msg.message.sender == self.index {
            return;
        }
        log::debug!("Round before: {}", self.state.current_round());
        log::debug!("Received message {:?}", serde_json::to_string(&msg.message));
        self.msg_count += 1;
        self.handle_incoming(msg.message);
        self.maybe_proceed(ctx);
        log::debug!("Round after: {}", self.state.current_round());
    }
}

impl<SM> Handler<MaybeProceed> for MpcPlayer<SM, SM::Err, SM::Output>
    where
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize,
        SM::Output: Send,
{
    type Result = ();

    fn handle(&mut self, _: MaybeProceed, ctx: &mut Context<Self>) {
        if self.msg_count == 0 {
            log::debug!("Try to proceed");
            self.maybe_proceed(ctx);
            ctx.run_later(Duration::from_millis(250), |_, _ctx| {
                _ctx.address().do_send(MaybeProceed {});
            });
        } else {
            log::debug!("Ignore proceed");
        }
    }
}
