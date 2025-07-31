use std::{io, path::PathBuf, sync::Arc};

use crate::{FileSpec, Receipt, WriteFramedJson, file_hash, replace_os_strings};
use futures_util::{SinkExt, TryStreamExt};
use serde::Deserialize;
use sqlx::{Pool, Sqlite, SqlitePool, sqlite::SqliteConnectOptions};
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

pub(crate) static DEFAULT_TOML_CONF: &'static str = include_str!("default_server.toml");

async fn processing_pipeline(
    file: FileSpec,
    channel: Arc<Mutex<WriteFramedJson<Receipt>>>,
    config: Arc<Config>,
    db: Pool<Sqlite>,
) {
    let mut server_path = config.incoming_directory.clone();
    server_path.push(file.server_filename());

    let in_db: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM files_in_pipeline WHERE hash = $1);")
            .bind(&file.sha256_digest)
            .fetch_one(&db)
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

    let mut processing = Command::new(&config.processing[0])
        .args(config.processing[1..].iter().map(|a| {
            replace_os_strings(
                a,
                [
                    ("{server_path}", server_path.as_os_str()),
                    ("{client_file_stem}", file.client_path.file_stem().unwrap()),
                    ("{hash}", file.sha256_digest.as_ref()),
                ]
                .into_iter(),
            )
        }))
        .spawn()
        .expect("could not spawn `copy_to_server` command");

    sqlx::query("INSERT INTO files_in_pipeline (hash, date_utc, client_path, status) VALUES ($1, datetime('now'), $2, 'Processing');")
        .bind(&file.sha256_digest)
        .bind(file.client_path.as_os_str().as_encoded_bytes())
        .execute(&db)
        .await
        .expect("failed to insert in db");

    let status = match processing.wait().await {
        Ok(status) if status.success() => "Done",
        Ok(_) | Err(_) => "Failed",
    };

    sqlx::query(
        "UPDATE files_in_pipeline SET date_utc = datetime('now'), status = $2 WHERE hash = $1;",
    )
    .bind(&file.sha256_digest)
    .bind(status)
    .execute(&db)
    .await
    .expect("failed to insert in db");
}

async fn handle_client(stream: TcpStream, config: Arc<Config>, db: Pool<Sqlite>) -> io::Result<()> {
    let (mut from_client, to_client) = crate::framed_json_channel::<FileSpec, Receipt>(stream);
    let to_client = Arc::new(Mutex::new(to_client));

    while let Some(msg) = from_client.try_next().await? {
        println!("Server got: {msg:?}");
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

    let db = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(".pipeline_server.db")
            .create_if_missing(true),
    )
    .await
    .expect("Connection to db failed");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS files_in_pipeline (
        hash TEXT PRIMARY KEY,
        date_utc TEXT NOT NULL,
        client_path BLOB NOT NULL,
        status TEXT NOT NULL
    ) STRICT;",
    )
    .execute(&db)
    .await
    .expect("Failed to create table in db");

    let listener = TcpListener::bind(&config.address).await?;

    println!("Server listening on {:?}", listener.local_addr());

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("Server got connection request from {addr:?}");
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
