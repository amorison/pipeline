use std::{io, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::FileSpec;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileDigest {
    Shallow(String),
    Full(String),
}

impl FileDigest {
    pub(crate) fn new(path: &Path, full: bool) -> io::Result<Self> {
        let mut hasher = Sha256::new();
        let file = std::fs::File::open(path)?;
        if full {
            let mut reader = io::BufReader::new(file);
            io::copy(&mut reader, &mut hasher)?;
        } else {
            todo!();
        }
        let hash = hex::encode(hasher.finalize());
        Ok(Self::Full(hash))
    }

    pub(crate) fn with_spec(path: &Path, spec: &FileSpec) -> io::Result<Self> {
        match &spec.sha256_digest {
            Self::Shallow(_) => todo!(),
            Self::Full(_) => Self::new(path, true),
        }
    }

    pub(crate) fn hash(&self) -> &str {
        match self {
            FileDigest::Shallow(h) => h,
            FileDigest::Full(h) => h,
        }
    }

    pub(crate) fn is_full(&self) -> bool {
        match self {
            FileDigest::Shallow(_) => false,
            FileDigest::Full(_) => true,
        }
    }
}
