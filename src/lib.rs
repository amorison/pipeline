use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::Mutex,
};
use tokio_serde::{SymmetricallyFramed, formats::SymmetricalJson};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

#[derive(Serialize, Deserialize, Debug)]
pub struct FileSpec {
    remote_path: PathBuf,
    sha256_digest: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewFileToProcess(FileSpec);

impl NewFileToProcess {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        NewFileToProcess(FileSpec {
            remote_path: PathBuf::from(path.as_ref()),
            sha256_digest: "000".to_owned(),
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Received(FileSpec);

impl From<NewFileToProcess> for Received {
    fn from(value: NewFileToProcess) -> Self {
        let NewFileToProcess(spec) = value;
        Received(spec)
    }
}

pub type ReadFramedJson<T> =
    SymmetricallyFramed<FramedRead<OwnedReadHalf, LengthDelimitedCodec>, T, SymmetricalJson<T>>;

pub type WriteFramedJson<T> =
    SymmetricallyFramed<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>, T, SymmetricalJson<T>>;

pub fn server_side_channel(
    stream: TcpStream,
) -> (ReadFramedJson<NewFileToProcess>, WriteFramedJson<Received>) {
    let (socket_r, socket_w) = stream.into_split();
    let read_half = tokio_serde::SymmetricallyFramed::new(
        FramedRead::new(socket_r, LengthDelimitedCodec::new()),
        SymmetricalJson::<NewFileToProcess>::default(),
    );
    let write_half = tokio_serde::SymmetricallyFramed::new(
        FramedWrite::new(socket_w, LengthDelimitedCodec::new()),
        SymmetricalJson::<Received>::default(),
    );
    (read_half, write_half)
}

pub async fn processing_pipeline(
    file: NewFileToProcess,
    channel: Arc<Mutex<WriteFramedJson<Received>>>,
) {
    let received: Received = file.into();
    channel.lock().await.send(received).await.unwrap();
}
