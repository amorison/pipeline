use std::{
    collections::HashSet,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use pipeline::{NewFileToProcess, ReadFramedJson, Receipt, WriteFramedJson};
use tokio::{fs, net::TcpStream};

type Db = Arc<Mutex<HashSet<PathBuf>>>;

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
        }
    }
    Err(io::Error::new(
        io::ErrorKind::ConnectionAborted,
        "Connection closed by server",
    ))
}

async fn watch_dir(mut to_server: WriteFramedJson<NewFileToProcess>, db: Db) -> io::Result<()> {
    loop {
        let mut files = fs::read_dir("./dummy-folder/client").await?;
        while let Some(entry) = files.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let client_path = entry.path().canonicalize()?;
                if client_path.extension().is_some_and(|ext| ext == "mrc")
                    && let Ok(last_modif) = client_path.metadata()?.modified()?.elapsed()
                    && last_modif > Duration::from_secs(10)
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

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("127.0.0.1:12345").await?;

    let (from_server, to_server) =
        pipeline::framed_json_channel::<Receipt, NewFileToProcess>(stream);

    let db = Arc::new(Mutex::new(HashSet::new()));

    tokio::select!(
        handle = tokio::spawn(listen_to_server(from_server, db.clone())) => {
            let res = handle.unwrap();
            res
        }
        res = watch_dir(to_server, db) => res
    )
}
