pub mod cli;
mod client;
mod server;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
};
use tokio::net::{
    TcpStream,
    tcp::{OwnedReadHalf, OwnedWriteHalf},
};
use tokio_serde::{SymmetricallyFramed, formats::SymmetricalJson};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

// FIXME: check whether it is worthwhile to make this non-blocking
fn file_hash(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let file = std::fs::File::open(path)?;
    let mut reader = io::BufReader::new(file);
    io::copy(&mut reader, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileSpec {
    client_path: PathBuf,
    server_path: PathBuf,
    sha256_digest: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct NewFileToProcess(FileSpec);

impl NewFileToProcess {
    fn new(client_path: PathBuf, server_path: PathBuf) -> io::Result<Self> {
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
enum Receipt {
    Received(FileSpec),
    DifferentHash {
        spec: FileSpec,
        received_hash: String,
    },
    Error(String),
}

impl FileSpec {
    pub fn client_path(&self) -> &Path {
        &self.client_path
    }
}

type ReadFramedJson<T> =
    SymmetricallyFramed<FramedRead<OwnedReadHalf, LengthDelimitedCodec>, T, SymmetricalJson<T>>;

type WriteFramedJson<T> =
    SymmetricallyFramed<FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>, T, SymmetricalJson<T>>;

fn framed_json_channel<R, W>(stream: TcpStream) -> (ReadFramedJson<R>, WriteFramedJson<W>) {
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
