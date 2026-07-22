//! The `vmux` command-line entry point: launches and controls the app and dispatches the
//! MCP server, OS notifications, and the background service.

use clap::Parser;

mod commands;

use commands::{Cli, Command, open::OpenAppLauncher};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Mcp {
            anchor,
            profile,
            acp_session,
            acp_terminals,
            run_timeout_secs,
        }) => {
            commands::mcp::run(
                anchor,
                profile,
                acp_session,
                acp_terminals,
                run_timeout_secs,
            )
            .await
        }
        Some(Command::Notify {
            title,
            body,
            anchor,
        }) => commands::notify::run(title, body, anchor).await,
        Some(Command::NotifyFileTouch { anchor }) => commands::notify_file_touch::run(anchor).await,
        Some(Command::NotifyTurnEnd { anchor }) => commands::notify_turn_end::run(anchor).await,
        Some(Command::Service(args)) => {
            let code = commands::service::run(args)?;
            std::process::exit(code);
        }
        Some(Command::Remote(args)) => {
            let code = commands::remote::run(args)?;
            std::process::exit(code);
        }
        None => commands::open::run(&OpenAppLauncher),
    }
}
