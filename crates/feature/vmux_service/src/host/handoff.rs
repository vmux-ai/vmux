#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::path::Path;
use std::time::{Duration, Instant};

use vmux_api::protocol::ClientMessage;
use vmux_profile::{DataLease, ProfilePaths};
use vmux_transport::service::ServiceCodec;

pub struct RuntimeHandoff;

impl RuntimeHandoff {
    pub(super) fn shutdown_runtime(
        socket: &Path,
        lock: &Path,
        timeout: Duration,
    ) -> std::io::Result<()> {
        let deadline = Instant::now() + timeout;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let lock = match options.open(lock) {
            Ok(lock) => lock,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Self::shutdown(socket, deadline);
            }
            Err(error) => return Err(error),
        };
        if !lock.metadata()?.is_file() {
            return Err(std::io::Error::other("invalid service lock"));
        }
        Self::shutdown(socket, deadline)?;
        loop {
            match lock.try_lock() {
                Ok(()) => return lock.unlock(),
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(std::fs::TryLockError::Error(error)) => return Err(error),
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Vmux service did not finish shutdown",
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    pub fn acquire() -> std::io::Result<DataLease> {
        let mut lease = DataLease::reserve_desktop()?;
        match lease.activate_runtime() {
            Ok(()) => return Ok(lease),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
        let build = lease.active_build()?;
        let data = ProfilePaths::current().shared_data();
        let directory = if matches!(build.as_str(), "release" | "local") {
            data.join("services")
        } else {
            data.join(&build).join("services")
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        Self::shutdown_services(&directory, &build, deadline)?;
        loop {
            match lease.activate_runtime() {
                Ok(()) => return Ok(lease),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "The previous Vmux service did not release its data",
                        ));
                    }
                }
                Err(error) => return Err(error),
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn shutdown_services(directory: &Path, build: &str, deadline: Instant) -> std::io::Result<()> {
        if !directory.exists() {
            return Ok(());
        }
        let prefix = format!("vmux-{build}");
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("sock") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            if stem != prefix && !stem.starts_with(&format!("{prefix}-")) {
                continue;
            }
            Self::shutdown(&path, deadline)?;
        }
        Ok(())
    }

    fn shutdown(socket: &Path, deadline: Instant) -> std::io::Result<()> {
        match std::fs::symlink_metadata(socket) {
            Ok(metadata) if metadata.file_type().is_socket() => {}
            Ok(_) => return Err(std::io::Error::other("invalid service socket")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Vmux service shutdown timed out",
            ));
        }
        let connection = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
        let address = socket2::SockAddr::unix(socket)?;
        let mut stream: std::os::unix::net::UnixStream =
            match connection.connect_timeout(&address, remaining.min(Duration::from_secs(1))) {
                Ok(()) => connection.into(),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) =>
                {
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Vmux service handoff timed out",
            ));
        }
        stream.set_write_timeout(Some(remaining.min(Duration::from_secs(1))))?;
        if let Err(error) =
            ServiceCodec::write_client_blocking(&mut stream, &ClientMessage::Shutdown)
        {
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ) {
                return Ok(());
            }
            return Err(error);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            stream.set_read_timeout(Some(remaining))?;
            let _ = ServiceCodec::read_service_blocking(&mut std::io::BufReader::new(&mut stream));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_shutdown_does_not_contact_a_socket_reached_through_a_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("unrelated.sock");
        let listener = std::os::unix::net::UnixListener::bind(&target).unwrap();
        listener.set_nonblocking(true).unwrap();
        let socket = directory.path().join("copied.sock");
        std::os::unix::fs::symlink(&target, &socket).unwrap();
        let lock = directory.path().join("copied.lock");
        std::fs::File::create(&lock).unwrap();
        assert!(
            RuntimeHandoff::shutdown_runtime(&socket, &lock, Duration::from_millis(50)).is_err()
        );
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }

    #[test]
    fn application_shutdown_waits_for_the_daemon_lease_after_its_acknowledgement() {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("service.sock");
        let lock = directory.path().join("service.lock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let file = std::fs::File::create(&lock).unwrap();
        file.lock().unwrap();
        let (acknowledged, received) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let payload = ServiceCodec::read_raw_blocking(&mut stream)
                .unwrap()
                .unwrap();
            let message = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&payload).unwrap();
            assert!(matches!(message, ClientMessage::Shutdown));
            let response = rkyv::to_bytes::<rkyv::rancor::Error>(
                &vmux_api::protocol::ServiceMessage::ProcessList {
                    processes: Vec::new(),
                },
            )
            .unwrap();
            ServiceCodec::write_raw_blocking(&mut stream, &response).unwrap();
            acknowledged.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(2)).unwrap();
            drop(file);
        });
        let (done, finished) = std::sync::mpsc::channel();
        let shutdown = std::thread::spawn(move || {
            done.send(RuntimeHandoff::shutdown_runtime(
                &socket,
                &lock,
                Duration::from_secs(2),
            ))
            .unwrap()
        });
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(
            finished.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        release.send(()).unwrap();
        finished
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        server.join().unwrap();
        shutdown.join().unwrap();
    }

    #[test]
    fn application_shutdown_does_not_follow_a_daemon_lock_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        std::fs::write(&data, "preserved").unwrap();
        let lock = directory.path().join("service.lock");
        std::os::unix::fs::symlink(&data, &lock).unwrap();
        assert!(
            RuntimeHandoff::shutdown_runtime(
                &directory.path().join("absent.sock"),
                &lock,
                Duration::from_millis(50)
            )
            .is_err()
        );
        assert_eq!(std::fs::read_to_string(data).unwrap(), "preserved");
    }

    #[test]
    fn switching_builds_sends_shutdown_only_to_the_previous_runtime() {
        let directory = tempfile::tempdir().unwrap();
        let old = directory.path().join("vmux-local.sock");
        let unrelated = directory.path().join("vmux-dev.sock");
        let listener = std::os::unix::net::UnixListener::bind(&old).unwrap();
        let other = std::os::unix::net::UnixListener::bind(&unrelated).unwrap();
        other.set_nonblocking(true).unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let payload = ServiceCodec::read_raw_blocking(&mut stream)
                .unwrap()
                .unwrap();
            let message = rkyv::from_bytes::<ClientMessage, rkyv::rancor::Error>(&payload).unwrap();
            assert!(matches!(message, ClientMessage::Shutdown));
        });
        RuntimeHandoff::shutdown_services(
            directory.path(),
            "local",
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        worker.join().unwrap();
        assert!(
            matches!(other.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}
