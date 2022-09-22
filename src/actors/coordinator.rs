use std::collections::HashMap;
use std::ops::Deref;
use std::pin::Pin;
use std::time::Duration;

use actix::prelude::*;
use actix_interop::{critical_section, FutureInterop, with_ctx};
use anyhow::{Context as AnyhowContext, Result};
use curv::arithmetic::Converter;
use curv::BigInt;
use futures::Sink;
use futures_util::SinkExt;
use futures_util::TryStreamExt;
use hex::ToHex;
use kv_log_macro as log;
use secp256k1::{PublicKey, SecretKey};
use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;

use crate::{KeygenPayload, prom, RequestType, SignPayload};
use crate::actors::aliases::*;
use crate::actors::msg_utils::describe_message;
use crate::actors::types::{EnrichedSignRequest, SignTask};
use crate::api::{RequestStatus, ResponsePayload};
use crate::core::{CoreMessage, MpcGroup, PublicKeyGroup, StoredLocalShare};
use crate::crypto::{Decrypter, Encrypter, SimpleDecrypter, SimpleEncrypter};
use crate::storage::{GroupStore, LocalShareStore, SledDbGroupStore, SledDbLocalShareStore};
use crate::utils;
use crate::utils::{extract_group_id, make_room_id};
use crate::wire::WireMessage;

use super::messages::*;
use super::MpcPlayer;
use super::Signer;

#[derive(Debug, Error)]
enum CoordinatorError {
    #[error("Public key doesn't belong to the group.")]
    WrongPublicKey,
    #[error("Some of the public keys don't belong to the group.")]
    WrongPublicKeys,
    #[error("The required number of participants is not met.")]
    WrongNumberOfParticipants,
    #[error("The message is not for me.")]
    NotForMe,
}

pub struct Coordinator {
    pk_str: String,
    tx_res: UnboundedSender<ResponsePayload>,
    group_store: Box<dyn GroupStore>,
    local_share_store: Box<dyn LocalShareStore>,
    encrypter: Box<dyn Encrypter>,
    decrypter: Box<dyn Decrypter>,
    keygen_runners: HashMap<String, Addr<MpcPlayerKG>>,
    offline_state_runners: HashMap<String, Addr<MpcPlayerO>>,
    signers: HashMap<String, Addr<Signer<EnrichedSignRequest>>>,
    sink: Option<Pin<Box<dyn Sink<CoreMessage, Error=anyhow::Error>>>>,
}

impl Coordinator {
    pub fn new<Si, St>(sk: SecretKey, tx_res: UnboundedSender<ResponsePayload>, db: sled::Db, stream: St, sink: Si) -> Addr<Self>
        where
            St: Stream<Item=Result<WireMessage>> + 'static,
            Si: Sink<CoreMessage, Error=anyhow::Error> + 'static,
    {
        let stream = stream.and_then(|msg| async move {
            Ok(CoordinatorMessage::Incoming(msg))
        });
        let sink: Box<dyn Sink<CoreMessage, Error=anyhow::Error>> = Box::new(sink);
        let pk = PublicKey::from_secret_key(&sk).serialize_compressed().encode_hex();
        let group_store = Box::new(SledDbGroupStore::new(db.clone()));
        let local_share_store = Box::new(SledDbLocalShareStore::new(db.clone()));
        let encrypter = Box::new(SimpleEncrypter::new(db));
        let decrypter = Box::new(SimpleDecrypter::new(sk));

        Self::create(|ctx| {
            ctx.add_stream(stream);
            Self {
                pk_str: pk,
                tx_res,
                group_store,
                local_share_store,
                encrypter,
                decrypter,
                keygen_runners: HashMap::new(),
                offline_state_runners: HashMap::new(),
                signers: HashMap::new(),
                sink: Some(sink.into()),
            }
        })
    }

