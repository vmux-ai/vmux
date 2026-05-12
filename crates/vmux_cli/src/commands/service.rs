use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub action: ServiceAction,
}

#[derive(Debug, Subcommand)]
pub enum ServiceAction {
    /// Print daemon status
    Status,
    /// Start (kickstart) the daemon
    Start,
    /// Stop the daemon (bootout)
    Stop,
    /// Restart the daemon
    Restart,
    /// Tail the service log file
    Logs {
        /// Follow new log lines as they arrive
        #[arg(short, long)]
        follow: bool,
    },
    /// Install the LaunchAgent plist for this profile
    Install,
    /// Uninstall the LaunchAgent plist for this profile
    Uninstall,
}

pub fn run(args: ServiceArgs) -> std::io::Result<i32> {
    match args.action {
        ServiceAction::Status => vmux_service::cli::cmd_status(),
        ServiceAction::Start => {
            #[cfg(target_os = "macos")]
            {
                vmux_service::cli::cmd_start(&current_service_binary()?)
            }
            #[cfg(not(target_os = "macos"))]
            {
                not_supported()
            }
        }
        ServiceAction::Stop => {
            #[cfg(target_os = "macos")]
            {
                vmux_service::cli::cmd_stop()
            }
            #[cfg(not(target_os = "macos"))]
            {
                not_supported()
            }
        }
        ServiceAction::Restart => {
            #[cfg(target_os = "macos")]
            {
                vmux_service::cli::cmd_restart(&current_service_binary()?)
            }
            #[cfg(not(target_os = "macos"))]
            {
                not_supported()
            }
        }
        ServiceAction::Logs { follow } => vmux_service::cli::cmd_logs(follow),
        ServiceAction::Install => {
            #[cfg(target_os = "macos")]
            {
                vmux_service::cli::cmd_install(&current_service_binary()?)
            }
            #[cfg(not(target_os = "macos"))]
            {
                not_supported()
            }
        }
        ServiceAction::Uninstall => {
            #[cfg(target_os = "macos")]
            {
                vmux_service::cli::cmd_uninstall()
            }
            #[cfg(not(target_os = "macos"))]
            {
                not_supported()
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn not_supported() -> std::io::Result<i32> {
    eprintln!("vmux service: launchd commands are macOS-only");
    Ok(2)
}

fn current_service_binary() -> std::io::Result<std::path::PathBuf> {
    let mut p = std::env::current_exe()?;
    p.pop();
    p.push("vmux_service");
    Ok(p)
}
