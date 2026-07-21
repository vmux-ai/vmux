use std::io;
use std::path::PathBuf;

use clap::{Args, Subcommand};
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
    Adopt {
        path: PathBuf,
        #[arg(long)]
        package: String,
    },
    Unlink {
        package: String,
    },
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

fn status() -> io::Result<()> {
    let manifest = registry::load_manifest().map_err(io::Error::other)?;
    println!("{}", registry::root_dir().display());
    for (provider, packages) in &manifest.packages {
        println!("{provider} ({})", packages.len());
        for package in packages {
            println!("  {package}");
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
