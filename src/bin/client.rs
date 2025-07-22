use std::{
    collections::HashSet,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use pipeline::{NewFileToProcess, ReadFramedJson, Received};
use tokio::{fs, net::TcpStream};

type Db = Arc<Mutex<HashSet<PathBuf>>>;

async fn listen_to_server(mut from_server: ReadFramedJson<Received>, db: Db) {
    while let Some(msg) = from_server.try_next().await.unwrap() {
        let in_db = db.try_lock().unwrap().remove(msg.path());
        println!("Client got: {msg:?}, was in db: {in_db}");
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

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("127.0.0.1:12345").await.unwrap();

    let (from_server, mut to_server) =
        pipeline::framed_json_channel::<Received, NewFileToProcess>(stream);

    let db = Arc::new(Mutex::new(HashSet::new()));

    tokio::spawn(listen_to_server(from_server, db.clone()));

    loop {
        let mut files = fs::read_dir(".").await?;
        while let Some(entry) = files.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "mrc")
                    && let Ok(last_modif) = path.metadata()?.modified()?.elapsed()
                    && last_modif > Duration::from_secs(10)
                {
                    if insert_clone(&db, &path) {
                        let nfp = NewFileToProcess::new(path);
                        to_server.send(nfp?).await.unwrap();
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
