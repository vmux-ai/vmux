use clap::Parser;

mod commands;

use commands::{Cli, Command, open::OpenAppLauncher};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Mcp) => commands::mcp::run().await,
        Some(Command::Service(args)) => {
            let code = commands::service::run(args)?;
            std::process::exit(code);
        }
        None => commands::open::run(&OpenAppLauncher),
    }
}
