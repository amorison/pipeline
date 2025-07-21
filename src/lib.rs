use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug)]
pub struct NewFileToProcess {
    remote_path: PathBuf,
    sha256_digest: String,
}

impl NewFileToProcess {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        NewFileToProcess {
            remote_path: PathBuf::from(path.as_ref()),
            sha256_digest: "000".to_owned(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Received {
    remote_path: PathBuf,
    sha256_digest: String,
}

impl From<NewFileToProcess> for Received {
    fn from(value: NewFileToProcess) -> Self {
        let NewFileToProcess {
            remote_path,
            sha256_digest,
        } = value;
        Received {
            remote_path,
            sha256_digest,
        }
    }
}
