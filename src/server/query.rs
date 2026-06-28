use std::io;

use log::warn;
use serde::Deserialize;
use tabled::{Table, settings::Style};
use tokio::net::TcpStream;

use crate::{
    cli::MarkStatus,
    framed_io::json_channel,
    handshake::{self, RequestPayload},
    server::{Database, database::FileInPipeline},
    server_route::ServerRoute,
};
use futures_util::{TryStreamExt, sink::SinkExt};

#[derive(Clone)]
pub(crate) enum Query {
    Mark { hash: String, status: MarkStatus },
    List,
    PruneDone,
}

impl Query {
    async fn get_response(&self, stream: TcpStream) -> io::Result<()> {
        match self {
            Query::Mark { .. } => Ok(()),
            Query::List => {
                let (mut from_server, _) = json_channel::<Vec<FileInPipeline>, (), _, _, _>(stream);
                let content = from_server
                    .try_next()
                    .await?
                    .expect("should have exactly one answer");
                let mut table = Table::new(&content);
                table.with(
                    Style::markdown()
                        .remove_vertical()
                        .remove_left()
                        .remove_right(),
                );
                println!("{table}");
                Ok(())
            }
            Query::PruneDone => Ok(()),
        }
    }
}

impl From<Query> for RequestPayload {
    fn from(value: Query) -> Self {
        match value {
            Query::Mark { hash, status } => RequestPayload::Mark { hash, status },
            Query::List => RequestPayload::List,
            Query::PruneDone => RequestPayload::PruneDone,
        }
    }
}

/// Minimal configuration file for commands that only query the server.
#[derive(Deserialize, Debug)]
pub(crate) struct QueryConfig {
    server: ServerRoute,
}

pub(crate) async fn main(config: QueryConfig, query: Query) -> io::Result<()> {
    let mut stream = config.server.connect().await;
    let payload = query.clone().into();
    if !handshake::client_side(&mut stream, payload).await? {
        return Err(io::Error::other("handshake failed"));
    }
    query.get_response(stream).await
}

pub(super) async fn process_mark_query(
    db: Database,
    hash: String,
    status: MarkStatus,
) -> io::Result<()> {
    while let Err(err) = db.update_status(&hash, status.into()).await {
        warn!("error updating status for {hash}: {err}");
    }
    Ok(())
}

pub(super) async fn process_list_query(stream: TcpStream, db: Database) -> io::Result<()> {
    let content = db.content().await.unwrap();
    let (_, mut to_client) = json_channel::<(), Vec<FileInPipeline>, _, _, _>(stream);
    to_client.send(content).await
}

pub(super) async fn process_prune_done_query(db: Database) -> io::Result<()> {
    if let Err(err) = db.mark_done_to_prune().await {
        warn!("error marking 'done' tasks to prune: {err}");
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn default_server_conf_as_query() {
        let bytes = crate::server::DEFAULT_TOML_CONF.as_bytes();
        assert!(toml::from_slice::<QueryConfig>(bytes).is_ok())
    }

    #[test]
    fn default_client_conf_as_query() {
        let bytes = crate::client::DEFAULT_TOML_CONF.as_bytes();
        assert!(toml::from_slice::<QueryConfig>(bytes).is_ok())
    }

    #[test]
    fn tunnel_client_conf_as_query() {
        let bytes = crate::client::TUNNEL_TOML_CONF.as_bytes();
        assert!(toml::from_slice::<QueryConfig>(bytes).is_ok())
    }
}
