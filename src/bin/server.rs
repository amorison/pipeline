use std::sync::Arc;

use futures_util::TryStreamExt;
use pipeline::{NewFileToProcess, Received};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};

async fn handle_client(stream: TcpStream) {
    let (mut from_client, to_client) =
        pipeline::framed_json_channel::<NewFileToProcess, Received>(stream);
    let to_client = Arc::new(Mutex::new(to_client));

    while let Some(msg) = from_client.try_next().await.unwrap() {
        println!("Server got: {msg:?}");
        tokio::spawn(pipeline::processing_pipeline(msg, to_client.clone()));
    }
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:12345").await.unwrap();

    println!("Server listening on {:?}", listener.local_addr());

    loop {
        let (socket, addr) = listener.accept().await.unwrap();
        println!("Server got connection request from {addr:?}");
        tokio::spawn(handle_client(socket));
    }
}
