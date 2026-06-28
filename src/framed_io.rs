use tokio::{
    io::{self, AsyncWrite, Sink},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf, ReadHalf, WriteHalf},
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

pub(crate) trait Splittable<R, W>
where
    Self: Sized,
{
    fn split(self) -> (R, W);
}

impl Splittable<OwnedReadHalf, OwnedWriteHalf> for TcpStream {
    fn split(self) -> (OwnedReadHalf, OwnedWriteHalf) {
        self.into_split()
    }
}

impl<'a> Splittable<ReadHalf<'a>, WriteHalf<'a>> for &'a mut TcpStream {
    fn split(self) -> (ReadHalf<'a>, WriteHalf<'a>) {
        self.split()
    }
}

pub(crate) fn json_channel<T, U, R, W, S>(
    stream: S,
) -> (ReadFramedJson<T, R>, WriteFramedJson<U, W>)
where
    S: Splittable<R, W>,
    W: AsyncWrite,
{
    let (socket_r, socket_w) = stream.split();
    let read_half = tokio_serde::SymmetricallyFramed::new(
        FramedRead::new(socket_r, LengthDelimitedCodec::new()),
        SymmetricalJson::<T>::default(),
    );
    let write_half = framed_json_writer(socket_w);
    (read_half, write_half)
}

pub(crate) fn framed_json_sink<T>() -> WriteFramedJson<T, Sink> {
    framed_json_writer(io::sink())
}
