use std::collections::HashMap;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::time::Duration;

use actix::prelude::*;
use actix_interop::{critical_section, FutureInterop, with_ctx};
use anyhow::{Context as AnyhowContext, Result};
use curv::arithmetic::Converter;
use curv::BigInt;
use curv::elliptic::curves::Secp256k1;
use futures::Sink;
use futures_util::SinkExt;
use futures_util::TryStreamExt;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::party_i::SignatureRecid;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::keygen::{Error as KeygenError, Keygen, LocalKey, ProtocolMessage as KeygenProtocolMessage};
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2020::state_machine::sign::{CompletedOfflineStage, Error as OfflineStageError, Error, OfflineProtocolMessage, OfflineStage, PartialSignature};
use round_based::{Msg, StateMachine};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;

use crate::core::{MpcGroup, PublicKeyGroup, ResponsePayload};

use super::messages::*;
use super::MpcPlayer;
use super::Signer;

#[derive(Debug, Error)]
enum GroupError {
    #[error("Public key doesn't belong to the group.")]
    WrongPublicKey,
    #[error("Some of the public keys don't belong to the group.")]
    WrongPublicKeys,
}

#[derive(Debug, Error)]
enum DataError {
    #[error("Couldn't find the local share for {0}.")]
    LocalShareNotFound(String),
    #[error("Couldn't find the group of id {0}.")]
    GroupNotFound(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredLocalShare {
    public_keys: Vec<String>,
    own_public_key: String,
    share: LocalKey<Secp256k1>,
}

pub struct Coordinator {
    tx_res: UnboundedSender<ResponsePayload>,
    db: sled::Db,
    keygen_runners: HashMap<String, Addr<MpcPlayer<KeygenRequest, Keygen, <Keygen as StateMachine>::MessageBody, <Keygen as StateMachine>::Err, <Keygen as StateMachine>::Output>>>,
    offline_state_runners: HashMap<String, Addr<MpcPlayer<EnrichedSignRequest, OfflineStage, <OfflineStage as StateMachine>::MessageBody, <OfflineStage as StateMachine>::Err, <OfflineStage as StateMachine>::Output>>>,
    signers: HashMap<String, Addr<Signer<EnrichedSignRequest>>>,
    sink: Option<Pin<Box<dyn Sink<Envelope, Error=anyhow::Error>>>>,
}

impl Coordinator {
    pub fn new<Si, St>(tx_res: UnboundedSender<ResponsePayload>, db: sled::Db, stream: St, sink: Si) -> Addr<Self>
        where
            St: Stream<Item=Result<SignedEnvelope<String>>> + 'static,
            Si: Sink<Envelope, Error=anyhow::Error> + 'static,
    {
        let stream = stream.and_then(|msg| async move {
            Ok(IncomingEnvelope {
                room: msg.room,
                message: msg.message,
                sender_public_key: msg.sender_public_key,
            })
        });
        let sink: Box<dyn Sink<Envelope, Error=anyhow::Error>> = Box::new(sink);

        Self::create(|ctx| {
            ctx.add_stream(stream);
            Self {
                tx_res,
                db,
                keygen_runners: HashMap::new(),
                offline_state_runners: HashMap::new(),
                signers: HashMap::new(),
                sink: Some(sink.into()),
            }
        })
    }

    fn valid_sender(&mut self, msg: IncomingEnvelope) -> Result<()> {
        let room = msg.room;
        let group = self.retrieve_group(room.clone()).context("Retrieve group.")?;
        group.get_index(&msg.sender_public_key).map_or(Err(GroupError::WrongPublicKey), |_| Ok(())).context("Validate sender.")?;
        Ok(())
    }

    fn handle_incoming_keygen(&mut self, room: &String, message: &String, _: &mut Context<Self>) -> Result<()> {
        let addr = self.keygen_runners.get(room).context("Can't found mpc player.")?;
        let msg = serde_json::from_str::<Msg<KeygenProtocolMessage>>(message).context("deserialize message")?;
        let _ = addr.do_send(IncomingMessage {
            room: room.clone(),
            message: msg,
        });
        Ok(())
    }

    fn handle_incoming_offline(&mut self, room: &String, message: &String, _: &mut Context<Self>) -> Result<()> {
        let addr = self.offline_state_runners.get(room).context("Not found mpc player.")?;
        let msg = serde_json::from_str::<Msg<OfflineProtocolMessage>>(message).context("deserialize message")?;
        let _ = addr.do_send(IncomingMessage {
            room: room.clone(),
            message: msg,
        });

        Ok(())
    }

    fn handle_incoming_sign(&mut self, room: &String, message: &String, _: &mut Context<Self>) -> Result<()> {
        let addr = self.signers.get(room).context("Not found signer.")?;
        let msg = serde_json::from_str::<Msg<PartialSignature>>(message).context("deserialize message")?;
        addr.do_send(IncomingMessage {
            room: room.clone(),
            message: msg,
        });
        Ok(())
    }

    fn retry(&mut self, envelope: RetryEnvelope, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::from_secs(1), move |_, _ctx| {
            _ctx.notify(envelope);
        });
    }

    fn handle_incoming_unsafe(&mut self, room: String, message: String, ctx: &mut Context<Self>) {
        let h1 = self.handle_incoming_offline(&room, &message, ctx);
        let h2 = self.handle_incoming_sign(&room, &message, ctx);
        let h3 = self.handle_incoming_keygen(&room, &message, ctx);
        let handled_by = match (h1.is_ok(), h2.is_ok(), h3.is_ok()) {
            (true, _, _) => { "offline" }
            (_, true, _) => { "sign" }
            (_, _, true) => { "keygen" }
            _ => { "none" }
        };
        log::debug!("Coordinator routing message (handled by {}): {} {}", handled_by, room, message);
        if h1.or(h2).or(h3).is_err() {
            self.retry(RetryEnvelope {
                room,
                message,
            }, ctx);
        }
    }

    fn handle_incoming(&mut self, msg: IncomingEnvelope, ctx: &mut Context<Self>) {
        match self.valid_sender(msg.clone()) {
            Ok(()) => {
                self.handle_incoming_unsafe(msg.room, msg.message, ctx);
            }
            Err(_) => {
                log::debug!("Not valid sender.")
                // Do nothing
            }
        }
    }

    fn send_one(envelope: OutgoingEnvelope) -> impl Future<Output=()> {
        async move {
            critical_section::<Self, _>(async {
                let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
                    .expect("Sink to be present");

                // Send the request
                let _ = sink.send(Envelope {
                    room: envelope.room,
                    message: envelope.message,
                }).await;

                // Put the sink back, and if the send was successful,
                // record the in-flight request.
                with_ctx(|actor: &mut Self, _| {
                    actor.sink = Some(sink);
                });
            })
                .await;
        }
    }
    fn save_group(&mut self, group: PublicKeyGroup) -> Result<()> {
        let value = serde_json::to_vec_pretty(&group).context("serialize local share")?;
        let key = group.get_group_id();
        let ov: Option<&[u8]> = None;
        let nv: Option<&[u8]> = Some(value.as_slice()); // TODO: Encrypt payload
        let _ = self.db.compare_and_swap(
            key.as_bytes(),      // key
            ov, // old value, None for not present
            nv, // new value, None for delete
        ).context("Save to db.")?;
        Ok(())
    }
    fn retrieve_group(&mut self, group_id: String) -> Result<PublicKeyGroup> {
        let group = self.db.get(group_id.as_bytes())?
            .ok_or_else(|| DataError::GroupNotFound(group_id.clone()))
            .context("Retrieving group.")?;
        let group = serde_json::from_slice::<PublicKeyGroup>(group.as_ref())
            .context("Decode group.")?;
        Ok(group)
    }
    fn save_local_share(&mut self, local_share: StoredLocalShare) -> Result<()> {
        let out = serde_json::to_vec_pretty(&local_share).context("serialize local share")?;
        let sum_pk_bytes = local_share.share.public_key().to_bytes(true);
        let sum_pk = hex::encode(sum_pk_bytes.deref());
        let ov: Option<&[u8]> = None;
        let nv: Option<&[u8]> = Some(out.as_slice()); // TODO: Encrypt payload
        let _ = self.db.compare_and_swap(
            sum_pk.as_bytes(),      // key
            ov, // old value, None for not present
            nv, // new value, None for delete
        ).context("Save to db.")?;
        Ok(())
    }
    fn retrieve_local_share(&mut self, public_key: String) -> Result<StoredLocalShare> {
        let local_share = self.db.get(public_key.as_bytes())?
            .ok_or_else(|| DataError::LocalShareNotFound(public_key.clone()))
            .context("Retrieving local share.")?;
        let local_share = serde_json::from_slice::<StoredLocalShare>(local_share.as_ref())
            .context("Decode local share.")?;
        Ok(local_share)
    }
}

impl Actor for Coordinator {
    type Context = Context<Self>;
}

impl Handler<KeygenRequest> for Coordinator {
    type Result = Result<()>;

