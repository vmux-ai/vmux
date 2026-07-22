use std::io;
use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use vmux_profile::registry::{self, DotfileLinkState};

#[derive(Debug, Args)]
pub struct RegistryArgs {
    #[command(subcommand)]
    command: RegistryCommand,
}

#[derive(Debug, Subcommand)]
enum RegistryCommand {
    Status,
    Apply,
    Import {
        provider: RegistryImportProvider,
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
enum RegistryImportProvider {
    Homebrew,
    Npm,
    Mcp,
    Dotfiles,
}

pub fn run(args: RegistryArgs) -> io::Result<()> {
    match args.command {
        RegistryCommand::Status => status(),
        RegistryCommand::Apply => {
            let manifest = registry::load_manifest().map_err(io::Error::other)?;
            let linked = registry::apply_enabled_dotfiles(&manifest).map_err(io::Error::other)?;
            println!("linked {linked} file(s)");
            Ok(())
        }
        RegistryCommand::Import { provider, path } => import(provider, path),
        RegistryCommand::Adopt { path, package } => {
            let destination = registry::adopt_dotfile(&path, &package).map_err(io::Error::other)?;
            println!("{}", destination.display());
            Ok(())
        }
        RegistryCommand::Unlink { package } => {
            let removed = registry::unlink_dotfile_package(&package).map_err(io::Error::other)?;
            let mut manifest = registry::load_manifest().map_err(io::Error::other)?;
            manifest.set_dotfile_package(&package, false);
            registry::write_manifest(&manifest).map_err(io::Error::other)?;
            println!("unlinked {removed} file(s)");
            Ok(())
        }
    }
}

fn import(provider: RegistryImportProvider, path: Option<PathBuf>) -> io::Result<()> {
    match provider {
        RegistryImportProvider::Homebrew => {
            let path = path.ok_or_else(|| io::Error::other("Brewfile path is required"))?;
            let (formulae, casks) = registry::import_brewfile(&path).map_err(io::Error::other)?;
            println!("imported {formulae} formulae and {casks} casks");
        }
        RegistryImportProvider::Npm => {
            let path = path.ok_or_else(|| io::Error::other("package.json path is required"))?;
            let imported = registry::import_npm_manifest(&path).map_err(io::Error::other)?;
            println!("imported {imported} npm package(s)");
        }
        RegistryImportProvider::Mcp => {
            let imported = if let Some(path) = path {
                registry::import_mcp_config(&path)
            } else {
                registry::import_default_mcp_configs()
            }
            .map_err(io::Error::other)?;
            println!("imported {imported} MCP server(s)");
        }
        RegistryImportProvider::Dotfiles => {
            if let Some(path) = path {
                let imported = registry::import_dotfiles(&path).map_err(io::Error::other)?;
                println!("imported {imported} dotfile package(s)");
            } else {
                let packages = registry::dotfile_packages();
                let mut manifest = registry::load_manifest().map_err(io::Error::other)?;
                let mut imported = 0;
                for package in packages {
                    imported += usize::from(!manifest.dotfiles.packages.contains(&package));
                    manifest.set_dotfile_package(&package, true);
                }
                registry::write_manifest(&manifest).map_err(io::Error::other)?;
                println!("imported {imported} dotfile package(s)");
            }
        }
    }
    Ok(())
}

fn status() -> io::Result<()> {
    let manifest = registry::load_manifest().map_err(io::Error::other)?;
    println!("{}", registry::root_dir().display());
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
    let mut packages = registry::dotfile_packages();
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
        match registry::plan_dotfile_package(&package) {
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
