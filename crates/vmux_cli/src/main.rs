use clap::Parser;

mod commands;

use commands::{Cli, Command, open::OpenAppLauncher};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Mcp) => commands::mcp::run().await,
        None => commands::open::run(&OpenAppLauncher),
    }
}
