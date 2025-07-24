use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{NewFileToProcess, ReadFramedJson, Receipt, WriteFramedJson};
use bstr::{ByteSlice, ByteVec};
use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use serde::{Deserialize, Serialize};
use tokio::{fs, net::TcpStream, process::Command};

type Db = Arc<Mutex<HashSet<PathBuf>>>;

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
struct Watching {
    directory: PathBuf,
    extension: String,
    last_modif_secs: u64,
    refresh_every_secs: u64,
}

async fn listen_to_server(mut from_server: ReadFramedJson<Receipt>, db: Db) -> io::Result<()> {
    while let Some(msg) = from_server.try_next().await? {
        match msg {
            Receipt::Received(spec) => {
                let path = spec.client_path();
                fs::remove_file(path).await?;
                let in_db = db.try_lock().unwrap().remove(path);
                println!("Client got confirmation for {spec:?}, was in db: {in_db}");
            }
            Receipt::DifferentHash { .. } => todo!(),
            Receipt::Error(_) => todo!(),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::ConnectionAborted,
        "Connection closed by server",
    ))
}

fn replace_filepaths(arg: &str, client_path: &Path, server_filename: &OsStr) -> OsString {
    arg.as_bytes()
        .replace("{client_path}", client_path.as_os_str().as_encoded_bytes())
        .replace("{server_filename}", server_filename.as_encoded_bytes())
        .into_os_string()
        .expect("failed to encode paths")
}

async fn watch_dir(
    mut to_server: WriteFramedJson<NewFileToProcess>,
    db: Db,
    conf: Config,
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
                    let server_filename = client_path.file_name().unwrap().to_owned();
                    let mut copy = Command::new(&conf.copy_to_server[0])
                        .args(
                            conf.copy_to_server[1..]
                                .iter()
                                .map(|a| replace_filepaths(a, &client_path, &server_filename)),
                        )
                        .spawn()
                        .expect("could not spawn `copy_to_server` command");
                    copy.wait().await?;
                    let nfp = NewFileToProcess::new(client_path, server_filename)?;
                    to_server.send(nfp).await?;
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

    let (from_server, to_server) = crate::framed_json_channel::<Receipt, NewFileToProcess>(stream);

    let db = Arc::new(Mutex::new(HashSet::new()));

    tokio::select!(
        handle = tokio::spawn(listen_to_server(from_server, db.clone())) => handle.unwrap(),
        res = watch_dir(to_server, db, config) => res,
    )
}
