mod database;

use std::{io, path::PathBuf, sync::Arc};

use crate::{FileSpec, Receipt, WriteFramedJson, file_hash, replace_os_strings};
use database::{Database, ProcessStatus};
use futures_util::{SinkExt, TryStreamExt};
use log::{info, warn};
use serde::Deserialize;
use tokio::{
    net::{TcpListener, TcpStream},
    process::Command,
    sync::Mutex,
};

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    address: String,
    incoming_directory: PathBuf,
    processing: Vec<String>,
}

pub(crate) static DEFAULT_TOML_CONF: &str = include_str!("default_server.toml");

async fn processing_pipeline(
    file: FileSpec,
    channel: Arc<Mutex<WriteFramedJson<Receipt>>>,
    config: Arc<Config>,
    db: Database,
) {
    let mut server_path = config.incoming_directory.clone();
    server_path.push(file.server_filename());

    let in_db = db
        .contains(&file.sha256_digest)
        .await
        .expect("failed to read in db");

    let receipt = if in_db {
        Receipt::Received(file.clone())
    } else {
        match file_hash(&server_path) {
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
        }
    };
    let continue_processing = receipt.continue_processing();
    channel.lock().await.send(receipt).await.unwrap();
    if in_db || !continue_processing {
        return;
    }

    info!("starting processing for {file:?}");

    let mut processing = Command::new(&config.processing[0])
        .args(config.processing[1..].iter().map(|a| {
            replace_os_strings(
                a,
                [
                    ("{hash}", file.sha256_digest.as_ref()),
                    ("{server_path}", server_path.as_os_str()),
                    ("{client_file_stem}", file.client_path.file_stem().unwrap()),
                ]
                .into_iter(),
            )
        }))
        .spawn()
        .expect("could not spawn `copy_to_server` command");

    db.insert_new_processing(&file)
        .await
        .expect("failed to insert in db");

    let status = match processing.wait().await {
        Ok(status) if status.success() => {
            info!("processing of {file:?} completed successfully");
            ProcessStatus::Done
        }
        Ok(status) => {
            warn!(
                "processing of {file:?} failed with code {:?}",
                status.code()
            );
            ProcessStatus::Failed
        }
        Err(err) => {
            warn!("processing of {file:?} failed: '{err}'");
            ProcessStatus::Failed
        }
    };

    db.update_status(&file.sha256_digest, status)
        .await
        .expect("failed to insert in db");
}

async fn handle_client(stream: TcpStream, config: Arc<Config>, db: Database) -> io::Result<()> {
    let (mut from_client, to_client) = crate::framed_json_channel::<FileSpec, Receipt>(stream);
    let to_client = Arc::new(Mutex::new(to_client));

    while let Some(msg) = from_client.try_next().await? {
        info!("received request {msg:?}");
        tokio::spawn(processing_pipeline(
            msg,
            to_client.clone(),
            config.clone(),
            db.clone(),
        ));
    }
    Ok(())
}

pub(crate) async fn main(config: Config) -> io::Result<()> {
    let config = Arc::new(config);

    let db = Database::create_if_missing()
        .await
        .expect("failed to create database");

    let listener = TcpListener::bind(&config.address).await?;

    info!("listening on {:?}", listener.local_addr());

    loop {
        let (socket, addr) = listener.accept().await?;
        info!("got connection request from {addr:?}");
        tokio::spawn(handle_client(socket, config.clone(), db.clone()));
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn read_default_config() {
        assert!(toml::from_slice::<Config>(DEFAULT_TOML_CONF.as_bytes()).is_ok());
    }
}
