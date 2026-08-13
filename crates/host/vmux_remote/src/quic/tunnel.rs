//! A UDP socket whose wire is another QUIC connection.
//!
//! The relay cannot reach a desktop behind NAT except through the mapping the desktop itself
//! opened, so a phone's packets arrive wrapped in DATAGRAM frames on the desktop's outbound
//! control connection. Handing quinn one of these instead of a real socket lets the inner
//! session — the one that actually terminates on this machine, holding the keys the relay does
//! not — run unmodified on top.
//!
//! Datagrams are lossy by design here. A send that does not fit is dropped rather than queued,
//! because the inner connection already retransmits and a second recovery loop underneath it
//! would fight the first.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, UdpPoller};

/// Bytes the relay prefixes to every tunnelled datagram, naming which peer it belongs to.
///
/// Inbound demultiplexing does not need this — quinn tells connections apart by connection ID.
/// The reply direction does: without a tag every connection would transmit to one address, and
/// the relay would have no way to tell which of a desktop's phones a datagram was for.
const TAG_BYTES: usize = 2;

/// The tag a phone always sees for its desktop.
///
/// A phone has exactly one peer, so one number does, and keeping it fixed means the phone never
/// has to learn what the relay called it. A desktop's phones get their own tags, assigned by the
/// relay, which rewrites the number as it forwards.
pub const DESKTOP_TAG: u16 = 1;

/// The synthetic address a relayed peer appears at, one per tag.
///
/// TEST-NET-1, so it can never collide with something routable. The tag rides in the port, which
/// is what makes quinn hand it back on the way out as `Transmit::destination`.
pub fn relayed_peer(tag: u16) -> SocketAddr {
    SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 1)),
        tag,
    )
}

/// The address the inner endpoint reports as its own. Nothing dials it.
const TUNNEL_LOCAL: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 2)),
    443,
);

/// How many inbound datagrams may queue before the oldest are dropped.
const INBOX_DEPTH: usize = 256;

#[derive(Debug)]
pub struct TunnelSocket {
    control: quinn::Connection,
    inbox: tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Bytes>>,
}

impl TunnelSocket {
    /// Wrap a control connection, pumping its datagrams into this socket until it closes.
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

    /// Largest inner packet that still fits in an outer DATAGRAM frame.
    ///
    /// `None` when the peer refuses datagrams outright, which makes the tunnel unusable and is
    /// worth failing loudly on rather than discovering as an unreachable desktop.
    pub fn usable_mtu(&self) -> Option<usize> {
        self.control.max_datagram_size()?.checked_sub(TAG_BYTES)
    }
}

impl AsyncUdpSocket for TunnelSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::pin(AlwaysWritable)
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        // Never reports WouldBlock: a full send buffer drops the packet, which the inner
        // connection recovers from, rather than stalling quinn behind a poller that would have to
        // learn when the outer connection drains.
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
            // The pump ended, so the control connection is gone. Reporting an error retires the
            // inner endpoint; the dialer builds a fresh one on reconnect.
            Poll::Ready(None) => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "relay control connection closed",
                )));
            }
            Poll::Pending => return Poll::Pending,
        };

        // A datagram too short to carry a tag cannot be attributed to a peer, so it is dropped
        // rather than guessed at. Waking keeps the endpoint from stalling behind it.
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
