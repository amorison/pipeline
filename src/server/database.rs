use sqlx::{Pool, Result, Sqlite, SqlitePool, sqlite::SqliteConnectOptions};

use crate::FileSpec;

#[derive(Copy, Clone)]
pub(super) enum ProcessStatus {
    Processing,
    Failed,
    Done,
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
pub(crate) struct Database(Pool<Sqlite>);

impl Database {
    pub(super) async fn create_if_missing() -> Result<Self> {
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(".pipeline_server.db")
                .create_if_missing(true),
        )
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS files_in_pipeline (
                hash TEXT PRIMARY KEY,
                date_utc TEXT NOT NULL,
                file_name TEXT NOT NULL,
                status TEXT NOT NULL
            ) STRICT;",
        )
        .execute(&pool)
        .await?;

        Ok(Self(pool))
    }

    pub(super) async fn contains(&self, hash: &str) -> Result<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM files_in_pipeline WHERE hash = $1);")
            .bind(hash)
            .fetch_one(&self.0)
            .await
    }

    pub(super) async fn insert_new_processing(&self, file: &FileSpec) -> Result<()> {
        sqlx::query("INSERT INTO files_in_pipeline (hash, date_utc, file_name, status) VALUES ($1, datetime('now'), $2, $3);")
            .bind(&file.sha256_digest)
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
}
