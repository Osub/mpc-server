use std::collections::VecDeque;
use std::pin::Pin;

use futures::{Sink, SinkExt, Stream, channel::oneshot};
use actix::prelude::*;

use actix_interop::{FutureInterop, with_ctx, critical_section};
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::OfflineStage;
use round_based::{Msg, StateMachine};
use crate::messages::{IncomingMessage, ProtocolMessage};
use anyhow::{Result};
use anyhow::{Context as AnyhowContext};


type OfflineMessage = Msg<<OfflineStage as StateMachine>::MessageBody>;

// Define our actor
pub struct PipelineAdapter<M: Send> {
    sink: Option<Pin<Box<dyn Sink<Msg<M>, Error = anyhow::Error>>>>,
    in_flight_reqs: VecDeque<oneshot::Sender<Result<Msg<M>>>>,
    room: String,
    processor: Recipient<ProtocolMessage<Msg<M>>>,
}

// Implement a constructor
impl<M> PipelineAdapter<M>
    where
        M: Send + 'static,
{
    pub fn new<Si, St>(sink: Si, stream: St, room: String, processor: Recipient<ProtocolMessage<Msg<M>>>) -> Addr<Self>
        where
            Si: Sink<Msg<M>, Error = anyhow::Error> + 'static,
            St: Stream<Item = Result<Msg<M>>> + 'static,
    {
        // Convert to a boxed trait object
        let sink: Box<dyn Sink<Msg<M>, Error = anyhow::Error>> = Box::new(sink);

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
impl<M> Actor for  PipelineAdapter<M>
    where
        M: Send + 'static,
{
    type Context = Context<Self>;
}

// Transform actix messages into the pipelines request/response protocol
// impl<M> Handler<ProtocolMessage<Msg<M>>> for PipelineAdapter<M>
//     where
//         M: Send + 'static,
// {
//     type Result = ResponseActFuture<Self, ()>; // <- Message response type
//
//     fn handle(&mut self, msg: ProtocolMessage<Msg<M>>, _ctx: &mut Context<Self>)  {
//         async move {
//             let (tx, rx) = oneshot::channel();
//
//             // Perform sends in a critical section so they are strictly ordered
//             critical_section::<Self, _>(async {
//                 // Take the sink from the actor state
//                 let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
//                     .expect("Sink to be present");
//
//                 // Send the request
//                 let res = sink.send(msg.message).await;
//
//                 // Put the sink back, and if the send was successful,
//                 // record the in-flight request.
//                 with_ctx(|actor: &mut Self, _| {
//                     actor.sink = Some(sink);
//                     match res {
//                         Ok(_) => actor.in_flight_reqs.push_back(tx),
//                         Err(e) => {
//                             // Don't care if the receiver has gone away
//                             let _ = tx.send(Err(e));
//                         }
//                     }
//                 });
//             })
//                 .await;
//
//             // Wait for the result concurrently, so many requests can
//             // be pipelined at the same time.
//             rx.await.expect("Sender should not be dropped");
//         }
//             .interop_actor(self);
//     }
// }

impl<M> Handler<ProtocolMessage<Msg<M>>> for PipelineAdapter<M>
    where
        M: Send + 'static,
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolMessage<Msg<M>>, _ctx: &mut Context<Self>)  {
        let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
            .expect("Sink to be present");
        sink.send(msg.message);
    }
}

// Process responses
impl<M> StreamHandler<Result<Msg<M>>> for PipelineAdapter<M>
    where
        M: Send + 'static,
{
    fn handle(&mut self, msg: Result<Msg<M>>, _ctx: &mut Context<Self>) {
        match msg {
            Ok(m) => {self.processor.do_send(ProtocolMessage{ room: self.room.clone(), message: m});}
            Err(e) => {
                // Ignore
            }
        }
    }
}