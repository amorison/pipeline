use log::warn;
use tokio::{io, net::TcpStream};

use crate::{
    cli::MarkStatus,
    handshake::{self, RequestPayload},
    server::{Config, Database},
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

pub(crate) async fn main(config: Config, hash: String, status: MarkStatus) -> io::Result<()> {
    let addr = &config.address;
    let mut stream = TcpStream::connect(addr).await?;

    let payload = RequestPayload::Mark { hash, status };
    if !handshake::client_side(&mut stream, payload).await? {
        eprintln!("mark request to {} failed", addr);
        return Ok(());
    }
    eprintln!("mark request accepted by {}", addr);
    Ok(())
}
