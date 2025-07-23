use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io,
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

// FIXME: check whether it is worthwhile to make this non-blocking
fn file_hash(path: &Path) -> io::Result<Vec<u8>> {
    let mut hasher = Sha256::new();
    let file = std::fs::File::open(path)?;
    let mut reader = io::BufReader::new(file);
    io::copy(&mut reader, &mut hasher)?;
    Ok(hasher.finalize().to_vec())
}

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
        let sha256_digest = file_hash(&client_path)?;
        let nfp = NewFileToProcess(FileSpec {
            client_path,
            server_path,
            sha256_digest,
        });
        Ok(nfp)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Receipt {
    Received(FileSpec),
    DifferentHash {
        spec: FileSpec,
        received_hash: Vec<u8>,
    },
    Error(String),
}

impl FileSpec {
    pub fn client_path(&self) -> &Path {
        &self.client_path
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
    channel: Arc<Mutex<WriteFramedJson<Receipt>>>,
) {
    let NewFileToProcess(spec) = file;
    let receipt = match file_hash(&spec.server_path) {
        Ok(received_hash) => {
            if spec.sha256_digest == received_hash {
                Receipt::Received(spec)
            } else {
                Receipt::DifferentHash {
                    spec,
                    received_hash,
                }
            }
        }
        Err(err) => Receipt::Error(err.to_string()),
    };
    channel.lock().await.send(receipt).await.unwrap();
}
