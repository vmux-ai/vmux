use bevy_ecs::prelude::Component;
use clap::{Args, Subcommand};

#[derive(Args, Clone, Component, Debug)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub command: ServiceCommand,
}

#[derive(Clone, Debug, Subcommand)]
pub enum ServiceCommand {
    Status,
    Start,
    Stop,
    Restart,
    Logs {
        #[arg(short, long)]
        follow: bool,
    },
    Install,
    Uninstall,
}

impl ServiceArgs {
    #[cfg(target_os = "macos")]
    pub(crate) fn execute(&self) -> std::io::Result<i32> {
        use vmux_service::{DaemonBinary, cli};

        match &self.command {
            ServiceCommand::Status => cli::cmd_status(),
            ServiceCommand::Start => match DaemonBinary::current() {
                Ok(binary) => cli::cmd_start(binary.path()),
                Err(error) => Err(error),
            },
            ServiceCommand::Stop => cli::cmd_stop(),
            ServiceCommand::Restart => match DaemonBinary::current() {
                Ok(binary) => cli::cmd_restart(binary.path()),
                Err(error) => Err(error),
            },
            ServiceCommand::Logs { follow } => cli::cmd_logs(*follow),
            ServiceCommand::Install => match DaemonBinary::current() {
                Ok(binary) => cli::cmd_install(binary.path()),
                Err(error) => Err(error),
            },
            ServiceCommand::Uninstall => cli::cmd_uninstall(),
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn execute(&self) -> std::io::Result<i32> {
        use vmux_service::cli;

        match &self.command {
            ServiceCommand::Status => cli::cmd_status(),
            ServiceCommand::Logs { follow } => cli::cmd_logs(*follow),
            ServiceCommand::Start
            | ServiceCommand::Stop
            | ServiceCommand::Restart
            | ServiceCommand::Install
            | ServiceCommand::Uninstall => {
                eprintln!("vmux service: launchd commands are macOS-only");
                Ok(2)
            }
        }
    }
}
