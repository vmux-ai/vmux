use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;
use crate::execution::Execution;
use crate::temporary::Temporary;

pub struct Consumer {
    root: PathBuf,
    target: PathBuf,
}

impl Consumer {
    pub fn new(root: PathBuf, target: Option<PathBuf>) -> Result<Self> {
        let root = root.canonicalize()?;
        let target = target.or_else(|| std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from));
        let target = match target {
            Some(target) if target.is_absolute() => target,
            Some(target) => root.join(target),
            None => root.join("target"),
        };
        Ok(Self { root, target })
    }

    pub fn run(&self, args: &[OsString]) -> Result<()> {
        if args.is_empty() {
            self.core()?;
            return self.workspace(
                &self.root.join("examples/framework-consumer"),
                "framework-consumer",
                None,
            );
        }
        if args.len() != 1 || args[0] != "--git" {
            return Err("usage: framework-consumer [--git]".into());
        }
        let revision = std::env::var("VMUX_SDK_REVISION").unwrap_or_else(|_| "HEAD".into());
        let revision = self.git(&["rev-parse", "--verify", &format!("{revision}^{{commit}}")])?;
        let repository = match std::env::var("VMUX_SDK_REPOSITORY") {
            Ok(repository) => repository,
            Err(_) => format!(
                "file://{}",
                self.git(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?
            ),
        };
        let temporary = Temporary::create(&std::env::temp_dir(), "vmux-git-consumer")?;
        let source = self.root.join("examples/framework-consumer");
        Directory::copy(&source.join("src"), &temporary.path().join("src"))?;
        Directory::copy(&source.join("tests"), &temporary.path().join("tests"))?;
        fs::copy(
            source.join("Cargo.lock"),
            temporary.path().join("Cargo.lock"),
        )?;
        let manifest = GitManifest::pin(
            &fs::read_to_string(source.join("Cargo.toml"))?,
            &repository,
            &revision,
        )?;
        fs::write(temporary.path().join("Cargo.toml"), manifest)?;
        let mut command = self.cargo(&["metadata", "--format-version", "1"]);
        command
            .arg("--manifest-path")
            .arg(temporary.path().join("Cargo.toml"));
        Execution::new(command).output(None)?;
        self.workspace(temporary.path(), "framework-git-consumer", Some(&revision))
    }

    fn core(&self) -> Result<()> {
        for (package, features, lib) in [
            ("vmux_ecs", "", true),
            ("vmux_ecs", "contracts", true),
            ("vmux_ecs", "store", true),
            ("vmux_ecs", "jobs", true),
            ("vmux_project", "contracts,filesystem", true),
            ("vmux_flex", "", false),
            ("vmux_flex", "window", true),
            ("vmux_pane", "", true),
            ("vmux_pane", "store", true),
        ] {
            let mut command =
                self.cargo(&["test", "--locked", "-p", package, "--no-default-features"]);
            if !features.is_empty() {
                command.args(["--features", features]);
            }
            if lib {
                command.arg("--lib");
            }
            Execution::new(command).run()?;
        }
        Ok(())
    }

    fn workspace(&self, directory: &Path, target: &str, revision: Option<&str>) -> Result<()> {
        let manifest = directory.join("Cargo.toml");
        let mut command = self.cargo(&["test", "--locked"]);
        command
            .arg("--manifest-path")
            .arg(&manifest)
            .arg("--target-dir")
            .arg(self.target.join(target));
        Execution::new(command).run()?;
        let mut command = self.cargo(&["tree", "--locked", "--prefix", "none", "--format", "{p}"]);
        command.arg("--manifest-path").arg(&manifest);
        let output = Execution::new(command).output(None)?;
        Graph::check(&String::from_utf8(output.stdout)?)?;
        if let Some(revision) = revision {
            GitLock::check(&fs::read_to_string(directory.join("Cargo.lock"))?, revision)?;
            println!("All SDK packages use the same immutable Git revision");
        }
        println!("Framework consumer uses the public core without Profile or UI dependencies");
        Ok(())
    }

    fn cargo(&self, args: &[&str]) -> Command {
        let mut command =
            Command::new(std::env::var_os("CARGO_BIN").unwrap_or_else(|| "cargo".into()));
        command.current_dir(&self.root).args(args);
        command
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        let mut command = Command::new("git");
        command.current_dir(&self.root).args(args);
        let output = Execution::new(command).output(None)?;
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
}

struct Directory;

impl Directory {
    fn copy(source: &Path, destination: &Path) -> Result<()> {
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let target = destination.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                Self::copy(&entry.path(), &target)?;
            } else {
                fs::copy(entry.path(), target)?;
            }
        }
        Ok(())
    }
}

struct GitManifest;

