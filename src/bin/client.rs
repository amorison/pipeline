use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use pipeline::{NewFileToProcess, Received};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() {
    let stream = TcpStream::connect("127.0.0.1:12345").await.unwrap();

    let (mut from_server, mut to_server) =
        pipeline::framed_json_channel::<Received, NewFileToProcess>(stream);

    let nfp1 = NewFileToProcess::new("/home/dummy/data.txt");
    let nfp2 = NewFileToProcess::new("/home/dummy/data3.txt");

    to_server.send(nfp1).await.unwrap();
    to_server.send(nfp2).await.unwrap();

    while let Some(msg) = from_server.try_next().await.unwrap() {
        println!("Client got: {msg:?}");
    }
}
