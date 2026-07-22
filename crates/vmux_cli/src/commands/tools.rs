use std::io;
use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use vmux_profile::tools::{self, DotfileLinkState};

#[derive(Debug, Args)]
pub struct ToolsArgs {
    #[command(subcommand)]
    command: ToolsCommand,
}

#[derive(Debug, Subcommand)]
enum ToolsCommand {
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

pub fn run(args: ToolsArgs) -> io::Result<()> {
    match args.command {
        ToolsCommand::Status => status(),
        ToolsCommand::Apply => {
            let manifest = tools::load_manifest().map_err(io::Error::other)?;
            let linked = tools::apply_enabled_dotfiles(&manifest).map_err(io::Error::other)?;
            println!("linked {linked} file(s)");
            Ok(())
        }
        ToolsCommand::Import { provider, path } => import(provider, path),
        ToolsCommand::Adopt { path, package } => {
            let destination = tools::adopt_dotfile(&path, &package).map_err(io::Error::other)?;
            println!("{}", destination.display());
            Ok(())
        }
        ToolsCommand::Unlink { package } => {
            let removed = tools::unlink_dotfile_package(&package).map_err(io::Error::other)?;
            let mut manifest = tools::load_manifest().map_err(io::Error::other)?;
            manifest.set_dotfile_package(&package, false);
            tools::write_manifest(&manifest).map_err(io::Error::other)?;
            println!("unlinked {removed} file(s)");
            Ok(())
        }
    }
}

fn import(provider: ToolImportProvider, path: Option<PathBuf>) -> io::Result<()> {
    match provider {
        ToolImportProvider::Homebrew => {
            let path = path.ok_or_else(|| io::Error::other("Brewfile path is required"))?;
            let (formulae, casks) = tools::import_brewfile(&path).map_err(io::Error::other)?;
            println!("imported {formulae} formulae and {casks} casks");
        }
        ToolImportProvider::Npm => {
            let path = path.ok_or_else(|| io::Error::other("package.json path is required"))?;
            let imported = tools::import_npm_manifest(&path).map_err(io::Error::other)?;
            println!("imported {imported} npm package(s)");
        }
        ToolImportProvider::Mcp => {
            let imported = if let Some(path) = path {
                tools::import_mcp_config(&path)
            } else {
                tools::import_default_mcp_configs()
            }
            .map_err(io::Error::other)?;
            println!("imported {imported} MCP server(s)");
        }
        ToolImportProvider::Dotfiles => {
            if let Some(path) = path {
                let imported = tools::import_dotfiles(&path).map_err(io::Error::other)?;
                println!("imported {imported} dotfile package(s)");
            } else {
                let packages = tools::dotfile_packages().map_err(io::Error::other)?;
                let mut manifest = tools::load_manifest().map_err(io::Error::other)?;
                let mut imported = 0;
                for package in packages {
                    imported += usize::from(!manifest.dotfiles.packages.contains(&package));
                    manifest.set_dotfile_package(&package, true);
                }
                tools::write_manifest(&manifest).map_err(io::Error::other)?;
                println!("imported {imported} dotfile package(s)");
            }
        }
    }
    Ok(())
}

fn status() -> io::Result<()> {
    let manifest = tools::load_manifest().map_err(io::Error::other)?;
    println!("{}", tools::root_dir().display());
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
    let mut packages = tools::dotfile_packages().map_err(io::Error::other)?;
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
        match tools::plan_dotfile_package(&package) {
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
