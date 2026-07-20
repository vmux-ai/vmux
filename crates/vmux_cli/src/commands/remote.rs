use std::process::Command;
use std::time::{Duration, Instant};

use clap::Args;

#[derive(Debug, Args)]
pub struct RemoteArgs {
    /// Keep the app on localhost and skip Tailscale Serve
    #[arg(long)]
    pub local: bool,
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
        if args.local {
            println!("paste into Vmux Remote: {local_url}");
            return Ok(0);
        }

        let target = format!("http://127.0.0.1:{port}");
        let served = Command::new("tailscale")
            .args(["serve", "--bg", &target])
            .status()
            .is_ok_and(|status| status.success());
        let dns_name = tailscale_dns_name();
        if served && let Some(host) = dns_name {
            println!("paste into Vmux Remote: https://{host}/#token={token}");
            return Ok(0);
        }

        println!("mobile app ready on {target}");
        println!("run: tailscale serve --bg {target}");
        if let Some(host) = dns_name {
            println!("paste into Vmux Remote: https://{host}/#token={token}");
        } else {
            println!("pair token: {token}");
        }
        Ok(1)
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

fn tailscale_dns_name() -> Option<String> {
    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    tailscale_dns_name_from_json(&output.stdout)
}

fn tailscale_dns_name_from_json(bytes: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()?
        .get("Self")?
        .get("DNSName")?
        .as_str()
        .map(|name| name.trim_end_matches('.').to_string())
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tailscale_dns_name() {
        let json = br#"{"Self":{"DNSName":"mac.tailnet.ts.net."}}"#;
        assert_eq!(
            tailscale_dns_name_from_json(json).as_deref(),
            Some("mac.tailnet.ts.net")
        );
    }
}
