use std::time::Duration;

use sqlx::{
    Pool, Result, Sqlite, SqlitePool,
    prelude::{FromRow, Type},
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tabled::Tabled;

use crate::FileSpec;

#[derive(Copy, Clone, Type, Debug)]
pub(super) enum ProcessStatus {
    Processing,
    Failed,
    Done,
}

#[derive(FromRow, Tabled)]
pub(super) struct FileInPipeline {
    hash: String,
    client: String,
    date_utc: String,
    path: String,
    file_name: String,
    #[tabled(format = "{:?}")]
    status: ProcessStatus,
}

impl From<FileInPipeline> for FileSpec {
    fn from(value: FileInPipeline) -> Self {
        Self {
            client: value.client,
            path: value.path,
            filename: value.file_name,
            sha256_digest: value.hash,
        }
    }
}

impl AsRef<str> for ProcessStatus {
    fn as_ref(&self) -> &str {
        match self {
            ProcessStatus::Processing => "Processing",
            ProcessStatus::Failed => "Failed",
            ProcessStatus::Done => "Done",
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
                    .create_if_missing(true),
            )
            .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS files_in_pipeline (
                hash TEXT PRIMARY KEY,
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

    pub(super) async fn contains(&self, hash: &str) -> Result<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM files_in_pipeline WHERE hash = $1);")
            .bind(hash)
            .fetch_one(&self.0)
            .await
    }

    pub(super) async fn insert_new_processing(&self, file: &FileSpec) -> Result<()> {
        sqlx::query("INSERT INTO files_in_pipeline (hash, client, date_utc, path, file_name, status) VALUES ($1, $2, datetime('now'), $3, $4, $5);")
            .bind(&file.sha256_digest)
            .bind(&file.client)
            .bind(&file.path)
            .bind(&file.filename)
            .bind(ProcessStatus::Processing.as_ref())
            .execute(&self.0)
            .await?;
        Ok(())
    }

    pub(super) async fn update_status(&self, hash: &str, status: ProcessStatus) -> Result<()> {
        sqlx::query(
            "UPDATE files_in_pipeline SET date_utc = datetime('now'), status = $2 WHERE hash = $1;",
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
