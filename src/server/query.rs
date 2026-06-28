pub(crate) mod list;
pub(crate) mod mark;

use std::io;

use serde::Deserialize;

use crate::{
    cli::MarkStatus,
    handshake::{self, RequestPayload},
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
