use std::io;

use futures_util::{SinkExt, TryStreamExt};
use log::error;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

use crate::{client, framed_io::borrowed_json_channel, server};

static VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize, Debug)]
struct Request {
    version: String,
    processing_groups: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
enum Answer {
    Ok,
    DifferentVersion(String),
    UnknownGroups(Vec<String>),
}

pub(crate) enum HandshakeOutcome {
    Success,
    Denied,
    ClosedConnection,
}

pub(crate) async fn server_side(
    stream: &mut TcpStream,
    config: &server::Config,
) -> io::Result<HandshakeOutcome> {
    let (mut from_client, mut to_client) = borrowed_json_channel::<Request, Answer>(stream);

    if let Some(msg) = from_client.try_next().await? {
        if msg.version != VERSION {
            to_client
                .send(Answer::DifferentVersion(VERSION.to_owned()))
                .await?;
            return Ok(HandshakeOutcome::Denied);
        }
        let unknown_groups: Vec<_> = msg
            .processing_groups
            .into_iter()
            .filter(|g| !config.is_proc_group(g))
            .collect();
        if unknown_groups.is_empty() {
            to_client.send(Answer::Ok).await?;
            Ok(HandshakeOutcome::Success)
        } else {
            to_client
                .send(Answer::UnknownGroups(unknown_groups))
                .await?;
            Ok(HandshakeOutcome::Denied)
        }
    } else {
        Ok(HandshakeOutcome::ClosedConnection)
    }
}

pub(crate) async fn client_side(
    stream: &mut TcpStream,
    config: &client::Config,
) -> io::Result<HandshakeOutcome> {
    let (mut from_server, mut to_server) = borrowed_json_channel::<Answer, Request>(stream);

    to_server
        .send(Request {
            version: VERSION.to_owned(),
            processing_groups: config.processing_groups(),
        })
        .await?;

    if let Some(msg) = from_server.try_next().await? {
        match msg {
            Answer::Ok => Ok(HandshakeOutcome::Success),
            Answer::DifferentVersion(version) => {
                error!("server reported a different version {version:?} (client is {VERSION})");
                Ok(HandshakeOutcome::Denied)
            }
            Answer::UnknownGroups(items) => {
                error!("server reported unknown groups {items:?}");
                Ok(HandshakeOutcome::Denied)
            }
        }
    } else {
        Ok(HandshakeOutcome::ClosedConnection)
    }
}
