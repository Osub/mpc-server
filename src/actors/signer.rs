use actix::prelude::*;
use anyhow::Result;
use kv_log_macro as log;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::party_i::SignatureRecid;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{PartialSignature, SignManual};
use round_based::Msg;
use crate::actors::messages::CoordinatorMessage;

use crate::actors::types::SignTask;
use crate::pb::types::CoreMessage;

use super::messages::{IncomingMessage, ProtocolOutput};

pub struct Signer<I: Send> {
    input: I,
    task: SignTask,
    partial_sigs: Vec<PartialSignature>,
    result_collector: Recipient<ProtocolOutput<I, SignatureRecid>>,
    message_broker: Recipient<CoordinatorMessage>,
}

impl<I> Signer<I>
    where
        I: Send + Clone + Unpin + 'static,
{
    pub(crate) fn new(
        input: I,
        task: SignTask,
        result_collector: Recipient<ProtocolOutput<I, SignatureRecid>>,
        message_broker: Recipient<CoordinatorMessage>,
    ) -> Self
    {
        let t = task.t;
        Self {
            input,
            task,
            partial_sigs: Vec::with_capacity(t),
            result_collector,
            message_broker,
        }
    }

    fn send_my_partial_signature(&mut self) -> Result<()> {
        let message = self.task.message.clone();
        let completed_offline_stage = self.task.completed_offline_stage.clone();
        let (_, partial_sig) = SignManual::new(
            message,
            completed_offline_stage,
        )?;
        let sig_msg = Msg {
            sender: self.task.index,
            receiver: None,
            body: partial_sig,
        };
        if let Ok(serialized) = serde_json::to_string(&sig_msg) {
            // log::debug!("Sending message {:?}", serde_json::to_string(&sig_msg));
            let _ = self.message_broker.do_send(CoordinatorMessage::Outgoing (CoreMessage{
                room: self.task.room.clone(),
                message: serialized,
            }));
        }

        Ok(())
    }

    fn finish_if_possible(&mut self, _: &mut Context<Self>) -> Result<()> {
        let message = self.task.message.clone();
        let completed_offline_stage = self.task.completed_offline_stage.clone();
        let (state, _) = SignManual::new(
            message,
            completed_offline_stage,
        )?;

        if self.partial_sigs.len() == self.task.t {
            if let Ok(signature) = state.complete(&self.partial_sigs) {
                let _ = self.result_collector.do_send(ProtocolOutput {
                    input: self.input.clone(),
                    output: signature,
                });
            }
        }
        Ok(())
    }
}

impl<I> Actor for Signer<I>
    where
        I: Send + Clone + Unpin + 'static,
{
    type Context = Context<Self>;
    fn started(&mut self, _: &mut Self::Context) {
        log::debug!("Started signer", { room: format!("\"{}\"", self.task.room) });
        let _ = self.send_my_partial_signature();
    }
}

impl<I> Handler<IncomingMessage<Msg<PartialSignature>>> for Signer<I>
    where
        I: Send + Clone + Unpin + 'static,
{
    type Result = ();

    fn handle(&mut self, msg: IncomingMessage<Msg<PartialSignature>>, ctx: &mut Context<Self>) {
        if msg.message.sender == self.task.index {
            return;
        }
        self.partial_sigs.push(msg.message.body);
        let _ = self.finish_if_possible(ctx);
    }
}
