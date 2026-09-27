use bevy_app::AppExit;
use clap::Parser;

mod command;

use command::Cli;

#[tokio::main]
async fn main() -> AppExit {
    command::run(Cli::parse()).await
}
