use std::{
    ffi::OsStr,
    io::{self, Read},
    path::Path,
};

use log::debug;
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
        if full {
            Self::new_helper(path, full, "", 0)
        } else {
            let name = path
                .file_name()
                .map(OsStr::to_str)
                .expect("failed to get filename")
                .expect("failed to convert filename to utf8");
            let size = path.metadata()?.len();
            Self::new_helper(path, full, name, size)
        }
    }

    fn new_helper(path: &Path, full: bool, name: &str, size: u64) -> io::Result<Self> {
        if full {
            debug!("computing full hash for {path:?}");
        } else {
            debug!("computing shallow hash for {path:?}, with name={name} and size={size}");
        }
        let mut hasher = Sha256::new();
        let mut file = std::fs::File::open(path)?;
        if full {
            let mut reader = io::BufReader::new(file);
            io::copy(&mut reader, &mut hasher)?;
        } else {
            hasher.update(name);
            hasher.update(size.to_le_bytes());
            let mut data = vec![0; 1024 * 1024];
            let mut idx = 0;
            while let read_bytes = file.read(&mut data[idx..])?
                && read_bytes != 0
            {
                idx += read_bytes;
            }
            hasher.update(data);
        }
        let hash = hex::encode(hasher.finalize());
        if full {
            Ok(Self::Full(hash))
        } else {
            Ok(Self::Shallow(hash))
        }
    }

    pub(crate) fn with_spec(path: &Path, spec: &FileSpec) -> io::Result<Self> {
        match &spec.sha256_digest {
            Self::Shallow(_) => {
                let size = path.metadata()?.len();
                Self::new_helper(path, false, &spec.filename, size)
            }
            Self::Full(_) => Self::new_helper(path, true, "", 0),
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