    fn handle(&mut self, req: KeygenRequest, ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Received request {:?}", req);
        let KeygenRequest { request_id, public_keys, t, own_public_key } = req.clone();
        let _ = self.tx_res.send(ResponsePayload {
            request_id,
            result: None,
            request_type: "KEYGEN".to_owned(),
            request_status: "PROCESSING".to_owned(),
        });
        let group = PublicKeyGroup::new(public_keys, t, own_public_key);
        let group_id = group.get_group_id();
        let room = req.request_id.clone() + "@" + group_id.clone().as_str();

        let state = Keygen::new(group.get_i(), group.get_t(), group.get_n()).context("Create state machine")?;
        let player = MpcPlayer::new(
            req.clone(),
            group_id.clone(),
            group.get_i(),
            state,
            ctx.address().recipient(),
            ctx.address().recipient(),
            ctx.address().recipient(),
        ).start();
        let _ = self.save_group(group.clone());
        self.keygen_runners.insert(room, player);
        Ok(())
    }
}

impl Handler<SignRequest> for Coordinator {
    type Result = Result<()>;

    fn handle(&mut self, req: SignRequest, ctx: &mut Context<Self>) -> Self::Result {
        log::info!("Received request {:?}", req);
        let local_share = self.retrieve_local_share(req.public_key.clone()).context("Retrieve local share.")?;
        let group = PublicKeyGroup::new(
            local_share.public_keys,
            local_share.share.t,
            req.own_public_key.clone(),
        );
        // let public_keys = local_share.public_keys;
        // public_keys.iter().position(|&r| *r == k)
        let indices: Vec<Option<usize>> = req.participant_public_keys.clone().into_iter().map(
            |k| group.get_index(&k)
        ).collect();
        let (indices, errors): (Vec<Option<usize>>, Vec<_>) = indices.into_iter().partition(Option::is_some);

        let s_l: Vec<u16> = if errors.len() == 0 {
            Ok(indices.into_iter().map(|o| o.expect("Index") as u16).collect())
        } else {
            Err(GroupError::WrongPublicKeys)
        }.context("Find index of participants")?;

        let _ = self.tx_res.send(ResponsePayload {
            request_id: req.request_id.clone(),
            result: None,
            request_type: "SIGN".to_owned(),
            request_status: "PROCESSING".to_owned(),
        });
        let index_in_group = group.get_i();
        let index_in_s_l = s_l.iter().position(|&x| x == index_in_group).unwrap() as u16 + 1;
        let group_id = group.get_group_id();
        let room = req.request_id.clone() + "@" + group_id.clone().as_str();

        let req = EnrichedSignRequest {
            inner: req,
            room: room.clone(),
            i: index_in_s_l,
            s_l: s_l.clone(),
        };
        let s = serde_json::to_string(&local_share.share);
        log::debug!("Local share is {:}", s.unwrap());
        let state = OfflineStage::new(index_in_s_l, s_l, local_share.share);
        log::debug!("Party index is {:?}", index_in_s_l);
        log::debug!("OfflineStage is {:?}", state);
        let state = state.context("Create state machine")?;
        let player = MpcPlayer::new(
            req.clone(),
            room.clone(),
            index_in_s_l,
            state,
            ctx.address().recipient(),
            ctx.address().recipient(),
            ctx.address().recipient(),
        ).start();
        self.offline_state_runners.insert(room, player);
        Ok(())
    }
}

impl Handler<ProtocolError<KeygenError, Msg<KeygenProtocolMessage>>> for Coordinator {
    type Result = ();

