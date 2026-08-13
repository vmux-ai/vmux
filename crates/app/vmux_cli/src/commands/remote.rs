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
        std::fs::write(vmux_client::RemotePaths::current().state(), b"enabled\n")?;
        // Enabling has to come first: the port is the relay's answer to a registration the daemon
        // only attempts once Remote is on, so asking before this write would always time out.
        let relay = vmux_client::pairing::Relay::from_env();
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
        let agent = vmux_client::LaunchAgent::current();
        if self.reset {
            let remote = vmux_client::RemotePaths::current();
            let _ = agent.bootout();
            let _ = std::fs::remove_file(remote.token());
            let _ = std::fs::remove_file(remote.paired());
            let _ = std::fs::remove_file(remote.relay_device());
            let _ = std::fs::remove_file(remote.relay_url());
            // The next device id is a different desktop as far as the relay is concerned, so a
            // registration recorded for the old one would put someone else's id in a pairing link.
            let _ = std::fs::remove_file(remote.relay_registration());
        }
        agent.ensure_running(vmux_client::DaemonBinary::current()?.path())
    }
}

pub fn run(args: RemoteArgs) -> std::io::Result<i32> {
    args.run()
}
