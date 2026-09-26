use std::io;
use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use vmux_tool::DotfileLinkState;

#[derive(Debug, Args)]
pub struct ToolArgs {
    #[command(subcommand)]
    command: ToolCommand,
}

#[derive(Debug, Subcommand)]
enum ToolCommand {
    Status,
    Apply,
    Import {
        provider: ToolImportProvider,
        path: Option<PathBuf>,
    },
    Adopt {
        path: PathBuf,
        #[arg(long)]
        package: String,
    },
    Unlink {
        package: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ToolImportProvider {
    Homebrew,
    Npm,
    Mcp,
    Dotfiles,
}

pub fn run(args: ToolArgs) -> io::Result<()> {
    match args.command {
        ToolCommand::Status => status(),
        ToolCommand::Apply => {
            let manifest = vmux_tool::ToolStore::current()
                .load()
                .map_err(io::Error::other)?;
            let linked = vmux_tool::apply_enabled_dotfiles(&manifest).map_err(io::Error::other)?;
            println!("linked {linked} file(s)");
            Ok(())
        }
        ToolCommand::Import { provider, path } => import(provider, path),
        ToolCommand::Adopt { path, package } => {
            let destination =
                vmux_tool::adopt_dotfile(&path, &package).map_err(io::Error::other)?;
            println!("{}", destination.display());
            Ok(())
        }
        ToolCommand::Unlink { package } => {
            let removed = vmux_tool::disable_and_unlink_dotfile_package(&package)
                .map_err(io::Error::other)?;
            println!("unlinked {removed} file(s)");
            Ok(())
        }
    }
}

fn import(provider: ToolImportProvider, path: Option<PathBuf>) -> io::Result<()> {
    match provider {
        ToolImportProvider::Homebrew => {
            let path = path.ok_or_else(|| io::Error::other("Brewfile path is required"))?;
            let (formulae, casks) = vmux_tool::import_brewfile(&path).map_err(io::Error::other)?;
            println!("imported {formulae} formulae and {casks} casks");
        }
        ToolImportProvider::Npm => {
            let path = path.ok_or_else(|| io::Error::other("package.json path is required"))?;
            let imported = vmux_tool::import_npm_manifest(&path).map_err(io::Error::other)?;
            println!("imported {imported} NPM package(s)");
        }
        ToolImportProvider::Mcp => {
            let imported = if let Some(path) = path {
                vmux_tool::import_mcp_config(&path)
            } else {
                vmux_tool::import_default_mcp_configs()
            }
            .map_err(io::Error::other)?;
            println!("imported {imported} MCP server(s)");
        }
        ToolImportProvider::Dotfiles => {
            if let Some(path) = path {
                let imported = vmux_tool::import_dotfiles(&path).map_err(io::Error::other)?;
                println!("imported {imported} dotfile package(s)");
            } else {
                let packages = vmux_tool::dotfile_packages().map_err(io::Error::other)?;
                let store = vmux_tool::ToolStore::current();
                let mut manifest = store.load().map_err(io::Error::other)?;
                let mut imported = 0;
                for package in packages {
                    imported += usize::from(!manifest.dotfiles.packages.contains(&package));
                    manifest.set_dotfile_package(&package, true);
                }
                store.save(&manifest).map_err(io::Error::other)?;
                println!("imported {imported} dotfile package(s)");
            }
        }
    }
    Ok(())
}

fn status() -> io::Result<()> {
    let store = vmux_tool::ToolStore::current();
    let manifest = store.load().map_err(io::Error::other)?;
    println!("{}", store.root().display());
    for (provider, packages) in &manifest.packages {
        println!("{provider} ({})", packages.len());
        for package in packages {
            println!("  {package}");
        }
    }
    if !manifest.mcp.servers.is_empty() {
        println!("mcp ({})", manifest.mcp.servers.len());
        for (name, server) in &manifest.mcp.servers {
            println!("  {name} · {:?}", server.transport);
        }
    }
    let mut packages = vmux_tool::dotfile_packages().map_err(io::Error::other)?;
    for package in &manifest.dotfiles.packages {
        if !packages.contains(package) {
            packages.push(package.clone());
        }
    }
    packages.sort();
    if !packages.is_empty() {
        println!("dotfiles ({})", packages.len());
    }
    for package in packages {
        let managed = manifest.dotfiles.packages.contains(&package);
        match vmux_tool::plan_dotfile_package(&package) {
            Ok(plan) => println!(
                "  {}{} · {} linked · {} missing · {} conflicts",
                package,
                if managed { " [managed]" } else { "" },
                plan.links
                    .iter()
                    .filter(|link| link.state == DotfileLinkState::Linked)
                    .count(),
                plan.links
                    .iter()
                    .filter(|link| link.state == DotfileLinkState::Missing)
                    .count(),
                plan.links
                    .iter()
                    .filter(|link| link.state == DotfileLinkState::Conflict)
                    .count(),
            ),
            Err(error) => println!("  {package} · {error}"),
        }
    }
    Ok(())
}