    fn handle_keygen_request(&mut self, req: KeygenPayload, ctx: &mut Context<Self>) -> Result<()> {
        log::info!("Received request", {request: serde_json::to_string(&req).unwrap()});
        prom::COUNTER_REQUESTS_KEYGEN_RECEIVED.inc();
        let KeygenPayload { request_id, public_keys, t } = req.clone();
        let _ = self.tx_res.send(ResponsePayload {
            request_id,
            result: None,
            request_type: RequestType::KEYGEN,
            request_status: RequestStatus::PROCESSING,
        });
        let group = PublicKeyGroup::new(public_keys, t, self.pk_str.clone());
        let group_id = group.get_group_id();
        let room = make_room_id(req.request_id.as_str(), group_id.as_str());

        let state = StateMachineKG::new(group.get_i(), group.get_t(), group.get_n()).context("Create state machine")?;
        let player = MpcPlayer::new(
            req,
            room.clone(),
            group.get_i(),
            state,
            ctx.address().recipient(),
            ctx.address().recipient(),
            ctx.address().recipient(),
        ).start();
        let _ = self.group_store.save_group(group);
        self.keygen_runners.insert(room, player);
        Ok(())
    }

    fn handle_sign_request(&mut self, req: SignPayload, ctx: &mut Context<Self>) -> Result<()> {
        prom::COUNTER_REQUESTS_SIGN_RECEIVED.inc();
        log::info!("Received request", { request: serde_json::to_string( &req).unwrap() });
        let local_share = self.local_share_store.retrieve_local_share(req.public_key.clone()).context("Retrieve local share.")?;
        let group = PublicKeyGroup::new(
            local_share.public_keys,
            local_share.share.t,
            self.pk_str.clone(),
        );
        // let public_keys = local_share.public_keys;
        // public_keys.iter().position(|&r| *r == k)
        let subgroup = PublicKeyGroup::new(
            req.participant_public_keys.clone(),
            local_share.share.t,
            self.pk_str.clone(),
        );
        let _ = self.group_store.save_group(subgroup.clone());
        let (indices, errors): (Vec<Option<usize>>, Vec<_>) = req.participant_public_keys.clone().into_iter().map(
            |k| group.get_index(&k)
        ).partition(Option::is_some);

        let s_l: Vec<u16> = if indices.len() != (local_share.share.t + 1) as usize {
            Err(CoordinatorError::WrongNumberOfParticipants)
        } else if !errors.is_empty() {
            Err(CoordinatorError::WrongPublicKeys)
        } else {
            Ok(indices.into_iter().map(|o| o.expect("Index") as u16).collect())
        }.context("Find index of participants")?;

        let _ = self.tx_res.send(ResponsePayload {
            request_id: req.request_id.clone(),
            result: None,
            request_type: RequestType::SIGN,
            request_status: RequestStatus::PROCESSING,
        });
        let _index_in_group = group.get_i();
        let index_in_s_l = subgroup.get_i();
        let room = req.request_id.clone() + "@" + subgroup.get_group_id().as_str();

        let req = EnrichedSignRequest {
            inner: req,
            room: room.clone(),
            i: index_in_s_l,
            s_l: s_l.clone(),
        };
        let _s = serde_json::to_string(&local_share.share);
        // log::debug!("Local share is {:}", s.unwrap());
        let state = StateMachineO::new(index_in_s_l, s_l, local_share.share);
        log::debug!("Party index is {:?}", index_in_s_l);
        // log::debug!("OfflineStage is {:?}", state);
        let state = state.context("Create state machine")?;
        let player = MpcPlayer::new(
            req,
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

    fn handle_coordinator_message(&mut self, msg: CoordinatorMessage, ctx: &mut Context<Self>) {
        match msg {
            CoordinatorMessage::Incoming(msg) => {
                self.handle_incoming(msg, ctx);
            }
            CoordinatorMessage::Retry(msg) => {
                if msg.check_passed {
                    self.handle_incoming_unsafe(msg.message, msg.initial_timestamp, msg.attempts, ctx);
                } else {
                    self.handle_retry(msg, ctx);
                }
            }
            CoordinatorMessage::Outgoing(mut msg) => {
                match self.encrypter.maybe_encrypt(&mut msg) {
                    Ok(_) => {
                        ctx.spawn(Self::send_one(msg).interop_actor(self));
                    }
                    Err(e) => {
                        log::error!("Failed encrypt message: {}", e);
                    }
                }
            }

            CoordinatorMessage::KeygenRequest(req) => {
                let _ = self.handle_keygen_request(req, ctx);
            }
            CoordinatorMessage::SignRequest(req) => {
                let _ = self.handle_sign_request(req, ctx);
            }
        }
    }

    fn valid_sender_and_receiver(&mut self, msg: WireMessage) -> Result<()> {
        let group_id = extract_group_id(&msg.room).context("extract group_id")?;
        let group = self.group_store.retrieve_group(group_id.to_string()).context("Retrieve group.")?;
        group.get_index(&msg.sender_public_key).map_or(Err(CoordinatorError::WrongPublicKey), |_| Ok(())).context("Validate sender.")?;
        let m = serde_json::from_str::<GenericProtocolMessage>(&msg.payload).context("parse GenericProtocolMessage")?;
        match m.receiver {
            Some(receiver) => {
                if receiver != group.get_i() {
                    return Err(CoordinatorError::NotForMe.into());
                }
                Ok(())
            }
            None => {
                Ok(())
            }
        }
    }

    fn handle_incoming_keygen(&mut self, message: &WireMessage, _: &mut Context<Self>) -> Result<()> {
        let addr = self.keygen_runners.get(&message.room).context("Can't found mpc player.")?;
        let msg = serde_json::from_str::<MsgKG>(&message.payload).context("deserialize message")?;
        addr.do_send(IncomingMessage {
            room: message.room.to_string(),
            wire_message: message.clone(),
            message: msg,
        });
        prom::COUNTER_MESSAGES_KEYGEN_HANDLED.inc();
        Ok(())
    }

    fn handle_incoming_offline(&mut self, message: &WireMessage, _: &mut Context<Self>) -> Result<()> {
        let addr = self.offline_state_runners.get(&message.room).context("Not found mpc player.")?;
        let msg = serde_json::from_str::<MsgO>(&message.payload).context("deserialize message")?;
        addr.do_send(IncomingMessage {
            room: message.room.to_string(),
            wire_message: message.clone(),
            message: msg,
        });
        prom::COUNTER_MESSAGES_OFFLINE_HANDLED.inc();
        Ok(())
    }

    fn handle_incoming_sign(&mut self, message: &WireMessage, _: &mut Context<Self>) -> Result<()> {
        let addr = self.signers.get(&message.room).context("Not found signer.")?;
        let msg = serde_json::from_str::<MsgS>(&message.payload).context("deserialize message")?;
        addr.do_send(IncomingMessage {
            room: message.room.clone(),
            wire_message: message.clone(),
            message: msg,
        });
        prom::COUNTER_MESSAGES_SIGN_HANDLED.inc();
        Ok(())
    }

    fn retry(&mut self, envelope: RetryMessage, ctx: &mut Context<Self>) {
        if envelope.attempts > 5 {
            prom::COUNTER_MESSAGES_DROPPED.inc();
            log::error!("reached max retries", {room: format!("\"{}\"", &envelope.message.room), protocolMessage: describe_message(&envelope.message.payload)});
        } else {
            ctx.run_later(Duration::from_secs(2_u32.pow(envelope.attempts as u32) as u64), move |_, _ctx| {
                _ctx.notify(CoordinatorMessage::Retry(envelope));
            });
        }
    }

    fn handle_incoming_unsafe(&mut self, message: WireMessage, init_ts: u128, attempts: u16, ctx: &mut Context<Self>) {
        let h1 = self.handle_incoming_offline(&message, ctx);
        let h2 = self.handle_incoming_sign(&message, ctx);
        let h3 = self.handle_incoming_keygen(&message, ctx);
        let handled_by = match (h1.is_ok(), h2.is_ok(), h3.is_ok()) {
            (true, _, _) => { "offline" }
            (_, true, _) => { "sign" }
            (_, _, true) => { "keygen" }
            _ => { "none" }
        };
        if attempts == 0 {
            log::debug!("Coordinator routing message", {
                handled_by: format!("\"{}\"", &handled_by),
                room: format!("\"{}\"", &message.room),
                protocolMessage: describe_message(&message.payload)
            });
        }

        if h1.or(h2).or(h3).is_err() {
            self.retry(RetryMessage {
                message,
                initial_timestamp: init_ts,
                attempts: attempts + 1,
                check_passed: true,
            }, ctx);
        }
    }

    fn handle_incoming(&mut self, msg: WireMessage, ctx: &mut Context<Self>) {
        prom::COUNTER_MESSAGES_TOTAL_RECEIVED.inc();
        match self.valid_sender_and_receiver(msg.clone()) {
            Ok(()) => {
                let mut transformed = msg.clone();
                match self.decrypter.maybe_decrypt(&mut transformed) {
                    Ok(_) => {
                        self.handle_incoming_unsafe(transformed, utils::current_timestamp(), 0, ctx);
                    }
                    Err(_e) => {
                        log::error!("Failed to decrypt message", {protocolMessage: serde_json::to_string(&msg).unwrap()});
                    }
                }
            }
            Err(_e) => {
                self.retry(RetryMessage {
                    message: msg,
                    initial_timestamp: utils::current_timestamp(),
                    attempts: 1,
                    check_passed: false,
                }, ctx);
            }
        }
    }
    //maybe_decrypt(&mut self, msg: &mut IncomingEnvelope)
    fn precheck_message(&mut self, msg: &mut WireMessage) -> Result<()> {
        self.valid_sender_and_receiver(msg.clone())?;
        self.decrypter.maybe_decrypt(msg)?;
        Ok(())
    }

    fn handle_retry(&mut self, msg: RetryMessage, ctx: &mut Context<Self>) {
        let mut adapted_envelope = msg.message.clone();

        match self.precheck_message(&mut adapted_envelope) {
            Ok(()) => {
                self.handle_incoming_unsafe(adapted_envelope, msg.initial_timestamp, msg.attempts, ctx);
            }
            Err(_e) => {
                self.retry(RetryMessage {
                    message: msg.message,
                    initial_timestamp: msg.initial_timestamp,
                    attempts: msg.attempts + 1,
                    check_passed: false,
                }, ctx);
            }
        }
    }

    async fn send_one(envelope: CoreMessage) {
        critical_section::<Self, _>(async {
            let mut sink = with_ctx(|actor: &mut Self, _| actor.sink.take())
                .expect("Sink to be present");
            log::debug!("Sending message", { room: format!("\"{}\"", &envelope.room), protocolMessage: describe_message(&envelope.message)});
            // Send the request
            let _ = sink.send(envelope).await;

            // Put the sink back, and if the send was successful,
            // record the in-flight request.
            with_ctx(|actor: &mut Self, _| {
                actor.sink = Some(sink);
            });
        })
            .await;
    }
}

impl Actor for Coordinator {
    type Context = Context<Self>;
}

impl Handler<ProtocolErrorKG> for Coordinator {
    type Result = ();

    fn handle(&mut self, msg: ProtocolErrorKG, ctx: &mut Context<Self>) {
        log::info!("Error {:?}", msg.error);
        if let KeygenError::ReceivedOutOfOrderMessage { current_round: _, msg_round: _ } = msg.error {
            self.retry(RetryMessage {
                message: msg.message.wire_message,
                initial_timestamp: utils::current_timestamp(),
                attempts: 1,
                check_passed: true,
            }, ctx);
        }
    }
}

impl Handler<ProtocolErrorO> for Coordinator {
    type Result = ();

    fn handle(&mut self, msg: ProtocolErrorO, ctx: &mut Context<Self>) {
        log::info!("Error {:?}", msg.error);
        if let OfflineStageError::ReceivedOutOfOrderMessage { current_round: _, msg_round: _ } = msg.error {
            self.retry(RetryMessage {
                message: msg.message.wire_message,
                initial_timestamp: utils::current_timestamp(),
                attempts: 1,
                check_passed: true,
            }, ctx);
        }
    }
}

impl Handler<ProtocolOutputKG> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutputKG, _: &mut Context<Self>) {
        prom::COUNTER_REQUESTS_KEYGEN_DONE.inc();
        let group = PublicKeyGroup::new(
            msg.input.public_keys.clone(),
            msg.input.t,
            self.pk_str.clone(),
        );

