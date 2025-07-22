use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
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
    client_path: PathBuf,
    server_path: PathBuf,
    sha256_digest: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewFileToProcess(FileSpec);

impl NewFileToProcess {
    pub fn new(client_path: PathBuf, server_path: PathBuf) -> io::Result<Self> {
        let mut hasher = Sha256::new();
        // FIXME: check whether it is worthwhile to make this non-blocking
        let file = std::fs::File::open(&client_path)?;
        let mut reader = io::BufReader::new(file);
        io::copy(&mut reader, &mut hasher)?;

        let nfp = NewFileToProcess(FileSpec {
            client_path,
            server_path,
            sha256_digest: hasher.finalize().to_vec(),
        });
        Ok(nfp)
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

impl Received {
    pub fn path(&self) -> &Path {
        &self.0.server_path
    }
}

pub type ReadFramedJson<T> =
    SymmetricallyFramed<FramedRead<OwnedReadHalf, LengthDelimitedCodec>, T, SymmetricalJson<T>>;

pub type WriteFramedJson<T> =
    SymmetricallyFramed<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>, T, SymmetricalJson<T>>;

pub fn framed_json_channel<R, W>(stream: TcpStream) -> (ReadFramedJson<R>, WriteFramedJson<W>) {
    let (socket_r, socket_w) = stream.into_split();
    let read_half = tokio_serde::SymmetricallyFramed::new(
        FramedRead::new(socket_r, LengthDelimitedCodec::new()),
        SymmetricalJson::<R>::default(),
    );
    let write_half = tokio_serde::SymmetricallyFramed::new(
        FramedWrite::new(socket_w, LengthDelimitedCodec::new()),
        SymmetricalJson::<W>::default(),
    );
    (read_half, write_half)
}

pub async fn processing_pipeline(
    file: NewFileToProcess,
    channel: Arc<Mutex<WriteFramedJson<Received>>>,
) {
    let received: Received = file.into();
    tokio::time::sleep(Duration::from_secs(2)).await; // to simulate processing
    channel.lock().await.send(received).await.unwrap();
}
