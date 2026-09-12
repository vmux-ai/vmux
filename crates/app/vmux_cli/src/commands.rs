use clap::{Parser, Subcommand};

pub mod mcp;
pub mod notify;
pub mod notify_file_touch;
pub mod notify_turn_end;
pub mod open;
pub mod remote;
pub mod service;
pub mod tools;

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
        #[arg(long)]
        acp_session: bool,
        #[arg(long)]
        acp_terminals: bool,
        #[arg(long, default_value_t = 50)]
        run_timeout_secs: u64,
        #[arg(long, default_value_t = String::new())]
        shell: String,
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
    NotifyTurnEnd {
        #[arg(long)]
        anchor: Option<String>,
    },
    Tools(tools::ToolsArgs),
    Service(service::ServiceArgs),
    Remote(remote::RemoteArgs),
}
