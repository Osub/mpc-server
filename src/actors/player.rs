use std::fmt::Debug;
use std::time::Duration;

use actix::prelude::*;
use kv_log_macro::debug;
use round_based::{Msg, StateMachine};
use serde::Serialize;
use tokio::time::{self};
use crate::actors::messages::CoordinatorMessage;

use crate::actors::msg_utils::describe_message;
use crate::core::CoreMessage;

use super::messages::{IncomingMessage, MaybeProceed, ProtocolError, ProtocolOutput};

pub(crate) struct MpcPlayer<I: Send + Clone + Unpin + 'static, SM, M: Send + Clone, E: Send, O: Send> {
    input: I,
    room: String,
    index: u16,
    msg_count: u16,
    coordinator: Recipient<ProtocolError<E, IncomingMessage<Msg<M>>>>,
    message_broker: Recipient<CoordinatorMessage>,
    state: SM,
    deadline: Option<time::Instant>,
    current_round: Option<u16>,
    result_collector: Recipient<ProtocolOutput<I, O>>,
}

impl<I, SM> MpcPlayer<I, SM, SM::MessageBody, SM::Err, SM::Output>
    where
        I: Send + Clone + Unpin + 'static,
        SM: StateMachine + Debug + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize + Clone,
        SM::Output: Send,
{
    pub(crate) fn new(
        input: I,
        room: String,
        index: u16,
        state: SM,
        coordinator: Recipient<ProtocolError<SM::Err, IncomingMessage<Msg<SM::MessageBody>>>>,
        result_collector: Recipient<ProtocolOutput<I, SM::Output>>,
        message_broker: Recipient<CoordinatorMessage>,
    ) -> Self
        where
            SM: StateMachine + Debug + Unpin,
    {
        debug!("Started player", {room: format!("\"{}\"", &room)});
        Self {
            input,
            room,
            index,
            msg_count: 0,
            coordinator,
            result_collector,
            deadline: None,
            current_round: None,
            state,
            message_broker,
        }
    }
    fn send_error(&mut self, error: SM::Err, message: IncomingMessage<Msg<SM::MessageBody>>) {
        debug!("Sending error");
        let _ = self.coordinator.do_send(ProtocolError { room: self.room.clone(), error, message });
    }
}

impl<I, SM> Actor for MpcPlayer<I, SM, SM::MessageBody, SM::Err, SM::Output>
    where
        I: Send + Clone + Unpin + 'static,
        SM: StateMachine + Debug + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize + Clone,
        SM::Output: Send,
{
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.notify(MaybeProceed {});
    }
}

impl<I, SM> MpcPlayer<I, SM, SM::MessageBody, SM::Err, SM::Output>
    where
        I: Send + Clone + Unpin + 'static,
        SM: StateMachine + Debug + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize + Clone,
        SM::Output: Send,
{
    fn handle_incoming(&mut self, msg: IncomingMessage<Msg<SM::MessageBody>>) {
        match self.state.handle_incoming(msg.message.clone()) {
            Ok(()) => {
                debug!("Handle Ok", { state: format!("\"{:?}\"", self.state) });
            }
            Err(e) => {
                debug!("Handle Err", { state: format!("\"{:?}\"", self.state) });
                self.send_error(e, msg)
            }
        }
    }

    fn proceed_if_needed(&mut self) -> Result<(), SM::Err> {
        if self.state.wants_to_proceed() {
            self.state.proceed()
        } else {
            Ok(())
        }
    }

    fn send_outgoing(&mut self, _ctx: &mut Context<Self>) {
        if !self.state.message_queue().is_empty() {
            debug!("Message queue size is {}", self.state.message_queue().len());
        }

        for msg in self.state.message_queue().drain(..) {
            if let Ok(serialized) = serde_json::to_string(&msg) {
                let _ = self.message_broker.do_send(CoordinatorMessage::Outgoing (CoreMessage{
                    room: self.room.clone(),
                    message: serialized,
                }));
            }
        }
    }

    fn send_result(&mut self, result: SM::Output) {
        let _ = self.result_collector.do_send(ProtocolOutput { input: self.input.clone(), output: result });
    }
    fn finish_if_possible(&mut self) -> Result<(), SM::Err> {
        if self.state.is_finished() {
            match self.state.pick_output() {
                Some(Ok(result)) => {
                    self.send_result(result);
                    Ok(())
                }
                Some(Err(e)) => { Err(e) }
                None => { Ok(()) }
            }
        } else {
            Ok(())
        }
    }

    fn refresh_timer(&mut self) {
        let round_n = self.state.current_round();
        if self.current_round != Some(round_n) {
            self.current_round = Some(round_n);
            self.deadline = self.state.round_timeout().map(|timeout| time::Instant::now() + timeout)
        }
    }


    fn maybe_proceed(&mut self, ctx: &mut Context<Self>) -> Result<(), SM::Err> {
        self.send_outgoing(ctx);
        self.refresh_timer();

        match self.proceed_if_needed() {
            Ok(()) => {}
            Err(e) => { return Err(e); }
        }
        self.send_outgoing(ctx);
        self.refresh_timer();
        let _ = self.finish_if_possible();
        Ok(())
    }
}


impl<I, SM> Handler<IncomingMessage<Msg<SM::MessageBody>>> for MpcPlayer<I, SM, SM::MessageBody, SM::Err, SM::Output>
    where
        I: Send + Clone + Unpin + 'static,
        SM: StateMachine + Debug + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize + Clone,
        SM::Output: Send,
{
    type Result = ();

    fn handle(&mut self, msg: IncomingMessage<Msg<SM::MessageBody>>, ctx: &mut Context<Self>) {
        if msg.message.sender == self.index {
            return;
        }
        let round_before = self.state.current_round();
        let valid_receiver = match msg.message.receiver {
            Some(i) if i != self.index => { false }
            _ => { true }
        };
        if !valid_receiver {
            // TODO: Is this necessary? Other option is let state machine take the message and throw error.
            // log::debug!("The message is not addressed to me. {} {}", msg.message.receiver.unwrap(), self.index);
            return;
        }
        self.msg_count += 1;
        self.handle_incoming(msg.clone());
        if let Err(e) = self.maybe_proceed(ctx) {
            self.send_error(e, msg.clone())
        }

        debug!("Received message", {
            protocolMessage: describe_message(&serde_json::to_string(&msg.message).unwrap()),
            round_before: round_before,
            round_after: self.state.current_round(),
            state: format!("\"{:?}\"", self.state)
        });
        // log::debug!("Round after: {}", self.state.current_round());
    }
}

impl<I, SM> Handler<MaybeProceed> for MpcPlayer<I, SM, SM::MessageBody, SM::Err, SM::Output>
    where
        I: Send + Clone + Unpin + 'static,
        SM: StateMachine + Debug + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize + Clone,
        SM::Output: Send,
{
    type Result = ();

    fn handle(&mut self, _: MaybeProceed, ctx: &mut Context<Self>) {
        if self.msg_count == 0 {
            // log::debug!("Try to proceed");
            let _ = self.maybe_proceed(ctx);
            ctx.run_later(Duration::from_millis(250), |_, _ctx| {
                _ctx.address().do_send(MaybeProceed {});
            });
        } else {
            // log::debug!("Ignore proceed");
        }
    }
}
