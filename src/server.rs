pub(crate) mod database;
pub(crate) mod list;
pub(crate) mod prune;

use std::{io, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use crate::{FileSpec, Receipt, WriteFramedJson, file_hash, replace_os_strings};
use database::{Database, ProcessStatus};
use futures_util::{SinkExt, TryStreamExt};
use log::{debug, info, warn};
use serde::Deserialize;
use tokio::{
    net::{TcpListener, TcpStream},
    process::Command,
    sync::{Mutex, Semaphore},
    time::MissedTickBehavior,
};

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    address: String,
    incoming_directory: PathBuf,
    processing: Vec<String>,
    retry_tasks_every_secs: u64,
    max_concurrent_hashes: usize,
}

impl Config {
    pub(crate) fn path_of(&self, file: &FileSpec) -> PathBuf {
        let mut path = self.incoming_directory.clone();
        path.push(file.server_filename());
        path
    }
}

pub(crate) static DEFAULT_TOML_CONF: &str = include_str!("server/default.toml");

async fn processing_pipeline(
    file: FileSpec,
    channel: Arc<Mutex<WriteFramedJson<Receipt>>>,
    config: Arc<Config>,
    db: Database,
    semaphore: Arc<Semaphore>,
) {
    let server_path = config.path_of(&file);

    let in_db = loop {
        match db.contains(&file.sha256_digest).await {
            Ok(in_db) => break in_db,
            Err(err) => warn!("failed to check if {file:?} is in database: {err}"),
        }
    };

    let receipt = if in_db {
        Receipt::Received(file.clone())
    } else {
        let hash = {
            let _permit = semaphore.acquire().await.unwrap();
            file_hash(&server_path)
        };
        match hash {
            Ok(received_hash) => {
                if file.sha256_digest == received_hash {
                    info!("{file:?} found");
                    Receipt::Received(file.clone())
                } else {
                    warn!("{file:?} does not have expected hash");
                    Receipt::DifferentHash {
                        spec: file.clone(),
                        received_hash,
                    }
                }
            }
            Err(err) => {
                info!("{file:?} not found {err:?}");
                Receipt::Error {
                    spec: file.clone(),
                    error: err.to_string(),
                }
            }
        }
    };
    let continue_processing = receipt.continue_processing();
    channel.lock().await.send(receipt).await.unwrap();
    if in_db || !continue_processing {
        return;
    }

    while let Err(err) = db.insert_new_processing(&file).await {
        warn!("failed to insert {file:?} in db: {err}");
    }

    process_file(file, config, db).await;
}

async fn process_file(file: FileSpec, config: Arc<Config>, db: Database) {
    info!("starting processing for {file:?}");

    while let Err(err) = db
        .update_status(&file.sha256_digest, ProcessStatus::Processing)
        .await
    {
        warn!("failed to update status of {file:?} in db: {err}");
    }

    let server_path = config.path_of(&file);

    let mut processing = Command::new(&config.processing[0])
        .args(config.processing[1..].iter().map(|a| {
            replace_os_strings(
                a,
                [
                    ("{hash}", file.sha256_digest.as_ref()),
                    ("{server_path}", server_path.as_os_str()),
                    ("{client_name}", file.client.as_ref()),
                    (
                        "{client_relative_directory}",
                        file.relative_directory().as_os_str(),
                    ),
                    ("{client_file_stem}", file.file_stem()),
                ]
                .into_iter(),
            )
        }))
        .spawn()
        .expect("could not spawn `copy_to_server` command");

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

    while let Err(err) = db.update_status(&file.sha256_digest, status).await {
        warn!("failed to update status of {file:?} in db: {err}");
    }
}

async fn handle_client(
    stream: TcpStream,
    addr: SocketAddr,
    config: Arc<Config>,
    db: Database,
    semaphore: Arc<Semaphore>,
) -> io::Result<()> {
    info!("got connection request from {addr:?}");

    let (mut from_client, to_client) = crate::framed_json_channel::<FileSpec, Receipt>(stream);
    let to_client = Arc::new(Mutex::new(to_client));

    while let Some(msg) = from_client.try_next().await? {
        info!("received request from {addr:?}: {msg:?}");
        tokio::spawn(processing_pipeline(
            msg,
            to_client.clone(),
            config.clone(),
            db.clone(),
            semaphore.clone(),
        ));
    }

    info!("client {addr:?} closed connection");
    Ok(())
}

async fn listen_to_clients(config: Arc<Config>, db: Database) -> io::Result<()> {
    let listener = TcpListener::bind(&config.address).await?;
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_hashes));

    info!("listening on {:?}", listener.local_addr());

    loop {
        let (socket, addr) = listener.accept().await?;
        tokio::spawn(handle_client(
            socket,
            addr,
            config.clone(),
            db.clone(),
            semaphore.clone(),
        ));
    }
}

async fn restart_failed_tasks(config: Arc<Config>, db: Database) -> io::Result<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(config.retry_tasks_every_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        debug!("looking for failed tasks to restart");
        let failed = db.tasks_with_status(ProcessStatus::Failed).await;
        match failed {
            Ok(failed) => {
                for spec in failed.into_iter().map(FileSpec::from) {
                    info!("restarting previously failed {spec:?}");
                    tokio::spawn(process_file(spec, config.clone(), db.clone()));
                }
            }
            Err(err) => {
                warn!("failed to read database for failed tasks: {err}");
            }
        }
    }
}

pub(crate) async fn main(config: Config) -> io::Result<()> {
    let config = Arc::new(config);

    let db = Database::create_if_missing()
        .await
        .expect("failed to create database");

    tokio::select!(
        listen = listen_to_clients(config.clone(), db.clone()) => listen,
        retry = restart_failed_tasks(config, db) => retry,
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn read_default_config() {
        assert!(toml::from_slice::<Config>(DEFAULT_TOML_CONF.as_bytes()).is_ok());
    }
}
