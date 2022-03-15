use std::convert::TryInto;
use anyhow::{Context, Result};
use futures::{Sink, Stream, StreamExt, TryStreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use structopt::StructOpt;

use round_based::Msg;
use crate::messages::Envelope;

pub async fn join_computation(
    url: surf::Url
) -> Result<(
    impl Stream<Item=Result<Envelope>>,
    impl Sink<Envelope, Error=anyhow::Error>,
)>
{
    let client = SmClient::new(url).context("construct SmClient")?;

    // Construct channel of incoming messages
    let incoming = client
        .subscribe()
        .await
        .context("subscribe")?
        .and_then(|msg| async move {
            serde_json::from_str::<Envelope>(&msg).context("deserialize message")
        });

    // Construct channel of outgoing messages
    let outgoing = futures::sink::unfold(client, |client, message: Envelope| async move {
        let serialized = serde_json::to_string(&message)?;
        client
            .broadcast(&serialized)
            .await
            .context("broadcast message")?;
        Ok::<_, anyhow::Error>(client)
    });

    Ok((incoming, outgoing))
}

pub struct SmClient {
    http_client: surf::Client,
}

impl SmClient {
    pub fn new(url: surf::Url) -> Result<Self> {
        let config = surf::Config::new()
            .set_base_url(url)
            .set_timeout(None);
        Ok(Self {
            http_client: config.try_into()?,
        })
    }

    pub async fn broadcast(&self, message: &str) -> Result<()> {
        self.http_client
            .post("broadcast")
            .body(message)
            .await
            .map_err(|e| e.into_inner())?;
        Ok(())
    }

    pub async fn subscribe(&self) -> Result<impl Stream<Item=Result<String>>> {
        let response = self
            .http_client
            .get("subscribe")
            .await
            .map_err(|e| e.into_inner())?;
        let events = async_sse::decode(response);
        Ok(events.filter_map(|msg| async {
            match msg {
                Ok(async_sse::Event::Message(msg)) => Some(
                    String::from_utf8(msg.into_bytes())
                        .context("SSE message is not valid UTF-8 string"),
                ),
                Ok(_) => {
                    // ignore other types of events
                    None
                }
                Err(e) => Some(Err(e.into_inner())),
            }
        }))
    }
}
