use log::warn;
use tokio::io;

use crate::{cli::MarkStatus, server::Database};

pub(in crate::server) async fn process_request(
    db: Database,
    hash: String,
    status: MarkStatus,
) -> io::Result<()> {
    while let Err(err) = db.update_status(&hash, status.into()).await {
        warn!("error updating status for {hash}: {err}");
    }
    Ok(())
}
