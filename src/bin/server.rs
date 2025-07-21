use std::sync::Arc;

use futures_util::TryStreamExt;
use pipeline::{NewFileToProcess, Received};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_serde::formats::SymmetricalJson;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

async fn handle_client(socket: TcpStream) {
    let (socket_r, socket_w) = socket.into_split();

    let length_limited = FramedRead::new(socket_r, LengthDelimitedCodec::new());

    let mut deserialized = tokio_serde::SymmetricallyFramed::new(
        length_limited,
        SymmetricalJson::<NewFileToProcess>::default(),
    );

    let ll = FramedWrite::new(socket_w, LengthDelimitedCodec::new());
    let server_to_client = {
        let serialized =
            tokio_serde::SymmetricallyFramed::new(ll, SymmetricalJson::<Received>::default());
        Arc::new(Mutex::new(serialized))
    };

    while let Some(msg) = deserialized.try_next().await.unwrap() {
        println!("Server got: {:?}", msg);
        let s2c = server_to_client.clone();
        tokio::spawn(async move { pipeline::processing_pipeline(msg, s2c).await });
    }
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:12345").await.unwrap();

    println!("Server listening on {:?}", listener.local_addr());

    loop {
        let (socket, addr) = listener.accept().await.unwrap();
        println!("Server got connection request from {:?}", addr);
        tokio::spawn(handle_client(socket));
    }
}
