use clap::Args;

#[derive(Debug, Args)]
pub struct RemoteArgs {
    /// Revoke the old phone token before starting
    #[arg(long)]
    pub reset: bool,
}

impl RemoteArgs {
    #[cfg(target_os = "macos")]
    fn run(&self) -> std::io::Result<i32> {
        use std::time::Duration;

        self.start_service()?;
        let token = vmux_client::RemoteToken::wait(Duration::from_secs(5))?;
        std::fs::write(vmux_client::remote_state_path(), b"enabled\n")?;
        // Enabling has to come first: the port is the relay's answer to a registration the daemon
        // only attempts once Remote is on, so asking before this write would always time out.
        let relay = vmux_client::pairing::Relay::new(vmux_client::relay_url_from_env());
        relay.persist()?;
        let pairing_url = relay.wait_for_pairing(&token.0, Duration::from_secs(20))?;
        println!("paste into Vmux Remote: {pairing_url}");
        Ok(0)
    }

    #[cfg(not(target_os = "macos"))]
    fn run(&self) -> std::io::Result<i32> {
        eprintln!("vmux remote is currently macOS-only");
        Ok(2)
    }

    #[cfg(target_os = "macos")]
    fn start_service(&self) -> std::io::Result<()> {
        if self.reset {
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
}

pub fn run(args: RemoteArgs) -> std::io::Result<i32> {
    args.run()
}