        let group_id = group.get_group_id();
        let room = make_room_id(msg.input.request_id.as_str(), group_id.as_str());
        self.offline_state_runners.remove(&*room);

        let sum_pk_bytes = msg.output.public_key().to_bytes(true);
        let sum_pk = hex::encode(sum_pk_bytes.deref());
        log::info!("Public key is {:?}", sum_pk);
        let _share = msg.output.clone();
        let request_id = msg.input.request_id.clone();
        let saved = self.local_share_store.save_local_share(StoredLocalShare {
            public_keys: msg.input.public_keys,
            own_public_key: self.pk_str.clone(),
            share: msg.output,
        });

        let _ = self.tx_res.send(ResponsePayload {
            request_id,
            result: Some(sum_pk),
            request_type: RequestType::KEYGEN,
            request_status: RequestStatus::DONE,
        });

        match saved {
            Ok(()) => {
                // log::debug!("Saved local share: {:?}", share);
            }
            Err(_e) => {
                //log::error!("Failed to save local share: {}", e);
            }
        }
    }
}

impl Handler<ProtocolOutputO> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutputO, ctx: &mut Context<Self>) {
        prom::COUNTER_REQUESTS_SIGN_DONE.inc();
        log::info!("output public key", { room: format!("{:?}", &msg.input.room), publicKey: format!("\"{}\"", hex::encode(msg.output.public_key().to_bytes(true).as_ref()))} );

