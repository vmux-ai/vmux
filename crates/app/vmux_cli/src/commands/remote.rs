use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct RemoteArgs {
    #[arg(long)]
    pub reset: bool,
    #[command(subcommand)]
    pub command: Option<RemoteCommand>,
}

#[derive(Debug, Subcommand)]
pub enum RemoteCommand {
    List,
    Revoke { client_id: String },
}

impl RemoteArgs {
    #[cfg(target_os = "macos")]
    fn run(&self) -> std::io::Result<i32> {
        use std::time::Duration;

        match &self.command {
            Some(RemoteCommand::List) => return self.list(),
            Some(RemoteCommand::Revoke { client_id }) => return self.revoke(client_id),
            None => {}
        }
        self.start_service()?;
        let relay_token = vmux_client::RelayToken::wait(Duration::from_secs(5))?;
        let pairing_token = vmux_client::RemoteAuthorizationStore::current().pairing_token()?;
        std::fs::write(vmux_client::RemotePaths::current().state(), b"enabled\n")?;
        let relay = vmux_client::pairing::Relay::from_env();
        relay.persist()?;
        let pairing_url = relay.wait_for_pairing(
            relay_token.as_str(),
            &pairing_token,
            Duration::from_secs(20),
        )?;
        println!("paste into Vmux Remote: {pairing_url}");
        Ok(0)
    }

    #[cfg(target_os = "macos")]
    fn list(&self) -> std::io::Result<i32> {
        for device in vmux_client::RemoteAuthorizationStore::current().devices()? {
            println!("{}\t{}", device.id.as_str(), device.authorized_at_unix);
        }
        Ok(0)
    }

    #[cfg(target_os = "macos")]
    fn revoke(&self, client_id: &str) -> std::io::Result<i32> {
        let client_id = vmux_remote::DeviceId::new(client_id);
        if vmux_client::RemoteAuthorizationStore::current().revoke(&client_id)? {
            println!("revoked {}", client_id.as_str());
            return Ok(0);
        }
        eprintln!("device not found: {}", client_id.as_str());
        Ok(1)
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
            let _ = std::fs::remove_file(remote.relay_token());
            let _ = vmux_client::RemoteAuthorizationStore::current().reset();
            let _ = std::fs::remove_file(remote.relay_device());
            let _ = std::fs::remove_file(remote.relay_url());
            let _ = std::fs::remove_file(remote.relay_registration());
        }
        agent.ensure_running(vmux_client::DaemonBinary::current()?.path())
    }
}

pub fn run(args: RemoteArgs) -> std::io::Result<i32> {
    args.run()
}
