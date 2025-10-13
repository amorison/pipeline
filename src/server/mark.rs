use log::warn;
use tokio::io;

use crate::{cli::MarkStatus, server::Database};

pub(crate) async fn main(hash: String, status: MarkStatus) -> io::Result<()> {
    let db = Database::create_if_missing()
        .await
        .expect("failed to create database");

    while let Err(err) = db.update_status(&hash, status.into()).await {
        warn!("error updating status for {hash}: {err}")
    }

    Ok(())
}
