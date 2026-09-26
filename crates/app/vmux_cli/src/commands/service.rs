use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub command: ServiceCommand,
}

#[derive(Debug, Subcommand)]
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
    fn run(self) -> std::io::Result<i32> {
        use vmux_service::{DaemonBinary, cli};

        match self.command {
            ServiceCommand::Status => cli::cmd_status(),
            ServiceCommand::Start => cli::cmd_start(DaemonBinary::current()?.path()),
            ServiceCommand::Stop => cli::cmd_stop(),
            ServiceCommand::Restart => cli::cmd_restart(DaemonBinary::current()?.path()),
            ServiceCommand::Logs { follow } => cli::cmd_logs(follow),
            ServiceCommand::Install => cli::cmd_install(DaemonBinary::current()?.path()),
            ServiceCommand::Uninstall => cli::cmd_uninstall(),
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn run(self) -> std::io::Result<i32> {
        use vmux_service::cli;

        match self.command {
            ServiceCommand::Status => cli::cmd_status(),
            ServiceCommand::Logs { follow } => cli::cmd_logs(follow),
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

pub fn run(args: ServiceArgs) -> std::io::Result<i32> {
    args.run()
}
