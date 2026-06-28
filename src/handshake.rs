use std::io;

use futures_util::{SinkExt, TryStreamExt};
use log::{error, warn};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    cli::MarkStatus,
    framed_io::{Splittable, json_channel},
    server,
};

static VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize, Debug)]
struct Request {
    version: String,
    payload: RequestPayload,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum RequestPayload {
    ProcessingClient { groups: Vec<String> },
    Mark { hash: String, status: MarkStatus },
}

#[derive(Serialize, Deserialize, Debug)]
enum Answer {
    Ok,
    DifferentVersion(String),
    UnknownGroups(Vec<String>),
}

pub(crate) enum HandshakeOutcome {
    Success(ClientKind),
    Denied,
    ClosedConnection,
}

pub(crate) enum ClientKind {
    Processing,
    Mark { hash: String, status: MarkStatus },
}

pub(crate) async fn server_side<R, W, S>(
    stream: S,
    config: &server::Config,
) -> io::Result<HandshakeOutcome>
where
    S: Splittable<R, W>,
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let (mut from_client, mut to_client) = json_channel::<Request, Answer, _, _, _>(stream);

    if let Some(msg) = from_client.try_next().await? {
        if msg.version != VERSION {
            to_client
                .send(Answer::DifferentVersion(VERSION.to_owned()))
                .await?;
            return Ok(HandshakeOutcome::Denied);
        }
        match msg.payload {
            RequestPayload::ProcessingClient { groups } => {
                let unknown_groups: Vec<_> = groups
                    .into_iter()
                    .filter(|g| !config.is_proc_group(g))
                    .collect();
                if unknown_groups.is_empty() {
                    to_client.send(Answer::Ok).await?;
                    Ok(HandshakeOutcome::Success(ClientKind::Processing))
                } else {
                    to_client
                        .send(Answer::UnknownGroups(unknown_groups))
                        .await?;
                    Ok(HandshakeOutcome::Denied)
                }
            }
            RequestPayload::Mark { hash, status } => {
                to_client.send(Answer::Ok).await?;
                Ok(HandshakeOutcome::Success(ClientKind::Mark { hash, status }))
            }
        }
    } else {
        Ok(HandshakeOutcome::ClosedConnection)
    }
}

pub(crate) async fn client_side<R, W, S>(stream: S, payload: RequestPayload) -> io::Result<bool>
where
    S: Splittable<R, W>,
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let (mut from_server, mut to_server) = json_channel::<Answer, Request, _, _, _>(stream);

    to_server
        .send(Request {
            version: VERSION.to_owned(),
            payload,
        })
        .await?;

    if let Some(msg) = from_server.try_next().await? {
        match msg {
            Answer::Ok => Ok(true),
            Answer::DifferentVersion(version) => {
                error!("server reported a different version {version:?} (client is {VERSION})");
                Ok(false)
            }
            Answer::UnknownGroups(items) => {
                error!("server reported unknown groups {items:?}");
                Ok(false)
            }
        }
    } else {
        warn!("server closed connection");
        Ok(false)
    }
}
