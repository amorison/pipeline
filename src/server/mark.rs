use log::warn;
use tokio::io;

use crate::{
    cli::MarkStatus,
    handshake::{self, RequestPayload},
    server::Database,
    server_route::QueryConfig,
};

pub(super) async fn process_mark_request(
    db: Database,
    hash: String,
    status: MarkStatus,
) -> io::Result<()> {
    while let Err(err) = db.update_status(&hash, status.into()).await {
        warn!("error updating status for {hash}: {err}");
    }
    Ok(())
}

pub(crate) async fn main(config: QueryConfig, hash: String, status: MarkStatus) -> io::Result<()> {
    let mut stream = config.server.connect().await;

    let payload = RequestPayload::Mark { hash, status };
    if !handshake::client_side(&mut stream, payload).await? {
        eprintln!("mark request failed");
        return Ok(());
    }
    eprintln!("mark request accepted");
    Ok(())
}
