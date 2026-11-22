use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::Native;
use crate::Result;
use crate::execution::Execution;

impl Native {
    pub fn internal(&mut self, task: &str, args: &[OsString]) -> Result<()> {
        match task {
            "host-config" => {
                println!("{}", self.product()?.json());
                Ok(())
            }
            "package-binaries" | "before-package" | "finalize-package" | "verify-web-bundle"
            | "verify-bundle" => Err("Packaging requires its registered ECS systems".into()),
            "install-cef" => self.install_cef(),
            "build-cef" => self.build_cef(),
            "cef-manifest" => self.cef_manifest(
                args.first()
                    .and_then(|arg| arg.to_str())
                    .ok_or("Missing CEF metadata key")?,
            ),
            "validate-cef" => {
                let path = args.first().map(PathBuf::from).unwrap_or(self.framework()?);
                self.validate_cef(&path)
            }
            "install-helper" => self.helper(),
            "install-package-tools" => self.package_tools(),
            "install-dioxus" => self.install_dioxus(),
            "install-binaryen" => self.binaryen(),
            "install-tailwindcss" => self.tailwind(),
            "ios-resources" => self.resources_ios(),
            "ios-release" => self.ios_release(),
            "ios-upload" => self.ios_upload(),
            "ios-verify" => self.verify_ios(args.iter().any(|arg| arg == "--signed")),
            "extension-conformance" => self.conformance(),
            "setup-hooks" => self.hooks(),
            "target-seed-key" => {
                println!("{}", self.seed_key()?);
                Ok(())
            }
            "relocate-target" => {
                if args.len() != 3 {
                    return Err("relocate-target requires staging, source, and destination".into());
                }
                self.relocate(
                    std::path::Path::new(&args[0]),
                    std::path::Path::new(&args[1]),
                    std::path::Path::new(&args[2]),
                )
            }
            "nightly-version" => self.nightly(args),
            "release-artifacts" => self.release_artifacts(
                args.first()
                    .and_then(|arg| arg.to_str())
                    .ok_or("Missing release version")?,
            ),
            "update-release-metadata" => {
                self.release_metadata(args.iter().any(|arg| arg == "--check"))
            }
            "pre-push" => self.pre_push(),
            _ => Err(format!("Unknown external integration: {task}").into()),
        }
    }

    fn hooks(&self) -> Result<()> {
        let binary = self
            .checkout
            .tools_root(&self.environment)
            .join(".build-tools/bin/vmux");
        self.copy(&std::env::current_exe()?, &binary)?;
        let path = self.text(
            "git",
            &[
                OsStr::new("rev-parse"),
                OsStr::new("--path-format=absolute"),
                OsStr::new("--git-common-dir"),
            ],
        )?;
        let directory = PathBuf::from(path.trim()).join("hooks");
        fs::create_dir_all(&directory)?;
        let hook = directory.join("pre-push");
        fs::write(
            &hook,
            "#!/bin/sh\nexec \"$(git rev-parse --show-toplevel)/scripts/hooks/pre-push\" \"$@\"\n",
        )?;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn pre_push(&mut self) -> Result<()> {
        let target = self
            .checkout
            .tools_root(&self.environment)
            .join(".build-tools/cargo");
        self.environment.set("CARGO_TARGET_DIR", target.as_os_str());
        for args in [
            vec!["test", "-p", "vmux_dx", "--features", "ci-syntax", "--lib"],
            vec![
                "test",
                "-p",
                "vmux_cli",
                "--no-default-features",
                "--features",
                "dx,ci-syntax",
                "--test",
                "dx",
                "--",
                "--test-threads=1",
            ],
            vec!["fmt", "--all", "--", "--check"],
            vec![
                "test",
                "-p",
                "vmux_dx",
                "--test",
                "conventions",
                "--test",
                "policy",
            ],
        ] {
            let mut command = self.command(self.get("CARGO_BIN", "cargo"));
            command.env(
                "CARGO_INCREMENTAL",
                self.environment.incremental(Some(OsStr::new(args[0]))),
            );
            command.args(args);
            Execution::new(command).run()?;
        }
        self.run_command("git", &[OsStr::new("diff"), OsStr::new("--check")])
    }
}
