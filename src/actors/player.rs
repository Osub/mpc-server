use std::time::Duration;

use actix::prelude::*;
use round_based::{Msg, StateMachine};
use serde::Serialize;
use tokio::time::{self};

use super::messages::{IncomingMessage, MaybeProceed, OutgoingEnvelope, ProtocolError, ProtocolOutput};

pub struct MpcPlayer<I: Send + Clone + Unpin + 'static, SM, E: Send, O: Send> {
    input: I,
    room: String,
    index: u16,
    msg_count: u16,
    coordinator: Recipient<ProtocolError<E>>,
    message_broker: Recipient<OutgoingEnvelope>,
    state: SM,
    deadline: Option<time::Instant>,
    current_round: Option<u16>,
    result_collector: Recipient<ProtocolOutput<I, O>>,
}

impl<I, SM> MpcPlayer<I, SM, SM::Err, SM::Output>
    where
        I: Send + Clone + Unpin + 'static,
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize,
        SM::Output: Send,
{
    pub fn new(
        input: I,
        room: String,
        index: u16,
        state: SM,
        coordinator: Recipient<ProtocolError<SM::Err>>,
        result_collector: Recipient<ProtocolOutput<I, SM::Output>>,
        message_broker: Recipient<OutgoingEnvelope>,
    ) -> Self
        where
            SM: StateMachine + Unpin,
    {
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
    fn send_error(&mut self, error: SM::Err) {
        log::debug!("Sending error");
        let _ = self.coordinator.do_send(ProtocolError { error });
    }
}

impl<I, SM> Actor for MpcPlayer<I, SM, SM::Err, SM::Output>
    where
        I: Send + Clone + Unpin + 'static,
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

impl<I, SM> MpcPlayer<I, SM, SM::Err, SM::Output>
    where
        I: Send + Clone + Unpin + 'static,
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
                    let _ = self.message_broker.do_send(OutgoingEnvelope {
                        room: self.room.clone(),
                        message: serialized,
                    });
                }
                Err(_) => {}
            }
        }
    }

    fn send_result(&mut self, result: SM::Output) {
        let _ = self.result_collector.do_send(ProtocolOutput { input: self.input.clone(), output: result });
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


impl<I, SM> Handler<IncomingMessage<Msg<SM::MessageBody>>> for MpcPlayer<I, SM, SM::Err, SM::Output>
    where
        I: Send + Clone + Unpin + 'static,
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

impl<I, SM> Handler<MaybeProceed> for MpcPlayer<I, SM, SM::Err, SM::Output>
    where
        I: Send + Clone + Unpin + 'static,
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
                let _ = _ctx.address().do_send(MaybeProceed {});
            });
        } else {
            log::debug!("Ignore proceed");
        }
    }
}
