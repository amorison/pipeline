use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{net::tcp::OwnedWriteHalf, sync::Mutex};
use tokio_serde::{SymmetricallyFramed, formats::SymmetricalJson};
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};

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

pub async fn processing_pipeline(
    file: NewFileToProcess,
    channel: Arc<
        Mutex<
            SymmetricallyFramed<
                FramedWrite<OwnedWriteHalf, LengthDelimitedCodec>,
                Received,
                SymmetricalJson<Received>,
            >,
        >,
    >,
) {
    let received: Received = file.into();
    channel.lock().await.send(received).await.unwrap();
}
