use super::device::{Axe, SimulatorDevice};
use bevy::prelude::*;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::process::Stdio;

/// Serves the device's MJPEG stream on loopback so the view can point an `<img>` at it.
///
/// `axe stream-video` writes a complete HTTP response — status line, then
/// `multipart/x-mixed-replace` parts — so this copies its stdout to the socket verbatim rather
/// than parsing and re-encoding. That is also why the page needs no decoder: Chromium renders
/// this format natively, and the frames never enter the Bevy world at all.
#[derive(Resource)]
pub struct StreamServer {
    port: u16,
}

impl StreamServer {
    const FPS: &'static str = "20";
    const SCALE: &'static str = "0.5";

    /// Binds an ephemeral loopback port and serves one `axe` child per connection.
    ///
    /// A child per connection rather than one shared child: the stream cannot seek or replay, so
    /// a reload needs a fresh one, and dying with the socket is what stops `axe` when the page
    /// goes away.
    pub fn start(axe: &Axe, device: SimulatorDevice) -> io::Result<Self> {
        let axe = axe.path().to_path_buf();
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        std::thread::Builder::new()
            .name("vmux-simulator-stream".into())
            .spawn(move || Self::accept_loop(listener, axe, device))?;
        Ok(Self { port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    fn accept_loop(listener: TcpListener, axe: std::path::PathBuf, device: SimulatorDevice) {
        for connection in listener.incoming() {
            let Ok(socket) = connection else {
                continue;
            };
            let device = device.clone();
            let axe = axe.clone();
            let spawned = std::thread::Builder::new()
                .name("vmux-simulator-pipe".into())
                .spawn(move || Self::pipe(socket, axe, device));
            if spawned.is_err() {
                warn!("could not spawn a stream thread");
            }
        }
    }

    fn pipe(mut socket: TcpStream, axe: std::path::PathBuf, device: SimulatorDevice) {
        let child = std::process::Command::new(axe)
            .args(["stream-video", "--udid", &device.udid])
            .args(["--format", "mjpeg"])
            .args(["--fps", Self::FPS])
            .args(["--scale", Self::SCALE])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            warn!("could not start `{} stream-video`", Axe::BIN);
            return;
        };
        if let Some(mut stdout) = child.stdout.take() {
            // Ends when the page navigates away and Chromium drops the socket.
            let _ = io::copy(&mut stdout, &mut socket);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}
