use std::{io, path::Path};

use sha2::{Digest, Sha256};

pub(crate) fn file_hash(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let file = std::fs::File::open(path)?;
    let mut reader = io::BufReader::new(file);
    io::copy(&mut reader, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}
