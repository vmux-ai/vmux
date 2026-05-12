use clap::{Parser, Subcommand};

pub mod mcp;
pub mod open;
pub mod service;

#[derive(Debug, Parser)]
#[command(name = "vmux", version, about = "Vmux command-line interface")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Mcp,
    Service(service::ServiceArgs),
}
