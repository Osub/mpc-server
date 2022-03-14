// use std::{
//     collections::{HashMap, HashSet},
//     sync::{
//         atomic::{AtomicUsize, Ordering},
//         Arc,
//     },
// };

use std::future::Future;
use std::ops::Deref;
use std::path::Iter;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc::{channel, sync_channel};
use std::time::Duration;
use actix::prelude::*;
use actix_interop::{critical_section, FutureInterop, FutureInteropWrap, with_ctx};
use actix_web::cookie::time::Month::May;
use actix_web::error::ErrorNotFound;
use futures::{Sink, SinkExt, StreamExt, TryStream};
use tokio::time::{self};
use crate::messages::{MaybeProceed, OutgoingMessage, ProtocolError, ProtocolMessage, ProtocolOutput};
use round_based::{Msg, StateMachine};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use anyhow::{Context as AnyhowContext, Error, Result};
use futures::stream::iter;

pub struct MpcPlayer<SM, E: Send, M: Send, O: Send> {
    msgCount: u16,
    initialPoller: Option<Box<SpawnHandle>>,
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
    SM::MessageBody: Send + Serialize,
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
                msgCount: 0,
                sink: Some(sink.into()),
                coordinator,
                result_collector,
                deadline: None,
                current_round: None,
                state,
                initialPoller: None,
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
        SM::MessageBody: Send + Serialize,
        SM::Output: Send,
{
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.address().send(MaybeProceed{});
    }
    // fn started(&mut self, ctx: &mut Self::Context) {
    //     let handle = ctx.run_interval(Duration::from_millis(250), | _actor, _ctx|{
    //         if _actor.msgCount > 0 {
    //             if let Some(_h) = _actor.initialPoller.clone() {
    //                 _actor.initialPoller = None;
    //                 _ctx.cancel_future(*_h.deref());
    //             }
    //         } else {
    //             _ctx.address().send(MaybeProceed{});
    //         }
    //     });
    //     // let handle1 = handle.clone();
    //     self.initialPoller = Some(Box::new(handle));
    // }
}

impl<SM> MpcPlayer<SM, SM::Err, SM::MessageBody, SM::Output>
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

    fn get_outgoing_stream(&mut self)-> Pin<Box<dyn TryStream<Item=Result<Msg<SM::MessageBody>>, Ok=Msg<SM::MessageBody>, Error=anyhow::Error>+'_>>{
        Box::pin(futures::stream::iter(self.state.message_queue().drain(..)).map(Ok))
    }
    fn send_outgoing(&mut self, ctx: &mut Context<Self>){
        if !self.state.message_queue().is_empty() {
            ctx.spawn(self.send_outgoing1());
        }
        // self.send_outgoing1(ctx).spawn(ctx);
    }
    fn send_outgoing1(&mut self)-> FutureInteropWrap<Self, impl Future<Output=()>> {
        // FutureInteropWrap<Self, impl Future<Output=Result<()>>>
        // ->FutureInteropWrap<Self, Result<()>>
        let msg = self.state.message_queue().drain(..).next().context("Not found.");
        let m = msg.as_ref().expect("Msg");
        log::debug!("Sending message {:?}", serde_json::to_string(&m));
            async move {
                let (tx, rx) = oneshot::channel();

                // Perform sends in a critical section so they are strictly ordered
                critical_section::<Self, _>(async {
                    // Take the sink from the actor state
                    let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
                        .expect("Sink to be present");


                    let res: Result<()> =  match msg {
                        Ok(msg) => {
                            let res0 = sink.send(msg).await;
                            res0
                        }
                        Err(e) => Err(e)
                    };

                    // Send the request
                    // let res = sink.send(msg).await;

                    // Put the sink back, and if the send was successful,
                    // record the in-flight request.
                    with_ctx(|actor: &mut Self, _| {
                        actor.sink = Some(sink);
                        match res {
                            Ok(()) => {},
                            err => {
                                // Don't care if the receiver has gone away
                                let _ = tx.send(err);
                            }
                        }
                    });
                })
                    .await;

                // Wait for the result concurrently, so many requests can
                // be pipelined at the same time.
                // rx.await.expect("Sender should not be dropped"); // TODO: Check what is this doing and do we need it?
            }.interop_actor(self)

        // if !self.state.message_queue().is_empty() {
        //     // let x = self.state.message_queue().drain(..).into_iter();
        //     // let mut s: Iter<Item=Msg<SM::MessageBody>> =futures::stream::iter(
        //     //     self.state.message_queue().drain(..).into_iter()
        //     // );
        //
        //
        //     actix::fut::wrap_future::<_,Self>({
        //         let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
        //             .expect("Sink to be present");
        //         let mut msgs = with_ctx(|actor: &mut Self, _| actor.get_outgoing_stream());
        //         sink.send_all(&mut msgs)
        //
        //     });
        // }
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



    fn maybe_proceed(&mut self,  ctx: &mut Context<Self>) {
        self.send_outgoing(ctx);
        self.refresh_timer();

        self.proceed_if_needed();
        self.send_outgoing(ctx);
        self.refresh_timer();
        self.finish_if_possible();
    }
}


impl<SM> StreamHandler<Result<Msg<SM::MessageBody>>> for MpcPlayer<SM, SM::Err, SM::MessageBody, SM::Output>
    where
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize,
        SM::Output: Send,
{

    fn handle(&mut self, msg: Result<Msg<SM::MessageBody>>, ctx: &mut Context<Self>) {
        match msg {
            Ok(m) => {
                log::debug!("Received message {:?}", serde_json::to_string(&m));
                self.msgCount += 1;
                self.handle_incoming(m);
                self.maybe_proceed(ctx);
            }
            Err(e) => {
                // Ignore
            }
        }
    }
}

impl<SM> Handler<MaybeProceed> for MpcPlayer<SM, SM::Err, SM::MessageBody, SM::Output>
    where
        SM: StateMachine + Unpin,
        SM::Err: Send,
        SM: Send + 'static,
        SM::MessageBody: Send + Serialize,
        SM::Output: Send,
{
    type Result = ();

    fn handle(&mut self, msg: MaybeProceed, ctx: &mut Context<Self>) {
        if self.msgCount == 0 {
            log::debug!("Try to proceed");
            self.maybe_proceed(ctx);
            ctx.run_later(Duration::from_millis(250), | _, _ctx|{
                _ctx.address().do_send(MaybeProceed{});
            });
        } else {
            log::debug!("Ignore proceed");
        }
    }
}