impl GitManifest {
    fn pin(source: &str, repository: &str, revision: &str) -> Result<String> {
        let repository = Self::quoted(repository);
        let revision = Self::quoted(revision);
        let mut result = String::new();
        let mut pinned = 0;
        for line in source.lines() {
            let Some((name, value)) = line.split_once('=') else {
                result.push_str(line);
                result.push('\n');
                continue;
            };
            if !name.trim().starts_with("vmux_") {
                result.push_str(line);
                result.push('\n');
                continue;
            }
            let Some(path) = value.trim().strip_prefix("{ path = \"") else {
                return Err(format!(
                    "SDK dependency {} must be an inline path dependency",
                    name.trim()
                )
                .into());
            };
            let Some((_, rest)) = path.split_once('"') else {
                return Err("Invalid SDK path dependency".into());
            };
            result.push_str(&format!(
                "{}= {{ git = {repository}, rev = {revision}{rest}\n",
                name
            ));
            pinned += 1;
        }
        if pinned == 0 {
            return Err("The consumer must declare SDK dependencies".into());
        }
        Ok(result)
    }

    fn quoted(value: &str) -> String {
        let mut result = String::from("\"");
        for character in value.chars() {
            match character {
                '"' => result.push_str("\\\""),
                '\\' => result.push_str("\\\\"),
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                other => result.push(other),
            }
        }
        result.push('"');
        result
    }
}

struct Graph;

impl Graph {
    fn check(tree: &str) -> Result<()> {
        let mut packages = HashSet::new();
        for line in tree.lines() {
            if let Some(package) = line.split_whitespace().next() {
                packages.insert(package);
            }
        }
        for required in [
            "vmux_app",
            "vmux_ecs",
            "vmux_project",
            "vmux_flex",
            "vmux_pane",
        ] {
            if !packages.contains(required) {
                return Err(format!("The consumer must exercise SDK package {required}").into());
            }
        }
        for forbidden in [
            "bevy",
            "bevy_cef",
            "bevy_cef_core",
            "bevy_winit",
            "vmux_profile",
            "vmux_editor",
            "vmux_ui",
            "vmux_layout",
            "wry",
        ] {
            if packages.contains(forbidden) {
                return Err(format!(
                    "Framework consumer links native application dependency {forbidden}"
                )
                .into());
            }
        }
        Ok(())
    }
}

struct GitLock;

impl GitLock {
    fn check(lock: &str, revision: &str) -> Result<()> {
        let mut name = None;
        let mut source = None;
        for line in lock.lines().chain(std::iter::once("[[package]]")) {
            if line == "[[package]]" {
                if let Some(name) = name.take() {
                    Self::package(name, source.take(), revision)?;
                }
            } else if let Some(value) = line
                .strip_prefix("name = \"")
                .and_then(|value| value.strip_suffix('"'))
            {
                name = Some(value);
            } else if let Some(value) = line
                .strip_prefix("source = \"")
                .and_then(|value| value.strip_suffix('"'))
            {
                source = Some(value);
            }
        }
        Ok(())
    }

    fn package(name: &str, source: Option<&str>, revision: &str) -> Result<()> {
        if name.starts_with("vmux_")
            && !source.is_some_and(|source| {
                source.starts_with("git+") && source.ends_with(&format!("#{revision}"))
            })
        {
            return Err(
                format!("SDK package {name} is not pinned to the tested Git revision").into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinning_preserves_features_and_refuses_unsupported_sdk_dependencies() {
        let manifest = "vmux_app = { path = \"../app\", features = [\"ecs\", \"project\"] }\n";
        assert_eq!(
            GitManifest::pin(manifest, "file:///repo", "revision").unwrap(),
            "vmux_app = { git = \"file:///repo\", rev = \"revision\", features = [\"ecs\", \"project\"] }\n"
        );
        assert!(GitManifest::pin("vmux_app = \"1\"\n", "file:///repo", "revision").is_err());
    }

    #[test]
    fn graph_refuses_native_packages_even_with_the_complete_sdk_surface() {
        let graph = "vmux_app v1\nvmux_ecs v1\nvmux_project v1\nvmux_flex v1\nvmux_pane v1\n";
        assert!(Graph::check(graph).is_ok());
        assert!(Graph::check(&format!("{graph}bevy_cef v1\n")).is_err());
    }

    #[test]
    fn git_lock_refuses_path_sources_and_mixed_sdk_revisions() {
        let lock = "[[package]]\nname = \"vmux_app\"\nsource = \"git+file:///repo#revision\"\n";
        assert!(GitLock::check(lock, "revision").is_ok());
        assert!(GitLock::check(lock, "other").is_err());
        assert!(GitLock::check("[[package]]\nname = \"vmux_app\"\n", "revision").is_err());
    }
}
