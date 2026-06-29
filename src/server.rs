pub(crate) mod clean;
pub(crate) mod create_buckets;
pub(crate) mod database;
mod processing;
pub(crate) mod query;

use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::{
    FileSpec, Receipt, assemble_path, custom_serde,
    framed_io::{Splittable, WriteFramedJson, json_channel},
    handshake::{self, ClientKind, HandshakeOutcome},
    hashing::FileDigest,
    server::clean::clean_tasks_with_status,
};
use database::{Database, ProcessStatus};
use futures_util::{SinkExt, TryStreamExt};
use log::{debug, error, info, warn};
use serde::Deserialize;
use tokio::{
    io::AsyncReadExt,
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::{Mutex, Semaphore},
    time::MissedTickBehavior,
};

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    incoming_directory: PathBuf,
    unix_mode: Option<u32>,
    #[serde(deserialize_with = "custom_serde::map_at_least_one")]
    processing: HashMap<String, ProcessingGroup>,
    retry_tasks_every_secs: u64,
    prune_every_secs: u64,
    server: ServerAddress,
    concurrency: Concurrency,
    database: DatabaseConfig,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub(crate) struct ServerAddress {
    address: String,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
struct ProcessingGroup {
    processing: processing::Processing,
    after_processing: processing::AfterProcessing,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
struct Concurrency {
    max_hashes: usize,
    max_processing: usize,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
struct DatabaseConfig {
    wal: bool,
}

impl Config {
    fn incoming_path<P: AsRef<Path>>(&self, relative: P) -> PathBuf {
        assemble_path(&self.incoming_directory, relative)
    }

    pub(crate) fn path_of(&self, file: &FileSpec) -> PathBuf {
        let rel_path = rel_path(file, self);
        self.incoming_path(rel_path)
    }

    pub(crate) async fn create_dir_async(&self, path: impl AsRef<Path>) -> io::Result<()> {
        use tokio::fs;

        fs::create_dir_all(&path).await?;

        #[cfg(unix)]
        if let Some(mode) = self.unix_mode {
            use std::{fs::Permissions, os::unix::fs::PermissionsExt};

            fs::set_permissions(&path, Permissions::from_mode(mode)).await?;
        }

        Ok(())
    }

    pub(crate) fn create_dir_sync(&self, path: impl AsRef<Path>) -> io::Result<()> {
        use std::fs;

        fs::create_dir_all(&path)?;

        #[cfg(unix)]
        if let Some(mode) = self.unix_mode {
            use std::{fs::Permissions, os::unix::fs::PermissionsExt};

            fs::set_permissions(&path, Permissions::from_mode(mode))?;
        }

        Ok(())
    }

    pub(crate) fn is_proc_group(&self, name: &str) -> bool {
        self.processing.contains_key(name)
    }
}

pub(crate) static DEFAULT_TOML_CONF: &str = include_str!("server/default.toml");

fn rel_path(spec: &FileSpec, config: &Config) -> String {
    let hash = spec.hash();
    let bucket = hash[0..2].to_owned() + "/" + &hash[2..4];
    match config.create_dir_sync(config.incoming_path(&bucket)) {
        Ok(_) => bucket + "/" + hash,
        Err(err) => {
            warn!("failed to create {bucket}: {err}");
            hash.to_owned()
        }
    }
}

async fn processing_pipeline<W: AsyncWriteExt + Unpin>(
    file: FileSpec,
    channel: Arc<Mutex<WriteFramedJson<Receipt, W>>>,
    config: Arc<Config>,
    db: Database,
    sem_hash: Arc<Semaphore>,
    sem_proc: Arc<Semaphore>,
) {
    let server_path = config.path_of(&file);

    let in_db = loop {
        match db.contains(file.hash()).await {
            Ok(in_db) => break in_db,
            Err(err) => warn!("failed to check if {file:?} is in database: {err}"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };

    let await_first_arrival = if in_db {
        let status = loop {
            match db.status(file.hash()).await {
                Ok(status) => break status,
                Err(err) => warn!("failed to check status of {file:?} in db: {err}"),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        };
        matches!(status, ProcessStatus::AwaitFromClient)
    } else {
        false
    };

    let receipt = if in_db && !await_first_arrival {
        Receipt::Received(file.clone())
    } else if in_db {
        let hash = {
            let _permit = sem_hash.acquire().await.unwrap();
            FileDigest::with_spec(&server_path, &file)
        };
        match hash {
            Ok(received_hash) => {
                if file.sha256_digest == received_hash {
                    debug!("{file:?} found");
                    Receipt::Received(file.clone())
                } else {
                    warn!(
                        "{file:?} does not have expected hash, got {}",
                        received_hash.hash()
                    );
                    Receipt::DifferentHash(file.clone())
                }
            }
            Err(err) => {
                warn!("{file:?} not found {err:?}");
                Receipt::Error {
                    spec: file.clone(),
                    server_rel_path: rel_path(&file, &config),
                    error: err.to_string(),
                }
            }
        }
    } else {
        while let Err(err) = db.insert_new(&file).await {
            warn!("failed to insert {file:?} in db: {err}");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Receipt::Expecting {
            spec: file.clone(),
            server_rel_path: rel_path(&file, &config),
        }
    };

    let continue_processing = receipt.continue_processing();
    channel.lock().await.send(receipt).await.unwrap();
    if !continue_processing {
        return;
    }

    let permit_proc = sem_proc.acquire().await.unwrap();
    process_file(file, config, db).await;
    drop(permit_proc);
}

async fn process_file(file: FileSpec, config: Arc<Config>, db: Database) {
    let status = loop {
        match db.status(file.hash()).await {
            Ok(status) => break status,
            Err(err) => warn!("failed to check status of {file:?} in db: {err}"),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    if matches!(status, ProcessStatus::Processing) {
        debug!("{file:?} is already being processed");
        return;
    }

    info!("starting processing for {file:?}");

    while let Err(err) = db
        .update_status(file.hash(), ProcessStatus::Processing)
        .await
    {
        warn!("failed to update status of {file:?} in db: {err}");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let Some(proc_group) = config.processing.get(&file.processing) else {
        // When establishing a connection with client, the handshake verifies that all processing
        // groups in the client config are known by the server so this is unreachable.
        error!(
            "unknown group name {}. This should have been caught during the handshake, please raise a bug report",
            file.processing,
        );
        return;
    };

    let status = match proc_group.processing.run(&file, &config).await {
        Ok(()) => {
            info!("processing of {file:?} completed successfully");
            proc_group.after_processing.run(&file, &config, &db).await
        }
        Err(err) => {
            warn!("processing of {file:?} failed: '{err}'");
            Some(ProcessStatus::Failed)
        }
    };

    if let Some(status) = status {
        debug!("marking {file:?} as {status:?}");
        while let Err(err) = db.update_status(file.hash(), status).await {
            warn!("failed to update status of {file:?} in db: {err}");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

async fn listen_to_processing_client<R, W, S>(
    stream: S,
    addr: SocketAddr,
    config: Arc<Config>,
    db: Database,
    sem_hash: Arc<Semaphore>,
    sem_proc: Arc<Semaphore>,
) -> io::Result<()>
where
    S: Splittable<R, W>,
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin + Send + 'static,
{
    let (mut from_client, to_client) = json_channel::<FileSpec, Receipt, _, _, _>(stream);
    let to_client = Arc::new(Mutex::new(to_client));

    while let Some(msg) = from_client.try_next().await? {
        debug!("received request from {addr:?}: {msg:?}");
        tokio::spawn(processing_pipeline(
            msg,
            to_client.clone(),
            config.clone(),
            db.clone(),
            sem_hash.clone(),
            sem_proc.clone(),
        ));
    }

    info!("client {addr:?} closed connection");
    Ok(())
}

async fn handle_client(
    mut stream: TcpStream,
    addr: SocketAddr,
    config: Arc<Config>,
    db: Database,
    sem_hash: Arc<Semaphore>,
    sem_proc: Arc<Semaphore>,
) -> io::Result<()> {
    debug!("got connection request from {addr:?}");

    match handshake::server_side(&mut stream, &config).await {
        Ok(HandshakeOutcome::Success(ClientKind::Processing)) => {
            info!("handshake with processing client {addr:?} was successful");
            listen_to_processing_client(stream, addr, config, db, sem_hash, sem_proc).await
        }
        Ok(HandshakeOutcome::Success(ClientKind::Mark { hash, status })) => {
            info!("received mark request from {addr:?}");
            query::process_mark_query(db, hash, status).await
        }
        Ok(HandshakeOutcome::Success(ClientKind::List)) => {
            info!("received list request from {addr:?}");
            query::process_list_query(stream, db).await
        }
        Ok(HandshakeOutcome::Success(ClientKind::PruneDone)) => {
            info!("received request to prune 'done' tasks from {addr:?}");
            query::process_prune_done_query(db).await
        }
        Ok(HandshakeOutcome::Success(ClientKind::Status)) => {
            info!("received status request from {addr:?}");
            Ok(())
        }
        Ok(HandshakeOutcome::Denied) => {
            warn!("handshake with {addr:?} was not successful, closing connection");
            _ = stream.shutdown().await;
            Ok(())
        }
        Ok(HandshakeOutcome::ClosedConnection) => {
            info!("client {addr:?} closed connection");
            Ok(())
        }
        Err(err) => {
            warn!("error reading from {addr:?}: {err}");
            stream.write_all(b"pipeline: invalid request\n").await
        }
    }
}

async fn listen_to_clients(config: Arc<Config>, db: Database) -> io::Result<()> {
    let listener = TcpListener::bind(&config.server.address).await?;
    let sem_hash = Arc::new(Semaphore::new(config.concurrency.max_hashes));
    let sem_proc = Arc::new(Semaphore::new(config.concurrency.max_processing));

    info!("listening on {:?}", listener.local_addr());

    loop {
        let (socket, addr) = listener.accept().await?;
        tokio::spawn(handle_client(
            socket,
            addr,
            config.clone(),
            db.clone(),
            sem_hash.clone(),
            sem_proc.clone(),
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

async fn prune_tasks(config: Arc<Config>, db: Database) -> io::Result<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(config.prune_every_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let summary =
            clean_tasks_with_status(config.clone(), db.clone(), ProcessStatus::ToPrune).await;
        debug!("{summary}");
    }
}

pub(crate) async fn main(config: Config) -> io::Result<()> {
    let config = Arc::new(config);

    let db = Database::create_if_missing(config.database.wal)
        .await
        .expect("failed to create database");

    tokio::select!(
        listen = listen_to_clients(config.clone(), db.clone()) => listen,
        retry = restart_failed_tasks(config.clone(), db.clone()) => retry,
        prune = prune_tasks(config, db) => prune,
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