        self.offline_state_runners.remove(msg.input.room.as_str());
        let do_it = || -> Result<()>{
            let message = BigInt::from_hex(msg.input.inner.message.as_ref())?;
            let completed_offline_stage = msg.output;
            let input = msg.input.clone();
            let task = SignTask {
                room: input.room.clone(),
                index: input.i,
                t: input.s_l.len() - 1,
                message,
                completed_offline_stage,
            };
            let signer = Signer::new(
                input,
                task,
                ctx.address().recipient(),
                ctx.address().recipient(),
            ).start();
            self.signers.insert(msg.input.room.to_owned(), signer);
            let request_id = msg.input.inner.request_id.clone();
            let _ = self.tx_res.send(ResponsePayload {
                request_id,
                result: None,
                request_type: RequestType::SIGN,
                request_status: RequestStatus::OFFLINE_STAGE_DONE,
            });

            Ok(())
        };
        match do_it() {
            Ok(_) => {}
            Err(_) => {
                let _ = self.tx_res.send(ResponsePayload {
                    request_id: msg.input.inner.request_id.clone(),
                    result: None,
                    request_type: RequestType::SIGN,
                    request_status: RequestStatus::ERROR,
                });
            }
        }
    }
}

impl Handler<ProtocolOutputS> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: ProtocolOutputS, _: &mut Context<Self>) {
        let r = hex::encode(msg.output.r.to_bytes().as_ref());
        let s = hex::encode(msg.output.s.to_bytes().as_ref());
        let v = hex::encode(msg.output.recid.to_be_bytes().as_ref());
        let mut sig: String = "0x".to_owned();

        sig.push_str(r.as_str());
        sig.push_str(s.as_str());
        sig.push_str(v.as_str());

        // log::info!("Sign request done {:?} sig: {:?}", msg.input, sig);
        let request_id = msg.input.inner.request_id;
        let _ = self.tx_res.send(ResponsePayload {
            request_id,
            result: Some(sig),
            request_type: RequestType::SIGN,
            request_status: RequestStatus::DONE,
        });
    }
}

impl StreamHandler<Result<CoordinatorMessage>> for Coordinator
{
    fn handle(&mut self, msg: Result<CoordinatorMessage>, ctx: &mut Context<Self>) {
        if let Ok(msg) = msg.context("invalid CoordinatorMessage") {
            self.handle_coordinator_message(msg, ctx);
        }
    }
}

impl Handler<CoordinatorMessage> for Coordinator
{
    type Result = ();

    fn handle(&mut self, msg: CoordinatorMessage, ctx: &mut Context<Self>) {
        self.handle_coordinator_message(msg, ctx);
    }
}
