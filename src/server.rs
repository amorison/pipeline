use std::{io, path::PathBuf, sync::Arc};

use crate::{FileSpec, Receipt, WriteFramedJson, file_hash, replace_os_strings};
use futures_util::{SinkExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio::{
    net::{TcpListener, TcpStream},
    process::Command,
    sync::Mutex,
};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
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
            processing: ["cp", "{server_path}", "./server/{client_file_stem}.tiff"]
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
        }
    }
}

async fn processing_pipeline(
    file: FileSpec,
    channel: Arc<Mutex<WriteFramedJson<Receipt>>>,
    config: Arc<Config>,
) {
    let mut server_path = config.incoming_directory.clone();
    server_path.push(file.server_filename());

    let receipt = match file_hash(&server_path) {
        Ok(received_hash) => {
            if file.sha256_digest == received_hash {
                Receipt::Received(file.clone())
            } else {
                Receipt::DifferentHash {
                    spec: file.clone(),
                    received_hash,
                }
            }
        }
        Err(err) => Receipt::Error {
            spec: file.clone(),
            error: err.to_string(),
        },
    };
    let continue_processing = receipt.continue_processing();
    channel.lock().await.send(receipt).await.unwrap();
    if !continue_processing {
        return;
    }

    let mut processing = Command::new(&config.processing[0])
        .args(config.processing[1..].iter().map(|a| {
            replace_os_strings(
                a,
                [
                    ("{server_path}", server_path.as_os_str()),
                    ("{client_file_stem}", file.client_path.file_stem().unwrap()),
                ]
                .into_iter(),
            )
        }))
        .spawn()
        .expect("could not spawn `copy_to_server` command");
    processing.wait().await.unwrap();
}

async fn handle_client(stream: TcpStream, config: Arc<Config>) -> io::Result<()> {
    let (mut from_client, to_client) = crate::framed_json_channel::<FileSpec, Receipt>(stream);
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
