use std::{
    collections::HashSet,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{NewFileToProcess, ReadFramedJson, Receipt, WriteFramedJson};
use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use serde::{Deserialize, Serialize};
use tokio::{fs, net::TcpStream};

type Db = Arc<Mutex<HashSet<PathBuf>>>;

#[derive(Serialize, Deserialize)]
pub struct Config {
    server: String,
    watching: Watching,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: "127.0.0.1:12345".to_owned(),
            watching: Watching {
                extension: "mrc".to_owned(),
                last_modif_secs: 10,
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Watching {
    extension: String,
    last_modif_secs: u64,
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

async fn watch_dir(
    mut to_server: WriteFramedJson<NewFileToProcess>,
    db: Db,
    conf: Watching,
) -> io::Result<()> {
    loop {
        let mut files = fs::read_dir("./dummy-folder/client").await?;
        while let Some(entry) = files.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let client_path = entry.path().canonicalize()?;
                if client_path
                    .extension()
                    .is_some_and(|ext| *ext == *conf.extension)
                    && let Ok(last_modif) = client_path.metadata()?.modified()?.elapsed()
                    && last_modif > Duration::from_secs(conf.last_modif_secs)
                    && insert_clone(&db, &client_path)
                {
                    let mut server_path = PathBuf::from("./dummy-folder/server");
                    server_path.push(client_path.file_name().unwrap());
                    fs::copy(&client_path, &server_path).await?;
                    let nfp = NewFileToProcess::new(client_path, server_path)?;
                    to_server.send(nfp).await?;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
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

pub async fn main(config: Config) -> io::Result<()> {
    let stream = TcpStream::connect(&config.server).await?;

    let (from_server, to_server) = crate::framed_json_channel::<Receipt, NewFileToProcess>(stream);

    let db = Arc::new(Mutex::new(HashSet::new()));

    tokio::select!(
        handle = tokio::spawn(listen_to_server(from_server, db.clone())) => handle.unwrap(),
        res = watch_dir(to_server, db, config.watching) => res,
    )
}
