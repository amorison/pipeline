use std::{io, path::Path, sync::Arc};

use crate::{NewFileToProcess, Receipt, WriteFramedJson, file_hash};
use futures_util::{SinkExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

#[derive(Serialize, Deserialize)]
pub struct Config {
    addr: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            addr: "127.0.0.1:12345".to_owned(),
        }
    }
}

async fn process_file(source: &Path, dest: &Path) -> io::Result<()> {
    fs::rename(source, dest).await?;
    Ok(())
}

async fn processing_pipeline(
    file: NewFileToProcess,
    channel: Arc<Mutex<WriteFramedJson<Receipt>>>,
) {
    let NewFileToProcess(spec) = file;
    let receipt = match file_hash(&spec.server_path) {
        Ok(received_hash) => {
            if spec.sha256_digest == received_hash {
                Receipt::Received(spec.clone())
            } else {
                Receipt::DifferentHash {
                    spec: spec.clone(),
                    received_hash,
                }
            }
        }
        Err(err) => Receipt::Error(err.to_string()),
    };
    channel.lock().await.send(receipt).await.unwrap();

    process_file(&spec.server_path, &spec.server_path.with_extension("tiff"))
        .await
        .unwrap();
}

async fn handle_client(stream: TcpStream) -> io::Result<()> {
    let (mut from_client, to_client) =
        crate::framed_json_channel::<NewFileToProcess, Receipt>(stream);
    let to_client = Arc::new(Mutex::new(to_client));

    while let Some(msg) = from_client.try_next().await? {
        println!("Server got: {msg:?}");
        tokio::spawn(processing_pipeline(msg, to_client.clone()));
    }
    Ok(())
}

pub async fn main(config: Config) -> io::Result<()> {
    let listener = TcpListener::bind(&config.addr).await?;

    println!("Server listening on {:?}", listener.local_addr());

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("Server got connection request from {addr:?}");
        tokio::spawn(handle_client(socket));
    }
}
