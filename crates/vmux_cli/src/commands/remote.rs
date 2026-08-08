#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

use clap::Args;

#[derive(Debug, Args)]
pub struct RemoteArgs {
    /// Revoke the old phone token before starting
    #[arg(long)]
    pub reset: bool,
}

pub fn run(args: RemoteArgs) -> std::io::Result<i32> {
    #[cfg(target_os = "macos")]
    {
        start_service(args.reset)?;
        let token = wait_for_token()?;
        std::fs::write(vmux_client::remote_state_path(), b"enabled\n")?;
        // Enabling has to come first: the port is the relay's answer to a registration the daemon
        // only attempts once Remote is on, so asking before this write would always time out.
        let pairing_url = wait_for_pairing_url(&token)?;
        println!("paste into Vmux Remote: {pairing_url}");
        Ok(0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = args;
        eprintln!("vmux remote is currently macOS-only");
        Ok(2)
    }
}

#[cfg(target_os = "macos")]
fn start_service(reset: bool) -> std::io::Result<()> {
    if reset {
        let _ = vmux_client::launchd::bootout(vmux_client::current_profile());
        let _ = std::fs::remove_file(vmux_client::remote_token_path());
        let _ = std::fs::remove_file(vmux_client::remote_paired_path());
        let _ = std::fs::remove_file(vmux_client::remote_relay_device_path());
        let _ = std::fs::remove_file(vmux_client::remote_relay_url_path());
        // A new device id earns a different port, so the recorded one would name someone else's.
        let _ = std::fs::remove_file(vmux_client::remote_relay_port_path());
    }
    vmux_client::launchd::ensure_running(
        vmux_client::current_profile(),
        &super::service::current_service_binary()?,
    )
}

/// Wait until the daemon has registered, then build the link a phone can scan.
///
/// The port comes from the relay and the fingerprint from the certificate the daemon loads, so
/// neither exists until it has started and dialled out. Waiting beats printing a link that names
/// a port nothing answers on.
#[cfg(target_os = "macos")]
fn wait_for_pairing_url(token: &str) -> std::io::Result<String> {
    let relay_url = vmux_client::relay_url_from_env();
    vmux_client::pairing::persist_relay_url(&relay_url)?;

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let (Some(port), Some(fingerprint)) = (
            vmux_client::pairing::allocated_port(),
            vmux_client::pairing::certificate_fingerprint(),
        ) {
            let base = vmux_client::pairing::relay_base_url(&relay_url, port)
                .map_err(std::io::Error::other)?;
            let pairing = vmux_client::pairing::pairing_info(&base, token, &fingerprint)
                .map_err(std::io::Error::other)?;
            return Ok(pairing.url);
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("{relay_url} has not allocated a port for this Mac yet"),
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(target_os = "macos")]
fn wait_for_token() -> std::io::Result<String> {
    let path = vmux_client::remote_token_path();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(token) = std::fs::read_to_string(&path) {
            let token = token.trim();
            if token.len() >= 32 {
                return Ok(token.to_string());
            }
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("remote token not created: {}", path.display()),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
