use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{NewFileToProcess, Receipt, WriteFramedJson, file_hash};
use bstr::{ByteSlice, ByteVec};
use futures_util::{SinkExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::{
    net::{TcpListener, TcpStream},
    process::Command,
    sync::Mutex,
};

#[derive(Serialize, Deserialize)]
pub(crate) struct Config {
    address: String,
    incoming_directory: PathBuf,
    processing: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            address: "127.0.0.1:12345".to_owned(),
            incoming_directory: "./server".into(),
            processing: ["cp", "{file_path}", "{file_path}.tiff"]
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
        }
    }
}

fn replace_filepaths(arg: &str, file_path: &Path) -> OsString {
    arg.as_bytes()
        .replace("{file_path}", file_path.as_os_str().as_encoded_bytes())
        .into_os_string()
        .expect("failed to encode paths")
}

async fn processing_pipeline(
    file: NewFileToProcess,
    channel: Arc<Mutex<WriteFramedJson<Receipt>>>,
    config: Arc<Config>,
) {
    let NewFileToProcess(spec) = file;
    let mut server_path = config.incoming_directory.clone();
    server_path.push(&spec.server_filename);

    let receipt = match file_hash(&server_path) {
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

    let mut processing = Command::new(&config.processing[0])
        .args(
            config.processing[1..]
                .iter()
                .map(|a| replace_filepaths(a, &server_path)),
        )
        .spawn()
        .expect("could not spawn `copy_to_server` command");
    processing.wait().await.unwrap();
}

async fn handle_client(stream: TcpStream, config: Arc<Config>) -> io::Result<()> {
    let (mut from_client, to_client) =
        crate::framed_json_channel::<NewFileToProcess, Receipt>(stream);
    let to_client = Arc::new(Mutex::new(to_client));

    while let Some(msg) = from_client.try_next().await? {
        println!("Server got: {msg:?}");
        tokio::spawn(processing_pipeline(msg, to_client.clone(), config.clone()));
    }
    Ok(())
}

pub(crate) async fn main(config: Config) -> io::Result<()> {
    let config = Arc::new(config);

    let listener = TcpListener::bind(&config.address).await?;

    println!("Server listening on {:?}", listener.local_addr());

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("Server got connection request from {addr:?}");
        tokio::spawn(handle_client(socket, config.clone()));
    }
}
