use std::io;

use log::warn;
use serde::Deserialize;
use tabled::{Table, settings::Style};

use crate::{
    cli::MarkStatus,
    handshake::{self, RequestPayload},
    server::{Database, database::DatabaseReadOnly},
    server_route::ServerRoute,
};

pub(crate) enum Query {
    Mark { hash: String, status: MarkStatus },
}

impl From<Query> for RequestPayload {
    fn from(value: Query) -> Self {
        match value {
            Query::Mark { hash, status } => RequestPayload::Mark { hash, status },
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
    let payload = query.into();
    if !handshake::client_side(&mut stream, payload).await? {
        return Err(io::Error::other("handshake failed"));
    }
    Ok(())
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

pub(crate) async fn process_list_query() -> io::Result<()> {
    let db = DatabaseReadOnly::new().await.unwrap();
    let content = db.content().await.unwrap();
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
