mod ssh_tunnel;
mod watch;

use std::{
    collections::HashSet,
    io,
    path::PathBuf,
    process::ExitStatus,
    sync::{Arc, LazyLock},
    time::Duration,
};

use crate::{
    FileSpec, Receipt, assemble_path,
    framed_io::{ReadFramedJson, WriteFramedJson, framed_json_channel},
    replace_os_strings,
};
use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use log::{info, warn};
use serde::Deserialize;
use tokio::{
    fs,
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    process::Command,
    sync::Mutex,
};

type Db = Arc<Mutex<HashSet<PathBuf>>>;
type ToServer = Arc<Mutex<WriteFramedJson<FileSpec, OwnedWriteHalf>>>;

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
    Move { move_in_same_fs_to: PathBuf },
    Copy { destination: PathBuf },
    Command(Vec<String>),
}

impl CopyToServer {
    fn requires_cleanup(&self) -> bool {
        match self {
            CopyToServer::Move { .. } => false,
            CopyToServer::Copy { .. } => true,
            CopyToServer::Command(_) => true,
        }
    }
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
    full_hash: bool,
}

impl Config {
    fn watched_path(&self, spec: &FileSpec) -> PathBuf {
        assemble_path(&self.watching.directory, spec.relative_path())
    }
}

async fn listen_to_server(
    mut from_server: ReadFramedJson<Receipt, OwnedReadHalf>,
    to_server: ToServer,
    db: Db,
    conf: Arc<Config>,
) -> io::Result<()> {
    while let Some(msg) = from_server.try_next().await? {
        match msg {
            Receipt::Expecting {
                spec,
                server_rel_path,
            } => {
                info!("server awaiting {spec:?}, sending according to `copy_to_server`");
                send_file_to_server(to_server.clone(), spec, server_rel_path, conf.clone()).await;
            }
            Receipt::Received(spec) => {
                info!("server confirmed reception of {spec:?}");
                if conf.copy_to_server.requires_cleanup() {
                    let path = conf.watched_path(&spec);
                    if let Err(err) = fs::remove_file(&path).await {
                        warn!("error when removing {path:?}: {err}");
                        continue;
                    }
                }
                db.lock().await.remove(&spec.relative_path());
            }
            Receipt::DifferentHash(spec) => {
                warn!(
                    "server does not have expected hash for {spec:?}, forgetting it in case of TOCTOU condition"
                );
                db.lock().await.remove(&spec.relative_path());
            }
            Receipt::Error {
                spec,
                server_rel_path,
                error,
            } => {
                warn!("server says '{error}' for {spec:?}, resending");
                send_file_to_server(to_server.clone(), spec, server_rel_path, conf.clone()).await;
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
    to_server: ToServer,
    spec: FileSpec,
    server_rel_path: String,
    conf: Arc<Config>,
) {
    let from = conf.watched_path(&spec);
    let outcome = match &conf.copy_to_server {
        CopyToServer::Move { move_in_same_fs_to } => {
            info!("move {spec:?} to server via `fs::rename`");
            let destination = assemble_path(move_in_same_fs_to, server_rel_path);
            fs::rename(from, destination).await.into()
        }
        CopyToServer::Copy { destination } => {
            info!("copying {spec:?} to server via `fs::copy`");
            let destination = assemble_path(destination, server_rel_path);
            fs::copy(from, destination).await.map(|_| ()).into()
        }
        CopyToServer::Command(items) => {
            info!("copying {spec:?} to server with `{}`", &items[0]);
            let rel_path = assemble_path(&server_rel_path, "");
            Command::new(&items[0])
                .args(items[1..].iter().map(|a| {
                    replace_os_strings(
                        a,
                        [
                            ("{server_filename}", rel_path.as_ref()),
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
    let stream = match &config.server {
        Server::Direct { address } => {
            let stream = loop {
                let stream = TcpStream::connect(address).await;
                match stream {
                    Ok(stream) => break stream,
                    Err(err) => {
                        warn!("cannot connect to {address}, will retry in 3s: {err}");
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                }
            };
            info!("connected to server at {address}");
            stream
        }
        Server::SshTunnel(conf) => {
            let stream = ssh_tunnel::setup_tunnel(conf.clone()).await;
            info!("connected to server via SSH tunnel");
            stream
        }
    };

    let (from_server, to_server) = framed_json_channel::<Receipt, FileSpec>(stream);

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