    fn handle(&mut self, msg: ProtocolError<KeygenError, Msg<KeygenProtocolMessage>>, ctx: &mut Context<Self>) {
        log::info!("Error {:?}", msg.error);
        match msg.error {
            KeygenError::ReceivedOutOfOrderMessage { current_round: _, msg_round: _ } => {
                let message = serde_json::to_string(&msg.message).unwrap();
                self.retry(RetryEnvelope {
                    room: msg.room,
                    message,
                }, ctx);
            }
            _ => {}
        }
    }
}

impl Handler<ProtocolError<OfflineStageError, Msg<OfflineProtocolMessage>>> for Coordinator {
    type Result = ();

    fn handle(&mut self, msg: ProtocolError<OfflineStageError, Msg<OfflineProtocolMessage>>, ctx: &mut Context<Self>) {
        log::info!("Error {:?}", msg.error);
        match msg.error {
            OfflineStageError::ReceivedOutOfOrderMessage { current_round: _, msg_round: _ } => {
                let message = serde_json::to_string(&msg.message).unwrap();
                self.retry(RetryEnvelope {
                    room: msg.room,
                    message,
                }, ctx);
            }
            _ => {}
        }
    }
}

impl Handler<OutgoingEnvelope> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: OutgoingEnvelope, ctx: &mut Context<Self>) {
        ctx.spawn(Self::send_one(msg).interop_actor(self));
    }
}

