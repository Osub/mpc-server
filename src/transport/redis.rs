use anyhow::{Context, Result};
use futures::{Sink, Stream, StreamExt};
use redis::{AsyncCommands, Client, Msg};
use redis::aio::Connection;
use secp256k1::{PublicKey, SecretKey};

use crate::actors::messages::SignedEnvelope;
use crate::core::CoreMessage;
use crate::transport::{parse_signed, sign_envelope, take_non_owned};

pub struct RedisClient {
    channel_name: String,
    client: Client,
    connection: Connection,
}

impl RedisClient {
    pub async fn new(url: String) -> Result<Self> {
        let channel_name = "avalido.mpc".to_string();
        let client = Client::open(url)?;
        let connection = client.get_async_connection().await?;
        Ok(Self {
            channel_name,
            client,
            connection,
        })
    }

    async fn broadcast(&mut self, message: &str) -> Result<()> {
        self.connection.publish(&self.channel_name, message).await?;
        Ok(())
    }

    async fn subscribe(&mut self) -> Result<impl Stream<Item=Result<String>>> {
        let conn = self.client.get_async_connection().await?;
        let mut pubsub = conn.into_pubsub();
        pubsub.subscribe(&self.channel_name).await?;
        let stream = pubsub.into_on_message().map(|msg: Msg| {
            match msg.get_payload::<String>() {
                Ok(msg) => {
                    Ok(msg)
                }
                Err(e) => {
                    Err(e.into())
                }
            }
        });
        Ok(stream)
    }
}

pub async fn join_computation_via_redis(
    redis_connection_string: String,
    key: SecretKey,
) -> Result<(
    impl Stream<Item=Result<SignedEnvelope<String>>>,
    impl Sink<CoreMessage, Error=anyhow::Error>,
)>
{
    let key = Box::new(key);
    let pub_key = PublicKey::from_secret_key(&key);
    let pub_key = Box::new(hex::encode(pub_key.serialize_compressed()));
    let mut client = RedisClient::new(redis_connection_string).await.context("construct RedisClient")?;
    let own_pub_key = pub_key.as_ref().clone();

    // Construct channel of incoming messages
    let incoming = client
        .subscribe()
        .await
        .context("subscribe")?
        .filter_map(parse_signed)
        .filter(take_non_owned(own_pub_key));
    // Construct channel of outgoing messages
    let outgoing = futures::sink::unfold((client, key, pub_key), |(mut client, key, pub_key), unsigned: CoreMessage| async move {
        let signed = sign_envelope(key.as_ref(), pub_key.as_ref(), unsigned).context("Failed to sign")?;
        let serialized = serde_json::to_string(&signed)?;
        client
            .broadcast(&serialized)
            .await
            .context("broadcast message")?;
        Ok::<_, anyhow::Error>((client, key, pub_key))
    });

    Ok((incoming, outgoing))
}
