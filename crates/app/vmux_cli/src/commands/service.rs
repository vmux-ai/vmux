use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub action: ServiceAction,
}

#[derive(Debug, Subcommand)]
pub enum ServiceAction {
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
        use vmux_client::{DaemonBinary, cli};

        match self.action {
            ServiceAction::Status => cli::cmd_status(),
            ServiceAction::Start => cli::cmd_start(DaemonBinary::current()?.path()),
            ServiceAction::Stop => cli::cmd_stop(),
            ServiceAction::Restart => cli::cmd_restart(DaemonBinary::current()?.path()),
            ServiceAction::Logs { follow } => cli::cmd_logs(follow),
            ServiceAction::Install => cli::cmd_install(DaemonBinary::current()?.path()),
            ServiceAction::Uninstall => cli::cmd_uninstall(),
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn run(self) -> std::io::Result<i32> {
        use vmux_client::cli;

        match self.action {
            ServiceAction::Status => cli::cmd_status(),
            ServiceAction::Logs { follow } => cli::cmd_logs(follow),
            ServiceAction::Start
            | ServiceAction::Stop
            | ServiceAction::Restart
            | ServiceAction::Install
            | ServiceAction::Uninstall => {
                eprintln!("vmux service: launchd commands are macOS-only");
                Ok(2)
            }
        }
    }
}

pub fn run(args: ServiceArgs) -> std::io::Result<i32> {
    args.run()
}
