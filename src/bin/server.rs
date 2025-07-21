use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use pipeline::{NewFileToProcess, Received};
use tokio::net::TcpListener;
use tokio_serde::formats::SymmetricalJson;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:12345").await.unwrap();

    println!("Server listening on {:?}", listener.local_addr());

    loop {
        let (socket, addr) = listener.accept().await.unwrap();
        println!("Server got request from {:?}", addr);

        let (socket_r, socket_w) = socket.into_split();

        let length_limited = FramedRead::new(socket_r, LengthDelimitedCodec::new());

        let mut deserialized = tokio_serde::SymmetricallyFramed::new(
            length_limited,
            SymmetricalJson::<NewFileToProcess>::default(),
        );

        let ll = FramedWrite::new(socket_w, LengthDelimitedCodec::new());
        let mut serialized =
            tokio_serde::SymmetricallyFramed::new(ll, SymmetricalJson::<Received>::default());

        tokio::spawn(async move {
            while let Some(msg) = deserialized.try_next().await.unwrap() {
                println!("Server got: {:?}", msg);
                let received: Received = msg.into();
                serialized.send(received).await.unwrap();
            }
        });
    }
}
