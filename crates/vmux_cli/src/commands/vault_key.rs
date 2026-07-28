use std::io::{self, Read};

use clap::{Args, ValueEnum};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum VaultKeyAction {
    Load,
    Migrate,
    Store,
}

#[derive(Debug, Args)]
pub struct VaultKeyArgs {
    #[arg(value_enum)]
    action: VaultKeyAction,
    #[arg(long)]
    vault_id: String,
    #[arg(long, hide = true)]
    no_ui: bool,
}

pub fn run(args: VaultKeyArgs) -> io::Result<i32> {
    if matches!(args.action, VaultKeyAction::Migrate) {
        return match vmux_profile::vault::migrate_legacy_key(&args.vault_id) {
            Ok(true) => Ok(0),
            Ok(false) => Ok(2),
            Err(error) => {
                eprintln!("{error}");
                Ok(1)
            }
        };
    }
    if let Err(error) = vmux_profile::vault::authorize_key_broker_parent() {
        eprintln!("{error}");
        return Ok(1);
    }
    let result = match args.action {
        VaultKeyAction::Load if args.no_ui => {
            vmux_profile::vault::key_broker_load_silent(&args.vault_id)
        }
        VaultKeyAction::Load => vmux_profile::vault::key_broker_load(&args.vault_id),
        VaultKeyAction::Migrate => unreachable!(),
        VaultKeyAction::Store => {
            let mut key = String::new();
            io::stdin().read_to_string(&mut key)?;
            vmux_profile::vault::key_broker_store(&args.vault_id, key.trim())
                .map(|_| Some(String::new()))
        }
    };
    match result {
        Ok(Some(value)) => {
            println!("{value}");
            Ok(0)
        }
        Ok(None) => Ok(2),
        Err(error) => {
            eprintln!("{error}");
            Ok(1)
        }
    }
}
