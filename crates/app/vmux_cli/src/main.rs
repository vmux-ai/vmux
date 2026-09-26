use bevy_app::AppExit;
use clap::Parser;

mod commands;

use commands::Cli;

#[tokio::main]
async fn main() -> AppExit {
    commands::run(Cli::parse()).await
}
