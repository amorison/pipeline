mod ssh_tunnel;

use std::{
    collections::HashSet,
    ffi::OsStr,
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, LazyLock},
    time::Duration,
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
    copy_to_server: Vec<String>,
    server: Server,
    watching: Watching,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum Server {
    Address(String),
    SshTunnel(SshTunnelConfig),
}

#[derive(Deserialize, Debug, Clone)]
struct SshTunnelConfig {
    ssh_host: String,
    ssh_port: u16,
    ssh_auth: SshAuth,
    server_addr_from_host: String,
    server_port_from_host: u16,
    accepted_ssh_keys: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "strategy", rename_all = "kebab-case")]
enum SshAuth {
    None { user: String },
    Password { user: String },
    Key { user: String, public_key: PathBuf },
}

pub(crate) static DEFAULT_TOML_CONF: LazyLock<String> = LazyLock::new(|| {
    format!(
        include_str!("client/default.toml"),
        server_conf = r#"server = "127.0.0.1:12345""#
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
}

impl Config {
    fn watched_path(&self, filename: impl AsRef<Path>) -> PathBuf {
        self.watching.directory.join(filename)
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
                let path = &conf.watched_path(spec.filename);
                if let Err(err) = fs::remove_file(path).await {
                    warn!("error when removing {path:?}: {err}");
                }
                db.try_lock().unwrap().remove(path);
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

async fn send_file_to_server(
    to_server: Arc<Mutex<WriteFramedJson<FileSpec>>>,
    spec: FileSpec,
    conf: Arc<Config>,
) {
    info!("copying {spec:?} to server");
    let mut copy = Command::new(&conf.copy_to_server[0])
        .args(conf.copy_to_server[1..].iter().map(|a| {
            replace_os_strings(
                a,
                [
                    ("{server_filename}", spec.server_filename()),
                    (
                        "{client_path}",
                        conf.watched_path(&spec.filename).as_os_str(),
                    ),
                ]
                .into_iter(),
            )
        }))
        .spawn()
        .expect("could not spawn `copy_to_server` command");
    match copy.wait().await {
        Ok(status) if status.success() => to_server
            .lock()
            .await
            .send(spec)
            .await
            .expect("couldn't send request to server"),
        Ok(status) => warn!(
            "copy of {spec:?} to server failed with status {:?}",
            status.code()
        ),
        Err(err) => warn!("copy of {spec:?} to server failed '{err}'"),
    }
}

async fn watch_dir(
    to_server: Arc<Mutex<WriteFramedJson<FileSpec>>>,
    db: Db,
    conf: Arc<Config>,
) -> io::Result<()> {
    loop {
        let mut files = fs::read_dir(&conf.watching.directory).await?;
        while let Some(entry) = files.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let client_path = entry.path();
                if client_path
                    .extension()
                    .is_some_and(|ext| *ext == *conf.watching.extension)
                    && client_path.file_name().map(OsStr::to_str).is_some()
                    && let Ok(last_modif) = client_path.metadata()?.modified()?.elapsed()
                    && last_modif > Duration::from_secs(conf.watching.last_modif_secs)
                    && insert_clone(&db, &client_path)
                {
                    let nfp = FileSpec::new(client_path)?;
                    info!("found file to process {nfp:?}");
                    to_server.lock().await.send(nfp).await?;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(conf.watching.refresh_every_secs)).await;
    }
}

fn insert_clone(db: &Db, path: &PathBuf) -> bool {
    let mut db = db.try_lock().unwrap();
    if db.contains(path) {
        false
    } else {
        db.insert(path.clone())
    }
}

pub(crate) async fn main(config: Config) -> io::Result<()> {
    let addr = match &config.server {
        Server::Address(addr) => SocketAddr::from_str(addr)
            .expect(&format!("Failed to parse {addr} as a socket address")),
        Server::SshTunnel(conf) => ssh_tunnel::setup_tunnel(conf.clone()).await,
    };

    let stream = TcpStream::connect(addr).await?;

    let (from_server, to_server) = crate::framed_json_channel::<Receipt, FileSpec>(stream);

    let to_server = Arc::new(Mutex::new(to_server));
    let db = Arc::new(Mutex::new(HashSet::new()));
    let config = Arc::new(config);

    tokio::select!(
        handle = tokio::spawn(listen_to_server(from_server, to_server.clone(), db.clone(), config.clone())) => handle.unwrap(),
        res = watch_dir(to_server, db, config) => res,
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
