pub mod cli;
mod client;
mod hashing;
mod server;

use bstr::{ByteSlice, ByteVec};
use serde::{Deserialize, Serialize};
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

use crate::hashing::FileDigest;

/// Join paths while ensuring the use of platform-specific delimiters
fn assemble_path<P1: AsRef<Path>, P2: AsRef<Path>>(dir: P1, relative: P2) -> PathBuf {
    let mut path = PathBuf::with_capacity(128);
    path.extend(dir.as_ref().components());
    path.extend(relative.as_ref().components());
    path
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
    client: String,
    path: String,
    filename: String,
    sha256_digest: FileDigest,
}

impl FileSpec {
    fn new<S: Into<String>>(
        client: S,
        root: &Path,
        client_path: &Path,
        full_hash: bool,
    ) -> io::Result<Self> {
        let client = client.into();
        let sha256_digest = FileDigest::new(client_path, full_hash)?;
        let filename = client_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let segments: Vec<String> = client_path
            .parent()
            .unwrap()
            .strip_prefix(root)
            .expect("root should be parent of path")
            .iter()
            .map(|segment| segment.to_str().unwrap())
            .map(ToOwned::to_owned)
            .collect();
        let path = segments.join("/");
        Ok(FileSpec {
            client,
            path,
            filename,
            sha256_digest,
        })
    }

    fn hash(&self) -> &str {
        self.sha256_digest.hash()
    }

    fn relative_directory(&self) -> PathBuf {
        assemble_path(&self.path, "")
    }

    fn relative_path(&self) -> PathBuf {
        let mut path = self.relative_directory();
        path.push(&self.filename);
        path
    }

    fn file_stem(&self) -> &OsStr {
        let path: &Path = self.filename.as_ref();
        path.file_stem().unwrap()
    }
}

#[derive(Serialize, Deserialize, Debug)]
enum Receipt {
    Expecting {
        spec: FileSpec,
        server_rel_path: String,
    },
    Received(FileSpec),
    DifferentHash(FileSpec),
    Error {
        spec: FileSpec,
        server_rel_path: String,
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

    #[test]
    fn assemble_path_subdirs() {
        let path1 = "foo/bar";
        let path2 = "baz/bam";
        let out = assemble_path(path1, path2);
        let expected: PathBuf = ["foo", "bar", "baz", "bam"].iter().collect();
        assert_eq!(out, expected);
    }

    #[test]
    fn assemble_path_empty_first() {
        let path1 = "";
        let path2 = "foo/bar";
        let out = assemble_path(path1, path2);
        let expected: PathBuf = ["foo", "bar"].iter().collect();
        assert_eq!(out, expected);
    }

    #[test]
    fn assemble_path_empty_second() {
        let path1 = "foo/bar";
        let path2 = "";
        let out = assemble_path(path1, path2);
        let expected: PathBuf = ["foo", "bar"].iter().collect();
        assert_eq!(out, expected);
    }
}
