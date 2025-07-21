use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
