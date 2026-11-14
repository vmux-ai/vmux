use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::Result;
use crate::environment::Environment;
use crate::execution::Execution;
use crate::repository::Checkout;
use crate::source::Source;
use crate::storage::Storage;
use crate::target_lock::TargetLock;

pub struct Tooling;

impl Tooling {
    pub fn ensure(checkout: &Checkout, environment: &Environment) -> Result<()> {
        if !checkout
            .root
            .join("crates/feature/vmux_dx/Cargo.toml")
            .is_file()
        {
            return Ok(());
        }
        if Source::key(&checkout.root)? == env!("VMUX_DX_SOURCE_KEY") {
            return Ok(());
        }
        if environment.present("VMUX_DX_HANDOFF") {
            return Err("The refreshed CLI still differs from its checkout; compiler inputs changed during bootstrap".into());
        }
        let cache = checkout.root.join(".build-tools");
        if std::fs::symlink_metadata(&cache).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err("Development tool cache must not be a symlink".into());
        }
        std::fs::create_dir_all(&cache)?;
        let target = checkout.root.join(".build-tools/cargo");
        let storage = Storage::new(
            checkout.root.clone(),
            target.clone(),
            crate::policy::Policy::configured(environment)?,
        )?;
        storage.guard()?;
        let lock = TargetLock::acquire(&checkout.root, &target)?;
        storage.guard()?;
        let mut command = environment.command(
            environment
                .get("CARGO_BIN")
                .unwrap_or(std::ffi::OsStr::new("cargo")),
            &checkout.root,
        );
        command
            .args([
                "build",
                "--locked",
                "-p",
                "vmux_cli",
                "--no-default-features",
                "--features",
                "dx,ci-syntax",
                "--target-dir",
            ])
            .arg(&target);
        command
            .env_remove("CARGO_BUILD_TARGET")
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_ENCODED_RUSTFLAGS");
        eprintln!("Updating the development CLI for the selected checkout");
        Execution::new(command).run()?;
        let binary = target.join("debug/vmux");
        if !binary.is_file() {
            return Err("Development CLI build produced no executable".into());
        }
        drop(lock);
        let error = Command::new(binary)
            .args(std::env::args_os().skip(1))
            .env("VMUX_DX_HANDOFF", "1")
            .exec();
        Err(error.into())
    }
}
