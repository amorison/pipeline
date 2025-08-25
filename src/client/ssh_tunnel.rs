use std::{collections::HashSet, sync::Arc};

use log::{info, warn};
use russh::{
    client::{self as ssh_client, Handle},
    keys::{PublicKey, ssh_key::public::KeyData},
};
use std::net::SocketAddr;
use tokio::net::TcpListener;

use crate::client::SshTunnelConfig;

struct Client {
    accepted_keys: HashSet<KeyData>,
}

impl Client {
    fn from_openssh_keys<S: AsRef<str>>(keys: impl IntoIterator<Item = S>) -> Client {
        let accepted_keys = keys
            .into_iter()
            .map(|key_str| {
                PublicKey::from_openssh(key_str.as_ref())
                    .expect("Failed to parse public key")
                    .into()
            })
            .collect();
        Client { accepted_keys }
    }
}

impl ssh_client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let ossh = server_public_key.to_openssh()?;
        if self.accepted_keys.contains(server_public_key.key_data()) {
            info!("accepted connection to {ossh}");
            Ok(true)
        } else {
            warn!("unknown server key, refusing connection: {ossh}");
            Ok(false)
        }
    }
}

async fn create_session(client: Client, conf: &SshTunnelConfig) -> Handle<Client> {
    let ssh_config = Arc::new(ssh_client::Config::default());

    let mut ssh_session =
        ssh_client::connect(ssh_config, (conf.ssh_host.as_str(), conf.ssh_port), client)
            .await
            .expect("Connection to SSH host failed");

    ssh_session
        .authenticate_none(&conf.ssh_user)
        .await
        .expect("Failed to authenticate");

    ssh_session
}

pub(super) async fn setup_tunnel(conf: SshTunnelConfig) -> SocketAddr {
    let ssh_client = Client::from_openssh_keys(&conf.accepted_ssh_keys);

    let ssh_session = create_session(ssh_client, &conf).await;

    let local_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Cannot bind local port");
    let local_addr = local_listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (mut local_socket, _) = local_listener
            .accept()
            .await
            .expect("Cannot process local client");

        let ssh_channel = ssh_session
            .channel_open_direct_tcpip(
                conf.server_addr_from_host,
                conf.server_port_from_host as u32,
                local_addr.ip().to_string(),
                local_addr.port() as u32,
            )
            .await
            .expect("Cannot open SSH forwarding channel");

        let mut ssh_stream = ssh_channel.into_stream();

        tokio::io::copy_bidirectional(&mut local_socket, &mut ssh_stream)
            .await
            .expect("Copy error between local socket and SSH stream");
    });

    local_addr
}
