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
        let port = vmux_service::remote_port();
        let local_url = format!("http://127.0.0.1:{port}/#token={token}");
        std::fs::write(vmux_service::remote_state_path(), b"enabled\n")?;
        println!("paste into Vmux Remote: {local_url}");
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
        let _ = vmux_service::launchd::bootout(vmux_service::current_profile());
        let _ = std::fs::remove_file(vmux_service::remote_token_path());
        let _ = std::fs::remove_file(vmux_service::remote_paired_path());
    }
    vmux_service::launchd::ensure_running(
        vmux_service::current_profile(),
        &super::service::current_service_binary()?,
    )
}

#[cfg(target_os = "macos")]
fn wait_for_token() -> std::io::Result<String> {
    let path = vmux_service::remote_token_path();
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
