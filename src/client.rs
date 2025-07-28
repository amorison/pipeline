use std::{collections::HashSet, io, path::PathBuf, sync::Arc, time::Duration};

use crate::{FileSpec, ReadFramedJson, Receipt, WriteFramedJson, replace_os_strings};
use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use serde::{Deserialize, Serialize};
use tokio::{fs, net::TcpStream, process::Command, sync::Mutex};

type Db = Arc<Mutex<HashSet<PathBuf>>>;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    server: String,
    copy_to_server: Vec<String>,
    watching: Watching,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: "127.0.0.1:12345".to_owned(),
            copy_to_server: ["cp", "{client_path}", "./server/{server_filename}"]
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
            watching: Watching {
                directory: "./client".into(),
                extension: "mrc".to_owned(),
                last_modif_secs: 10,
                refresh_every_secs: 5,
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct Watching {
    directory: PathBuf,
    extension: String,
    last_modif_secs: u64,
    refresh_every_secs: u64,
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
                let path = spec.client_path();
                fs::remove_file(path).await?;
                let in_db = db.try_lock().unwrap().remove(path);
                println!("Client got confirmation for {spec:?}, was in db: {in_db}");
            }
            Receipt::DifferentHash { spec, .. } => {
                println!(
                    "Client got told by server {spec:?} doesn't have expected hash, resending"
                );
                send_file_to_server(to_server.clone(), spec, conf.clone()).await?;
            }
            Receipt::Error { spec, error } => {
                println!("Client got told by server '{error}' for {spec:?}, resending");
                send_file_to_server(to_server.clone(), spec, conf.clone()).await?;
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
) -> io::Result<()> {
    let mut copy = Command::new(&conf.copy_to_server[0])
        .args(conf.copy_to_server[1..].iter().map(|a| {
            replace_os_strings(
                a,
                [
                    ("{client_path}", spec.client_path.as_os_str()),
                    ("{server_filename}", spec.server_filename()),
                ]
                .into_iter(),
            )
        }))
        .spawn()
        .expect("could not spawn `copy_to_server` command");
    copy.wait().await?;
    to_server.lock().await.send(spec).await
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
                let client_path = entry.path().canonicalize()?;
                if client_path
                    .extension()
                    .is_some_and(|ext| *ext == *conf.watching.extension)
                    && let Ok(last_modif) = client_path.metadata()?.modified()?.elapsed()
                    && last_modif > Duration::from_secs(conf.watching.last_modif_secs)
                    && insert_clone(&db, &client_path)
                {
                    let nfp = FileSpec::new(client_path)?;
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
    let stream = TcpStream::connect(&config.server).await?;

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
    fn write_read_default_config() {
        let conf_dflt = Config::default();
        let conf_toml = toml::to_string_pretty(&conf_dflt).expect("failed to write config");
        let conf_read: Config =
            toml::from_slice(conf_toml.as_bytes()).expect("failed to read config");
        assert_eq!(conf_read, conf_dflt);
    }
}
