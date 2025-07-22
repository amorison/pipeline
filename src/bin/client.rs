use std::{
    collections::HashSet,
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use pipeline::{NewFileToProcess, ReadFramedJson, Received};
use tokio::{fs, net::TcpStream};

async fn listen_to_server(mut from_server: ReadFramedJson<Received>) {
    while let Some(msg) = from_server.try_next().await.unwrap() {
        println!("Client got: {msg:?}");
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let stream = TcpStream::connect("127.0.0.1:12345").await.unwrap();

    let (from_server, mut to_server) =
        pipeline::framed_json_channel::<Received, NewFileToProcess>(stream);

    let db = Arc::new(Mutex::new(HashSet::new()));

    tokio::spawn(listen_to_server(from_server));

    loop {
        let mut files = fs::read_dir(".").await?;
        while let Some(entry) = files.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let path = entry.path();
                if db.try_lock().unwrap().insert(path.clone()) {
                    let nfp = NewFileToProcess::new(path);
                    to_server.send(nfp).await.unwrap();
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
