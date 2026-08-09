use super::*;
use std::io::ErrorKind;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

struct AsyncErrReader(Option<ErrorKind>);

impl AsyncRead for AsyncErrReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let kind = self.0.take().expect("reader polled again after error");
        Poll::Ready(Err(std::io::Error::from(kind)))
    }
}

struct BlockingErrReader(Option<ErrorKind>);

impl std::io::Read for BlockingErrReader {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        let kind = self.0.take().expect("reader read again after error");
        Err(std::io::Error::from(kind))
    }
}

#[tokio::test]
async fn read_raw_frame_maps_connection_reset_to_clean_eof() {
    let mut reader = AsyncErrReader(Some(ErrorKind::ConnectionReset));
    let got = read_raw_frame(&mut reader)
        .await
        .expect("a reset peer must not surface as an error");
    assert!(
        got.is_none(),
        "a reset connection must read as end-of-stream, like a clean EOF"
    );
}

#[tokio::test]
async fn read_raw_frame_propagates_non_disconnect_errors() {
    let mut reader = AsyncErrReader(Some(ErrorKind::InvalidData));
    let err = read_raw_frame(&mut reader)
        .await
        .expect_err("genuine I/O errors must still surface");
    assert_eq!(err.kind(), ErrorKind::InvalidData);
}

#[test]
fn read_raw_frame_blocking_maps_connection_reset_to_clean_eof() {
    let mut reader = BlockingErrReader(Some(ErrorKind::ConnectionReset));
    let got =
        read_raw_frame_blocking(&mut reader).expect("a reset peer must not surface as an error");
    assert!(got.is_none());
}
