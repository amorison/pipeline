use std::{io, sync::Arc};

use chrono::{Local, SecondsFormat};
use log::warn;
use serde::Deserialize;
use tabled::{Table, settings::Style};
use tokio::{net::TcpStream, sync::Mutex};

use crate::{
    cli::MarkStatus,
    framed_io::json_channel,
    handshake::{self, RequestPayload},
    server::{Database, Status, database::FileInPipeline},
    server_route::ServerRoute,
};
use futures_util::{TryStreamExt, sink::SinkExt};

#[derive(Clone)]
pub(crate) enum Query {
    Mark { hash: String, status: MarkStatus },
    List,
    PruneDone,
    Status,
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
            Query::Status => {
                let (mut from_server, _) = json_channel::<Status, (), _, _, _>(stream);
                let status = from_server
                    .try_next()
                    .await?
                    .expect("should have exactly one answer");
                let start_utc = status
                    .start_time()
                    .to_rfc3339_opts(SecondsFormat::Secs, true);
                let start_local = status
                    .start_time()
                    .with_timezone(&Local)
                    .to_rfc3339_opts(SecondsFormat::Secs, false);
                println!("server has been running since {start_utc}");
                println!("(local {start_local})\n");
                println!("Number of files received:\n");
                let mut tally_table = status.tally_table();
                tally_table.with(
                    Style::markdown()
                        .remove_vertical()
                        .remove_left()
                        .remove_right(),
                );
                println!("{tally_table}");
                println!();
                println!("Clients currently connected:");
                for (name, &count) in status.clients() {
                    let count_str = if count == 1 {
                        String::new()
                    } else {
                        format!(" ({count})")
                    };
                    println!("  - {name}{count_str}");
                }
                Ok(())
            }
        }
    }
}

impl From<Query> for RequestPayload {
    fn from(value: Query) -> Self {
        match value {
            Query::Mark { hash, status } => RequestPayload::Mark { hash, status },
            Query::List => RequestPayload::List,
            Query::PruneDone => RequestPayload::PruneDone,
            Query::Status => RequestPayload::Status,
        }
    }
}

/// Minimal configuration file for commands that only query the server.
#[derive(Deserialize, Debug)]
pub(crate) struct QueryConfig {
    server: ServerRoute,
}

pub(crate) static QUERY_TOML_CONF: &str = include_str!("query.toml");

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

pub(super) async fn process_status_query(
    stream: TcpStream,
    status: Arc<Mutex<Status>>,
) -> io::Result<()> {
    let status = status.lock().await;
    let (_, mut to_client) = json_channel::<(), Status, _, _, _>(stream);
    to_client.send(status.clone()).await
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn read_query_conf() {
        let bytes = QUERY_TOML_CONF.as_bytes();
        assert!(toml::from_slice::<QueryConfig>(bytes).is_ok())
    }

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
