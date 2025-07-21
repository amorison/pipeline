use std::time::Duration;

use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use pipeline::{NewFileToProcess, ReadFramedJson, Received};
use tokio::net::TcpStream;

async fn listen_to_server(mut from_server: ReadFramedJson<Received>) {
    while let Some(msg) = from_server.try_next().await.unwrap() {
        println!("Client got: {msg:?}");
    }
}

#[tokio::main]
async fn main() {
    let stream = TcpStream::connect("127.0.0.1:12345").await.unwrap();

    let (from_server, mut to_server) =
        pipeline::framed_json_channel::<Received, NewFileToProcess>(stream);

    tokio::spawn(listen_to_server(from_server));

    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let nfp = NewFileToProcess::new("/home/dummy/data.txt");
        to_server.send(nfp).await.unwrap();
    }
}
