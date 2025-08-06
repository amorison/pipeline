pub mod cli;
mod client;
mod server;

use bstr::{ByteSlice, ByteVec};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
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

fn replace_os_strings<'a, I>(arg: &str, replacements: I) -> OsString
where
    I: Iterator<Item = (&'a str, &'a OsStr)>,
{
    replacements
        .fold(arg.into(), |b: Vec<u8>, (needle, replacement)| {
            b.replace(needle, replacement.as_encoded_bytes())
        })
        .into_os_string()
        .expect("failed to encode OS string")
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileSpec {
    filename: String,
    sha256_digest: String,
}

impl FileSpec {
    fn new(client_path: PathBuf) -> io::Result<Self> {
        let sha256_digest = file_hash(&client_path)?;
        let filename = client_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        Ok(FileSpec {
            filename,
            sha256_digest,
        })
    }

    fn server_filename(&self) -> &OsStr {
        self.sha256_digest.as_ref()
    }

    fn file_stem(&self) -> &OsStr {
        let path: &Path = self.filename.as_ref();
        path.file_stem().unwrap()
    }
}

#[derive(Serialize, Deserialize, Debug)]
enum Receipt {
    Received(FileSpec),
    DifferentHash {
        spec: FileSpec,
        received_hash: String,
    },
    Error {
        spec: FileSpec,
        error: String,
    },
}

impl Receipt {
    fn continue_processing(&self) -> bool {
        matches!(self, Self::Received(_))
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn replace_osstr_nothing_to_replace() {
        let arg = "foo-file_path";
        let out = replace_os_strings(arg, [("{foo}", "bar".as_ref())].into_iter());
        assert_eq!(out, arg);
    }

    #[test]
    fn replace_osstr_one() {
        let arg = "foo-{file_path}";
        let out = replace_os_strings(arg, [("{file_path}", "./server.file".as_ref())].into_iter());
        assert_eq!(out, "foo-./server.file");
    }

    #[test]
    fn replace_osstr_two() {
        let arg = "{foo} {bar}";
        let out = replace_os_strings(
            arg,
            [("{foo}", "hello".as_ref()), ("{bar}", "world".as_ref())].into_iter(),
        );
        assert_eq!(out, "hello world");
    }
}
