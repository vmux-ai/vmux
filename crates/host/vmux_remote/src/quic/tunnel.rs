use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};

const TAG_BYTES: usize = 2;

pub const DESKTOP_TAG: u16 = 1;

pub fn relayed_peer(tag: u16) -> SocketAddr {
    SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 1)),
        tag,
    )
}

const TUNNEL_LOCAL: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 2)),
    443,
);

const INBOX_DEPTH: usize = 256;

#[derive(Debug)]
pub struct TunnelSocket {
    control: quinn::Connection,
    inbox: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Bytes>>,
}

impl TunnelSocket {
    pub fn new(control: quinn::Connection) -> Arc<Self> {
        let (tx, rx) = tokio::sync::mpsc::channel(INBOX_DEPTH);
        let pump = control.clone();
        tokio::spawn(async move {
            while let Ok(datagram) = pump.read_datagram().await {
                if tx.send(datagram).await.is_err() {
                    return;
                }
            }
        });
        Arc::new(Self {
            control,
            inbox: tokio::sync::Mutex::new(rx),
        })
    }

    pub fn usable_mtu(&self) -> Option<usize> {
        self.control.max_datagram_size()?.checked_sub(TAG_BYTES)
    }
}

impl AsyncUdpSocket for TunnelSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::pin(AlwaysWritable)
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        let mut tagged = Vec::with_capacity(TAG_BYTES + transmit.contents.len());
        tagged.extend_from_slice(&transmit.destination.port().to_be_bytes());
        tagged.extend_from_slice(transmit.contents);
        let _ = self.control.send_datagram(Bytes::from(tagged));
        Ok(())
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let Some(buffer) = bufs.first_mut() else {
            return Poll::Ready(Ok(0));
        };
        let Ok(mut inbox) = self.inbox.try_lock() else {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        };
        let datagram = match inbox.poll_recv(cx) {
            Poll::Ready(Some(datagram)) => datagram,
            Poll::Ready(None) => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "relay control connection closed",
                )));
            }
            Poll::Pending => return Poll::Pending,
        };

        if datagram.len() < TAG_BYTES {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        let tag = u16::from_be_bytes([datagram[0], datagram[1]]);
        let payload = &datagram[TAG_BYTES..];

        let len = payload.len().min(buffer.len());
        buffer[..len].copy_from_slice(&payload[..len]);
        meta[0] = RecvMeta {
            addr: relayed_peer(tag),
            len,
            stride: len,
            ecn: None,
            dst_ip: None,
        };
        Poll::Ready(Ok(1))
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(TUNNEL_LOCAL)
    }

    fn max_transmit_segments(&self) -> usize {
        1
    }

    fn max_receive_segments(&self) -> usize {
        1
    }
}

#[derive(Debug)]
struct AlwaysWritable;

impl UdpPoller for AlwaysWritable {
    fn poll_writable(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
