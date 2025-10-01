mod ssh_tunnel;
mod watch;

use std::{
    collections::HashSet,
    io,
    net::SocketAddr,
    path::PathBuf,
    process::ExitStatus,
    str::FromStr,
    sync::{Arc, LazyLock},
};

use crate::{FileSpec, ReadFramedJson, Receipt, WriteFramedJson, replace_os_strings};
use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use log::{info, warn};
use serde::Deserialize;
use tokio::{fs, net::TcpStream, process::Command, sync::Mutex};

type Db = Arc<Mutex<HashSet<PathBuf>>>;

#[derive(Deserialize, Debug)]
pub(crate) struct Config {
    name: String,
    copy_to_server: CopyToServer,
    server: Server,
    watching: Watching,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum CopyToServer {
    Copy { destination: PathBuf },
    Command(Vec<String>),
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum Server {
    Direct { address: String },
    SshTunnel(SshTunnelConfig),
}

#[derive(Deserialize, Debug, Clone)]
struct SshTunnelConfig {
    ssh_host: String,
    ssh_port: u16,
    ssh_auth: SshAuth,
    keepalive_every_secs: u64,
    server_addr_from_host: String,
    server_port_from_host: u16,
    accepted_ssh_keys: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "method", rename_all = "kebab-case")]
enum SshAuth {
    None { user: String },
    Password { user: String },
    Key { user: String, public_key: PathBuf },
}

pub(crate) static DEFAULT_TOML_CONF: LazyLock<String> = LazyLock::new(|| {
    format!(
        include_str!("client/default.toml"),
        server_conf = "[server]\naddress = \"127.0.0.1:12345\""
    )
});

pub(crate) static TUNNEL_TOML_CONF: LazyLock<String> = LazyLock::new(|| {
    format!(
        include_str!("client/default.toml"),
        server_conf = include_str!("client/tunnel.toml").trim_end()
    )
});

#[derive(Deserialize, Debug)]
struct Watching {
    directory: PathBuf,
    extension: String,
    last_modif_secs: u64,
    refresh_every_secs: u64,
    max_concurrent_hashes: usize,
}

impl Config {
    fn watched_path(&self, spec: &FileSpec) -> PathBuf {
        self.watching.directory.join(spec.relative_path())
    }
}

async fn listen_to_server(
    mut from_server: ReadFramedJson<Receipt>,
    to_server: Arc<Mutex<WriteFramedJson<FileSpec>>>,
    db: Db,
    conf: Arc<Config>,
) -> io::Result<()> {
    while let Some(msg) = from_server.try_next().await? {
        match msg {
            Receipt::Received(spec) => {
                info!("server confirmed reception of {spec:?}");
                let path = conf.watched_path(&spec);
                if let Err(err) = fs::remove_file(&path).await {
                    warn!("error when removing {path:?}: {err}");
                }
                db.lock().await.remove(&spec.relative_path());
            }
            Receipt::DifferentHash { spec, .. } => {
                info!("server does not have expected hash for {spec:?}, resending");
                send_file_to_server(to_server.clone(), spec, conf.clone()).await;
            }
            Receipt::Error { spec, error } => {
                info!("server says '{error}' for {spec:?}, resending");
                send_file_to_server(to_server.clone(), spec, conf.clone()).await;
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::ConnectionAborted,
        "Connection closed by server",
    ))
}

enum CopyOutcome {
    Ok,
    ErrCommand(ExitStatus),
    Err(io::Error),
}

impl From<io::Result<ExitStatus>> for CopyOutcome {
    fn from(value: io::Result<ExitStatus>) -> Self {
        match value {
            Ok(status) if status.success() => Self::Ok,
            Ok(status) => Self::ErrCommand(status),
            Err(err) => Self::Err(err),
        }
    }
}

impl From<io::Result<()>> for CopyOutcome {
    fn from(value: io::Result<()>) -> Self {
        match value {
            Ok(()) => Self::Ok,
            Err(err) => Self::Err(err),
        }
    }
}

async fn send_file_to_server(
    to_server: Arc<Mutex<WriteFramedJson<FileSpec>>>,
    spec: FileSpec,
    conf: Arc<Config>,
) {
    let from = conf.watched_path(&spec);
    let outcome = match &conf.copy_to_server {
        CopyToServer::Copy { destination } => {
            info!("copying {spec:?} to server via `fs::copy`");
            let mut destination = destination.clone();
            destination.push(spec.server_filename());
            fs::copy(from, destination).await.map(|_| ()).into()
        }
        CopyToServer::Command(items) => {
            info!("copying {spec:?} to server with `{}`", &items[0]);
            Command::new(&items[0])
                .args(items[1..].iter().map(|a| {
                    replace_os_strings(
                        a,
                        [
                            ("{server_filename}", spec.server_filename()),
                            ("{client_path}", from.as_os_str()),
                        ]
                        .into_iter(),
                    )
                }))
                .spawn()
                .expect("could not spawn `copy_to_server` command")
                .wait()
                .await
                .into()
        }
    };
    match outcome {
        CopyOutcome::Ok => {
            info!("copy of {spec:?} completed successfully");
            to_server
                .lock()
                .await
                .send(spec)
                .await
                .expect("couldn't send request to server");
        }
        CopyOutcome::ErrCommand(status) => warn!(
            "copy of {spec:?} to server failed with status {:?}",
            status.code()
        ),
        CopyOutcome::Err(err) => warn!("copy of {spec:?} to server failed '{err}'"),
    }
}

pub(crate) async fn main(config: Config) -> io::Result<()> {
    let addr = match &config.server {
        Server::Direct { address } => SocketAddr::from_str(address)
            .unwrap_or_else(|_| panic!("Failed to parse {address} as a socket address")),
        Server::SshTunnel(conf) => ssh_tunnel::setup_tunnel(conf.clone()).await,
    };

    let stream = TcpStream::connect(addr).await?;
    info!("connected to server at {addr}");

    let (from_server, to_server) = crate::framed_json_channel::<Receipt, FileSpec>(stream);

    let to_server = Arc::new(Mutex::new(to_server));
    let db = Arc::new(Mutex::new(HashSet::new()));
    let config = Arc::new(config);

    tokio::select!(
        handle = tokio::spawn(listen_to_server(from_server, to_server.clone(), db.clone(), config.clone())) => handle.unwrap(),
        res = watch::watch_dir(to_server, db, config) => res,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn read_default_config() {
        assert!(toml::from_slice::<Config>(DEFAULT_TOML_CONF.as_bytes()).is_ok());
    }

    #[test]
    fn read_tunnel_config() {
        assert!(toml::from_slice::<Config>(TUNNEL_TOML_CONF.as_bytes()).is_ok());
    }
}
