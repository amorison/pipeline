use futures_util::TryStreamExt;
use futures_util::sink::SinkExt;
use pipeline::{NewFileToProcess, Received};
use tokio::net::TcpStream;
use tokio_serde::formats::SymmetricalJson;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

#[tokio::main]
async fn main() {
    let mut socket = TcpStream::connect("127.0.0.1:12345").await.unwrap();

    let (socket_r, socket_w) = socket.split();

    let length_delimited = FramedWrite::new(socket_w, LengthDelimitedCodec::new());
    let mut serialized = tokio_serde::SymmetricallyFramed::new(
        length_delimited,
        SymmetricalJson::<NewFileToProcess>::default(),
    );

    let ll = FramedRead::new(socket_r, LengthDelimitedCodec::new());
    let mut deserialized =
        tokio_serde::SymmetricallyFramed::new(ll, SymmetricalJson::<Received>::default());

    let nfp1 = NewFileToProcess::new("/home/dummy/data.txt");
    let nfp2 = NewFileToProcess::new("/home/dummy/data3.txt");

    serialized.send(nfp1).await.unwrap();
    serialized.send(nfp2).await.unwrap();

    while let Some(msg) = deserialized.try_next().await.unwrap() {
        println!("Client got: {:?}", msg);
    }
}