impl Handler<ProtocolOutput<KeygenRequest, LocalKey<Secp256k1>>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutput<KeygenRequest, LocalKey<Secp256k1>>, _: &mut Context<Self>) {
        let sum_pk_bytes = msg.output.public_key().to_bytes(true);
        let sum_pk = hex::encode(sum_pk_bytes.deref());
        log::info!("Public key is {:?}", sum_pk);
        let share = msg.output.clone();
        let request_id = msg.input.request_id.clone();
        let saved = self.save_local_share(StoredLocalShare {
            public_keys: msg.input.public_keys,
            own_public_key: msg.input.own_public_key,
            share: msg.output,
        });

        let _ = self.tx_res.send(ResponsePayload {
            request_id,
            result: Some(sum_pk),
            request_type: "KEYGEN".to_owned(),
            request_status: "DONE".to_owned(),
        });

        match saved {
            Ok(()) => {
                log::debug!("Saved local share: {:?}", share);
            }
            Err(e) => { log::error!("Failed to save local share: {}", e); }
        }
    }
}

impl Handler<ProtocolOutput<EnrichedSignRequest, CompletedOfflineStage>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutput<EnrichedSignRequest, CompletedOfflineStage>, ctx: &mut Context<Self>) {
        log::info!("result {:?}", msg.output.public_key());
        let reqId = msg.input.inner.request_id.clone();
        let do_it = || -> Result<()>{
            let message = BigInt::from_hex(msg.input.inner.message.as_ref())?;
            let completed_offline_stage = msg.output;
            let input = msg.input.clone();
            let signer = Signer::new(
                input.clone(),
                input.room,
                input.i,
                input.s_l.len() - 1,
                message,
                completed_offline_stage,
                ctx.address().recipient(),
                ctx.address().recipient(),
            ).start();
            self.signers.insert(msg.input.room.to_owned(), signer);
            let request_id = msg.input.inner.request_id.clone();
            let _ = self.tx_res.send(ResponsePayload {
                request_id,
                result: None,
                request_type: "SIGN".to_owned(),
                request_status: "OFFLINE_STAGE_DONE".to_owned(),
            });

            Ok(())
        };
        match do_it() {
            Ok(_) => {}
            Err(_) => {
                let _ = self.tx_res.send(ResponsePayload {
                    request_id: reqId,
                    result: None,
                    request_type: "SIGN".to_owned(),
                    request_status: "ERROR".to_owned(),
                });
            }
        }
    }
}

impl Handler<ProtocolOutput<EnrichedSignRequest, SignatureRecid>> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutput<EnrichedSignRequest, SignatureRecid>, _: &mut Context<Self>) {
        let r = hex::encode(msg.output.r.to_bytes().as_ref());
        let s = hex::encode(msg.output.s.to_bytes().as_ref());
        let v = hex::encode(msg.output.recid.to_be_bytes().as_ref());
        let mut sig: String = "0x".to_owned();

        sig.push_str(r.as_str());
        sig.push_str(s.as_str());
        sig.push_str(v.as_str());

        log::info!("Sign request done {:?} sig: {:?}", msg.input, sig);
        let request_id = msg.input.inner.request_id.clone();
        let _ = self.tx_res.send(ResponsePayload {
            request_id,
            result: Some(sig),
            request_type: "SIGN".to_owned(),
            request_status: "DONE".to_owned(),
        });
    }
}

impl StreamHandler<Result<IncomingEnvelope>> for Coordinator
{
    fn handle(&mut self, msg: Result<IncomingEnvelope>, ctx: &mut Context<Self>) {
        match msg.context("Invalid IncomingEnvlope") {
            Ok(msg) => { self.handle_incoming(msg, ctx); }
            Err(_) => {}
        }
    }
}

impl Handler<RetryEnvelope> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: RetryEnvelope, ctx: &mut Context<Self>) {
        self.handle_incoming_unsafe(msg.room, msg.message, ctx);
    }
}
