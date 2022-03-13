// use std::{
//     collections::{HashMap, HashSet},
//     sync::{
//         atomic::{AtomicUsize, Ordering},
//         Arc,
//     },
// };

use std::pin::Pin;
use actix::prelude::*;
use actix_interop::with_ctx;
use futures::{Sink, SinkExt};
use tokio::time::{self};
use crate::messages::{OutgoingMessage, ProtocolError, ProtocolMessage, ProtocolOutput};
use round_based::{Msg, StateMachine};

use anyhow::{Result};

pub struct MpcPlayer<SM, E: Send, M: Send, O: Send> {
    sink: Option<Pin<Box<dyn Sink<Msg<M>, Error = anyhow::Error>>>>,
    coordinator: Recipient<ProtocolError<E>>,
    state: SM,
    deadline: Option<time::Instant>,
    current_round: Option<u16>,
    result_collector: Recipient<ProtocolOutput<O>>,
}

impl<SM> MpcPlayer<SM, SM::Err, SM::MessageBody, SM::Output>
where
    SM: StateMachine + Unpin,
    SM::Err: Send,
    SM: Send + 'static,
    SM::MessageBody: Send,
    SM::Output: Send,
{
    pub fn new<Si, St>(
        state: SM,
        sink: Si,
        stream: St,
        coordinator: Recipient<ProtocolError<SM::Err>>,
        result_collector: Recipient<ProtocolOutput<SM::Output>>
    ) -> Addr<Self>
        where
            SM: StateMachine + Unpin,
            Si: Sink<Msg<SM::MessageBody>, Error = anyhow::Error> + 'static,
            St: Stream<Item = Result<Msg<SM::MessageBody>>> + 'static,
    {
        let sink: Box<dyn Sink<Msg<SM::MessageBody>, Error = anyhow::Error>> = Box::new(sink);

        Self::create(|ctx| {
            ctx.add_stream(stream);
            Self {
                sink: Some(sink.into()),
                coordinator,
                result_collector,
                deadline: None,
                current_round: None,
                state,
            }
        })
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
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send,
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

    fn send_outgoing(&mut self) {
        if !self.state.message_queue().is_empty() {
            let msgs = self.state.message_queue().drain(..);

            for msg in msgs {
                let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
                    .expect("Sink to be present");
                sink.send(msg);
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


impl<SM> StreamHandler<Result<Msg<SM::MessageBody>>> for MpcPlayer<SM, SM::Err, SM::MessageBody, SM::Output>
    where
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send,
        SM::Output: Send,
{
    fn handle(&mut self, msg: Result<Msg<SM::MessageBody>>, _ctx: &mut Context<Self>) {
        match msg {
            Ok(m) => {
                self.handle_incoming(m);
                self.send_outgoing();
                self.refresh_timer();

                self.proceed_if_needed();
                self.send_outgoing();
                self.refresh_timer();
                self.finish_if_possible();
            }
            Err(e) => {
                // Ignore
            }
        }
    }
}