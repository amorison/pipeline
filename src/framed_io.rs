use tokio::{
    io::AsyncWrite,
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
};
use tokio_serde::{SymmetricallyFramed, formats::SymmetricalJson};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

pub(crate) type ReadFramedJson<T, R> =
    SymmetricallyFramed<FramedRead<R, LengthDelimitedCodec>, T, SymmetricalJson<T>>;

pub(crate) type WriteFramedJson<T, W> =
    SymmetricallyFramed<FramedWrite<W, LengthDelimitedCodec>, T, SymmetricalJson<T>>;

fn framed_json_writer<T, W: AsyncWrite>(writer: W) -> WriteFramedJson<T, W> {
    tokio_serde::SymmetricallyFramed::new(
        FramedWrite::new(writer, LengthDelimitedCodec::new()),
        SymmetricalJson::<T>::default(),
    )
}

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
    let write_half = framed_json_writer(socket_w);
    (read_half, write_half)
}
