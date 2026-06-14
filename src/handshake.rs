use std::io;

use futures_util::{SinkExt, TryStreamExt};
use log::error;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

use crate::{client, framed_io::borrowed_json_channel, server};

#[derive(Serialize, Deserialize, Debug)]
struct Request {
    processing_groups: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
enum Answer {
    Ok,
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
        let unknown_groups: Vec<_> = msg
            .processing_groups
            .into_iter()
            .filter(|g| !config.is_proc_group(g))
            .collect();
        if unknown_groups.is_empty() {
            to_client.send(Answer::Ok).await.unwrap();
            Ok(HandshakeOutcome::Success)
        } else {
            to_client
                .send(Answer::UnknownGroups(unknown_groups))
                .await
                .unwrap();
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
            processing_groups: config.processing_groups(),
        })
        .await
        .unwrap();

    if let Some(msg) = from_server.try_next().await? {
        match msg {
            Answer::Ok => Ok(HandshakeOutcome::Success),
            Answer::UnknownGroups(items) => {
                error!("server reported unknown groups {items:?}");
                Ok(HandshakeOutcome::Denied)
            }
        }
    } else {
        Ok(HandshakeOutcome::ClosedConnection)
    }
}
