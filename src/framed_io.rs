use tokio::net::{
    TcpStream,
    tcp::{OwnedReadHalf, OwnedWriteHalf},
};
use tokio_serde::{SymmetricallyFramed, formats::SymmetricalJson};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

pub(crate) type ReadFramedJson<T, R> =
    SymmetricallyFramed<FramedRead<R, LengthDelimitedCodec>, T, SymmetricalJson<T>>;

pub(crate) type WriteFramedJson<T, W> =
    SymmetricallyFramed<FramedWrite<W, LengthDelimitedCodec>, T, SymmetricalJson<T>>;

pub(crate) fn framed_json_channel<T, U>(
    stream: TcpStream,
) -> (
    ReadFramedJson<T, OwnedReadHalf>,
    WriteFramedJson<U, OwnedWriteHalf>,
) {
    let (socket_r, socket_w) = stream.into_split();
    let read_half = tokio_serde::SymmetricallyFramed::new(
        FramedRead::new(socket_r, LengthDelimitedCodec::new()),
        SymmetricalJson::<T>::default(),
    );
    let write_half = tokio_serde::SymmetricallyFramed::new(
        FramedWrite::new(socket_w, LengthDelimitedCodec::new()),
        SymmetricalJson::<U>::default(),
    );
    (read_half, write_half)
}
