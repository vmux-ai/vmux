use clap::{Parser, Subcommand};

pub mod mcp;
pub mod notify;
pub mod notify_file_touch;
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
    Mcp {
        #[arg(long)]
        anchor: Option<String>,
        #[arg(long)]
        profile: Option<String>,
    },
    Notify {
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        anchor: Option<String>,
    },
    NotifyFileTouch {
        #[arg(long)]
        anchor: Option<String>,
    },
    Service(service::ServiceArgs),
}
