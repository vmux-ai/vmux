use std::path::Path;
use std::time::{Duration, Instant};

use vmux_api::protocol::ClientMessage;
use vmux_profile::{DataLease, ProfilePaths};
use vmux_transport::service::ServiceCodec;

pub struct RuntimeHandoff;

impl RuntimeHandoff {
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
        let mut stream = match std::os::unix::net::UnixStream::connect(socket) {
            Ok(stream) => stream,
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
        ServiceCodec::write_client_blocking(&mut stream, &ClientMessage::Shutdown)?;
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
