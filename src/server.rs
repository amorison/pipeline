use std::{io, sync::Arc};

use crate::{NewFileToProcess, Receipt};
use futures_util::TryStreamExt;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

async fn handle_client(stream: TcpStream) -> io::Result<()> {
    let (mut from_client, to_client) =
        crate::framed_json_channel::<NewFileToProcess, Receipt>(stream);
    let to_client = Arc::new(Mutex::new(to_client));

    while let Some(msg) = from_client.try_next().await? {
        println!("Server got: {msg:?}");
        tokio::spawn(crate::processing_pipeline(msg, to_client.clone()));
    }
    Ok(())
}

pub async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:12345").await?;

    println!("Server listening on {:?}", listener.local_addr());

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("Server got connection request from {addr:?}");
        tokio::spawn(handle_client(socket));
    }
}
