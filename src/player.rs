// use std::{
//     collections::{HashMap, HashSet},
//     sync::{
//         atomic::{AtomicUsize, Ordering},
//         Arc,
//     },
// };

use actix::prelude::*;
use tokio::time::{self};
use crate::messages::{OutgoingMessage, ProtocolError, ProtocolMessage, ProtocolOutput};
use round_based::{Msg, StateMachine};

#[derive(Debug)]
pub struct MpcPlayer<SM, E: Send, M: Send, O: Send> {
    room: String,
    coordinator: Recipient<ProtocolError<E>>,
    state: SM,
    deadline: Option<time::Instant>,
    current_round: Option<u16>,
    message_hub: Recipient<OutgoingMessage<Msg<M>>>,
    result_collector: Recipient<ProtocolOutput<O>>,
}

impl<SM> MpcPlayer<SM, SM::Err, SM::MessageBody, SM::Output>
where
    SM: StateMachine,
    SM::Err: Send,
    SM: Send + 'static,
    SM::MessageBody: Send,
    SM::Output: Send,
{
    pub fn new(state: SM, room: String, coordinator: Recipient<ProtocolError<SM::Err>>, message_hub: Recipient<OutgoingMessage<Msg<SM::MessageBody>>>, result_collector: Recipient<ProtocolOutput<SM::Output>>) -> Self {
        Self {
            state,
            room,
            coordinator,
            message_hub,
            result_collector,
            deadline: None,
            current_round: None,
        }
    }
    fn send_error(&mut self, error: SM::Err) {
        self.coordinator.do_send(ProtocolError{error});
    }
}

impl<SM> Actor for MpcPlayer<SM, SM::Err, SM::MessageBody, SM::Output>
    where
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send,
        SM::Output: Send,
{
    type Context = Context<Self>;
}

impl<SM> MpcPlayer<SM, SM::Err, SM::MessageBody, SM::Output>
    where
        SM: StateMachine,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send,
        SM::Output: Send,
{
    fn handle_incoming(&mut self, msg: ProtocolMessage<Msg<SM::MessageBody>>) {
          match self.state.handle_incoming(msg.message) {
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

    fn send_outgoing(&mut self) {
        if !self.state.message_queue().is_empty() {
            let msgs = self.state.message_queue().drain(..);

            for msg in msgs {
                self.message_hub.do_send(OutgoingMessage {room: self.room.clone(), message: msg});
            }
        }
    }
    fn send_result(&mut self, result: SM::Output) {
        self.result_collector.do_send(ProtocolOutput{ output: result});
    }
    fn finish_if_possible(&mut self) {
        if self.state.is_finished() {
            match self.state.pick_output() {
                Some(Ok(result)) => { self.send_result(result); }
                Some(Err(err)) => { self.send_error(err); }
                None => {
                    // self.send_error(BadStateMachineReason::ProtocolFinishedButNoResult.into())
                }
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
}


impl<SM> Handler<ProtocolMessage<Msg<SM::MessageBody>>> for MpcPlayer<SM, SM::Err, SM::MessageBody, SM::Output>
    where
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send,
        SM::Output: Send,
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolMessage<Msg<SM::MessageBody>>, _: &mut Context<Self>) -> Self::Result {
        self.handle_incoming(msg);
        self.send_outgoing();
        self.refresh_timer();

        self.proceed_if_needed();
        self.send_outgoing();
        self.refresh_timer();
        self.finish_if_possible();
    }
}


