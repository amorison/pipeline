use std::time::Duration;

use sqlx::{
    ConnectOptions, Pool, Result, Sqlite, SqlitePool,
    prelude::{FromRow, Type},
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tabled::Tabled;

use crate::{FileSpec, cli::MarkStatus, hashing::FileDigest};

#[derive(Copy, Clone, Type, Debug)]
pub(super) enum ProcessStatus {
    AwaitFromClient,
    Processing,
    Failed,
    Done,
    ToPrune,
}

impl From<MarkStatus> for ProcessStatus {
    fn from(value: MarkStatus) -> Self {
        match value {
            MarkStatus::Done => ProcessStatus::Done,
            MarkStatus::Failed => ProcessStatus::Failed,
            MarkStatus::ToPrune => ProcessStatus::ToPrune,
        }
    }
}

#[derive(FromRow, Tabled)]
pub(super) struct FileInPipeline {
    hash: String,
    full_hash: bool,
    client: String,
    date_utc: String,
    path: String,
    file_name: String,
    #[tabled(format = "{:?}")]
    status: ProcessStatus,
}

impl From<FileInPipeline> for FileSpec {
    fn from(value: FileInPipeline) -> Self {
        let hash = value.hash;
        let sha256_digest = if value.full_hash {
            FileDigest::Full(hash)
        } else {
            FileDigest::Shallow(hash)
        };
        Self {
            client: value.client,
            path: value.path,
            filename: value.file_name,
            sha256_digest,
        }
    }
}

impl AsRef<str> for ProcessStatus {
    fn as_ref(&self) -> &str {
        match self {
            ProcessStatus::AwaitFromClient => "AwaitFromClient",
            ProcessStatus::Processing => "Processing",
            ProcessStatus::Failed => "Failed",
            ProcessStatus::Done => "Done",
            ProcessStatus::ToPrune => "ToPrune",
        }
    }
}

#[derive(Clone)]
pub(super) struct Database(Pool<Sqlite>);

impl Database {
    pub(super) async fn read_only() -> Result<Self> {
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(".pipeline_server.db")
                .read_only(true),
        )
        .await?;

        Ok(Self(pool))
    }

    pub(super) async fn create_if_missing() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_slow_threshold(Duration::from_secs(5))
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(".pipeline_server.db")
                    .log_slow_statements(log::LevelFilter::Warn, Duration::from_secs(5))
                    .create_if_missing(true),
            )
            .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS files_in_pipeline (
                hash TEXT PRIMARY KEY,
                full_hash INTEGER NOT NULL,
                client TEXT NOT NULL,
                date_utc TEXT NOT NULL,
                path TEXT NOT NULL,
                file_name TEXT NOT NULL,
                status TEXT NOT NULL
            ) STRICT;",
        )
        .execute(&pool)
        .await?;

        Ok(Self(pool))
    }

    pub(super) async fn content(&self) -> Result<Vec<FileInPipeline>> {
        sqlx::query_as("SELECT * FROM files_in_pipeline;")
            .fetch_all(&self.0)
            .await
    }

    pub(super) async fn tasks_with_status(
        &self,
        status: ProcessStatus,
    ) -> Result<Vec<FileInPipeline>> {
        sqlx::query_as("SELECT * FROM files_in_pipeline WHERE status = $1;")
            .bind(status.as_ref())
            .fetch_all(&self.0)
            .await
    }

    pub(super) async fn status(&self, hash: &str) -> Result<ProcessStatus> {
        sqlx::query_scalar("SELECT status FROM files_in_pipeline WHERE hash = $1;")
            .bind(hash)
            .fetch_one(&self.0)
            .await
    }

    pub(super) async fn contains(&self, hash: &str) -> Result<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM files_in_pipeline WHERE hash = $1);")
            .bind(hash)
            .fetch_one(&self.0)
            .await
    }

    pub(super) async fn insert_new(&self, file: &FileSpec) -> Result<()> {
        sqlx::query(
            "INSERT INTO files_in_pipeline
            (hash, full_hash, client, date_utc, path, file_name, status)
            VALUES ($1, $2, $3, datetime('now'), $4, $5, $6);",
        )
        .bind(file.hash())
        .bind(file.sha256_digest.is_full())
        .bind(&file.client)
        .bind(&file.path)
        .bind(&file.filename)
        .bind(ProcessStatus::AwaitFromClient.as_ref())
        .execute(&self.0)
        .await?;
        Ok(())
    }

    pub(super) async fn update_status(&self, hash: &str, status: ProcessStatus) -> Result<()> {
        sqlx::query(
            "UPDATE files_in_pipeline
            SET date_utc = datetime('now'), status = $2
            WHERE hash = $1;",
        )
        .bind(hash)
        .bind(status.as_ref())
        .execute(&self.0)
        .await?;
        Ok(())
    }

    pub(super) async fn remove(&self, hash: &str) -> Result<()> {
        sqlx::query("DELETE FROM files_in_pipeline WHERE hash = $1;")
            .bind(hash)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}
