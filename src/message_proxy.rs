use std::collections::VecDeque;
use std::pin::Pin;

use futures::{Sink, SinkExt, Stream, channel::oneshot};
use actix::prelude::*;

use actix_interop::{FutureInterop, with_ctx, critical_section};
use round_based::{Msg, StateMachine};
use crate::messages::ProtocolMessage;

// Define our actor
pub struct PipelineAdapter<O: Send, I: Send, Err: Send> {
    sink: Option<Pin<Box<dyn Sink<O, Error=Err>>>>,
    in_flight_reqs: VecDeque<oneshot::Sender<Result<I, Err>>>,
    room: String,
    processor: Recipient<ProtocolMessage<I>>,
}

// Implement a constructor
impl<O, I, Err> PipelineAdapter<O, I, Err>
    where
        O: Send + 'static,
        I: Send + 'static,
        Err: Send + 'static,
{
    // pub fn new<Si, St>(sink: Si, stream: St, room: String, processor: Recipient<ProtocolMessage<I>>) -> Addr<Self>
    //     where
    //         Si: Sink<O, Error=Err> + 'static,
    //         St: Stream<Item=I> + 'static,
    // {
    //     // Convert to a boxed trait object
    //     let sink: Box<dyn Sink<O, Error=Err>> = Box::new(sink);
    //
    //     PipelineAdapter::create(|ctx| {
    //         ctx.add_stream(stream);
    //         Self {
    //             sink: Some(sink.into()),
    //             in_flight_reqs: VecDeque::new(),
    //             room,
    //             processor,
    //         }
    //     })
    // }
    pub fn new<SM, Err>(sink: Si, stream: St, room: String, processor: Recipient<ProtocolMessage<Msg<SM::MessageBody>>>) -> Addr<Self>
        where
            SM: StateMachine,
            SM::MessageBody: Send + 'static,
            Err: Send + 'static,
            Si: Sink<Msg<SM::MessageBody>, Error=Err> + 'static,
            St: Stream<Item=Msg<SM::MessageBody>> + 'static,
    {
        // Convert to a boxed trait object
        let sink: Box<dyn Sink<O, Error=Err>> = Box::new(sink);

        PipelineAdapter::create(|ctx| {
            ctx.add_stream(stream);
            Self {
                sink: Some(sink.into()),
                in_flight_reqs: VecDeque::new(),
                room,
                processor,
            }
        })
    }
}


// Tell actix this is an actor using the default Context type
impl<O, I, Err> Actor for  PipelineAdapter<O, I, Err>
    where
        O: Send + 'static,
        I: Send + 'static,
        Err: Send + 'static,
{
    type Context = Context<Self>;
}

// Transform actix messages into the pipelines request/response protocol
impl<SM, Err> Handler<ProtocolMessage<Msg<O>>> for PipelineAdapter<Msg<M>, Msg<M>, Err>
    where
        M: Send + 'static,
        Err: Send + 'static,
{
    type Result = ResponseActFuture<Self, Result<Msg<SM::MessageBody>, Err>>; // <- Message response type

    fn handle(&mut self, msg: ProtocolMessage<Msg<SM::MessageBody>>, _ctx: &mut Context<Self>) -> Self::Result {
        async move {
            let (tx, rx) = oneshot::channel();

            // Perform sends in a critical section so they are strictly ordered
            critical_section::<Self, _>(async {
                // Take the sink from the actor state
                let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
                    .expect("Sink to be present");

                // Send the request
                let res = sink.send(msg.message).await;

                // Put the sink back, and if the send was successful,
                // record the in-flight request.
                with_ctx(|actor: &mut Self, _| {
                    actor.sink = Some(sink);
                    match res {
                        Ok(()) => actor.in_flight_reqs.push_back(tx),
                        Err(e) => {
                            // Don't care if the receiver has gone away
                            let _ = tx.send(Err(e));
                        }
                    }
                });
            })
                .await;

            // Wait for the result concurrently, so many requests can
            // be pipelined at the same time.
            rx.await.expect("Sender should not be dropped")
        }
            .interop_actor_boxed(self)
    }
}

// Process responses
impl<SM, Err> StreamHandler<ProtocolMessage<Msg<SM::MessageBody>>> for PipelineAdapter<Msg<SM::MessageBody>, Msg<SM::MessageBody>, Err>
    where
        SM: StateMachine,
        SM::MessageBody: Send + 'static,
        Err: Send + 'static,
{
    fn handle(&mut self, msg: Msg<SM::MessageBody>, _ctx: &mut Context<Self>) {

        self.processor.do_send(ProtocolMessage{ room: self.room.clone(), message: msg.clone()});

        // // When we receive a response, just pull the first in-flight
        // // request and forward on the result.
        // let _ = self.in_flight_reqs
        //     .pop_front()
        //     .expect("There to be an in-flight request")
        //     .send(Ok(msg));

    }
}