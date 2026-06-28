pub(crate) mod mark;

use serde::Deserialize;

use crate::server_route::ServerRoute;

/// Minimal configuration file for commands that only query the server.
#[derive(Deserialize, Debug)]
pub(crate) struct QueryConfig {
    server: ServerRoute,
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
