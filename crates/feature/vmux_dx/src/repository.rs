use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bevy_ecs::prelude::*;

use crate::Result;

#[derive(Component, Clone, Debug)]
pub struct Checkout {
    pub root: PathBuf,
    pub target: PathBuf,
    pub build_target: Option<OsString>,
}

impl Checkout {
    pub fn title(&self, profile: &str, product: &str) -> Result<String> {
        if profile == "release" {
            return Ok(product.into());
        }
        let mut source = String::new();
        for arguments in [
            ["symbolic-ref", "--quiet", "--short", "HEAD"].as_slice(),
            ["rev-parse", "--short=7", "HEAD"].as_slice(),
        ] {
            let mut command = crate::environment::Environment::current().command("git", &self.root);
            command.args(arguments);
            let output = command.output()?;
            if output.status.success() {
                source = std::str::from_utf8(&output.stdout)?.trim().to_owned();
                if !source.is_empty() {
                    break;
                }
            }
        }
        if source.is_empty() {
            source = "unknown".into();
        }
        Ok(match profile {
            "dev" => format!("{product} Dev ({source})"),
            "local" => format!("{product} ({source})"),
            other => format!("{product} ({other})"),
        })
    }

    pub fn resolve(path: &Path, branch: Option<&OsStr>) -> Result<Self> {
        let mut root = path.canonicalize()?;
        loop {
            if root.join("crates/app/vmux_cli/Cargo.toml").is_file()
                || root.join(".gitmodules").is_file() && root.join("Cargo.toml").is_file()
            {
                break;
            }
            if !root.pop() {
                return Err("Not inside a Vmux repository; use --root PATH".into());
            }
        }
        let cloud = root.join(".gitmodules").is_file();
        if let Some(branch) = branch {
            let candidates = Self::worktrees(&root)?;
            if cloud {
                if let Some(selected) = Self::exact(&candidates, branch)? {
                    root = selected;
                } else {
                    let mut clients = Vec::new();
                    let mut seen = std::collections::BTreeSet::new();
                    for candidate in &candidates {
                        let client = candidate.path.join("client");
                        if !client.join(".git").exists() {
                            continue;
                        }
                        for mut worktree in Self::worktrees(&client)? {
                            if !worktree.path.join(".git").exists() {
                                continue;
                            }
                            worktree.path = worktree.path.canonicalize()?;
                            if seen.insert(worktree.path.clone()) {
                                clients.push(worktree);
                            }
                        }
                    }
                    root = match Self::exact(&clients, branch)? {
                        Some(selected) => Some(selected),
                        None => match Self::alias(&candidates, branch)? {
                            Some(selected) => Some(selected),
                            None => Self::alias(&clients, branch)?,
                        },
                    }
                    .ok_or("Branch has no worktree; create its worktree explicitly first")?;
                }
            } else {
                root = Self::select(&candidates, branch)?
                    .ok_or("Branch has no worktree; create its worktree explicitly first")?;
            }
        }
        if !root.join("crates/app/vmux_cli/Cargo.toml").is_file() {
            root.push("client");
        }
        if !root.join("crates/app/vmux_cli/Cargo.toml").is_file() {
            return Err("Selected checkout has no initialized Vmux client".into());
        }
        root = root.canonicalize()?;
        let setting = std::env::var_os("CARGO_TARGET_DIR")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "target".into());
        let target = if Path::new(&setting).is_absolute() {
            PathBuf::from(setting)
        } else {
            root.join(setting)
        };
        let target = std::path::absolute(target)?;
        fs::create_dir_all(target.parent().ok_or("Missing target parent")?)?;
        let target = target
            .parent()
            .unwrap()
            .canonicalize()?
            .join(target.file_name().ok_or("Missing target name")?);
        if target == root || root.starts_with(&target) {
            return Err("Cargo target must be outside the repository source root".into());
        }
        Ok(Self {
            root,
            target,
            build_target: std::env::var_os("CARGO_BUILD_TARGET").filter(|value| !value.is_empty()),
        })
    }

    fn worktrees(root: &Path) -> Result<Vec<Worktree>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["worktree", "list", "--porcelain", "-z"])
            .output()?;
        if !output.status.success() {
            return Err("Cannot list repository worktrees".into());
        }
        let mut worktrees = Worktree::parse(&output.stdout)?;
        let common = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
            .output()?;
        if !common.status.success() {
            return Err("Cannot resolve Git data directory".into());
        }
        let common = PathBuf::from(std::str::from_utf8(&common.stdout)?.trim());
        let setting = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["config", "--local", "--get", "core.worktree"])
            .output()?;
        if setting.status.success() {
            let path = common
                .join(std::str::from_utf8(&setting.stdout)?.trim())
                .canonicalize()?;
            for worktree in &mut worktrees {
                if worktree.path == common {
                    worktree.path = path.clone();
                }
            }
        }
        Ok(worktrees)
    }

    fn select(worktrees: &[Worktree], name: &OsStr) -> Result<Option<PathBuf>> {
        match Self::exact(worktrees, name)? {
            Some(path) => Ok(Some(path)),
            None => Self::alias(worktrees, name),
        }
    }

    fn exact(worktrees: &[Worktree], name: &OsStr) -> Result<Option<PathBuf>> {
        let mut exact = None;
        for worktree in worktrees {
            if worktree.branch.as_deref() == Some(name)
                && exact.replace(worktree.path.clone()).is_some()
            {
                return Err("Branch matches multiple worktrees".into());
            }
        }
        Ok(exact)
    }

    fn alias(worktrees: &[Worktree], name: &OsStr) -> Result<Option<PathBuf>> {
        let mut alias = None;
        for worktree in worktrees {
            if worktree.path.file_name() == Some(name)
                && alias.replace(worktree.path.clone()).is_some()
            {
                return Err("Worktree alias is ambiguous; use the full branch name".into());
            }
        }
        Ok(alias)
    }

    pub fn directory(&self, profile: &str) -> PathBuf {
        let mut path = self.target.clone();
        if let Some(target) = &self.build_target {
            path.push(target);
        }
        path.join(profile)
    }

    pub fn configure(
        &mut self,
        target: Option<&OsStr>,
        build_target: Option<&OsStr>,
    ) -> Result<()> {
        let target = target
            .filter(|value| !value.is_empty())
            .unwrap_or(OsStr::new("target"));
        let path = if Path::new(target).is_absolute() {
            PathBuf::from(target)
        } else {
            self.root.join(target)
        };
        let path = std::path::absolute(path)?;
        let parent = path.parent().ok_or("Missing target parent")?;
        fs::create_dir_all(parent)?;
        let path = parent
            .canonicalize()?
            .join(path.file_name().ok_or("Missing target name")?);
        if path == self.root || self.root.starts_with(&path) {
            return Err("Cargo target overlaps the source repository".into());
        }
        self.target = path;
        self.build_target = build_target
            .filter(|value| !value.is_empty())
            .map(OsStr::to_owned);
        Ok(())
    }
}

struct Worktree {
    path: PathBuf,
    branch: Option<OsString>,
}

impl Worktree {
    fn parse(bytes: &[u8]) -> Result<Vec<Self>> {
        let text = std::str::from_utf8(bytes)?;
        let mut worktrees = Vec::new();
        let mut path = None;
        let mut branch = None;
        for field in text.split('\0') {
            if let Some(value) = field.strip_prefix("worktree ") {
                path = Some(PathBuf::from(value));
            } else if let Some(value) = field.strip_prefix("branch refs/heads/") {
                branch = Some(value.into());
            } else if field.is_empty()
                && let Some(path) = path.take()
            {
                worktrees.push(Self {
                    path,
                    branch: branch.take(),
                });
            }
        }
        Ok(worktrees)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        directory: tempfile::TempDir,
        root: PathBuf,
    }

    impl Fixture {
        fn cloud() -> Self {
            let fixture = Self::new();
            fs::remove_dir_all(fixture.root.join("crates")).unwrap();
            fs::write(fixture.root.join(".gitmodules"), "").unwrap();
            fs::write(
                fixture.root.join("Cargo.toml"),
                "[workspace]\nmembers = []\n",
            )
            .unwrap();
            fixture.git(&["add", "-A"]);
            fixture.git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "Cloud fixture",
            ]);
            fixture
        }

        fn client(&self, cloud: &Path, branch: &str) -> PathBuf {
            let client = cloud.join("client");
            fs::create_dir_all(client.join("crates/app/vmux_cli")).unwrap();
            fs::write(
                client.join("crates/app/vmux_cli/Cargo.toml"),
                "[package]\nname = \"vmux_cli\"\n",
            )
            .unwrap();
            for arguments in [
                vec!["init", "-b", branch],
                vec!["add", "."],
                vec![
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-m",
                    "Client fixture",
                ],
            ] {
                let output = Command::new("git")
                    .arg("-C")
                    .arg(&client)
                    .args(arguments)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            client.canonicalize().unwrap()
        }
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().join("repository");
            fs::create_dir_all(root.join("crates/app/vmux_cli")).unwrap();
            fs::write(
                root.join("crates/app/vmux_cli/Cargo.toml"),
                "[package]\nname = \"vmux_cli\"\n",
            )
            .unwrap();
            let fixture = Self { directory, root };
            fixture.git(&["init", "-b", "main"]);
            fixture.git(&["add", "."]);
            fixture.git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "Fixture",
            ]);
            fixture
        }

        fn git(&self, args: &[&str]) {
            let output = Command::new("git")
                .arg("-C")
                .arg(&self.root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        fn worktree(&self, branch: &str, relative: &str) -> PathBuf {
            let path = self.directory.path().join(relative);
            self.git(&["worktree", "add", "-b", branch, path.to_str().unwrap()]);
            path.canonicalize().unwrap()
        }
    }

    #[test]
    fn cloud_selection_discovers_client_branches_in_other_cloud_worktrees() {
        let fixture = Fixture::cloud();
        fixture.client(&fixture.root, "client-main");
        let cloud = fixture.worktree("cloud-topic", "different-cloud-directory");
        let client = fixture.client(&cloud, "client-topic");
        assert_eq!(
            Checkout::resolve(&fixture.root, Some(OsStr::new("client-topic")))
                .unwrap()
                .root,
            client
        );
        let standalone = client.join(".worktrees/standalone-directory");
        let output = Command::new("git")
            .arg("-C")
            .arg(&client)
            .args(["worktree", "add", "-b", "client-standalone"])
            .arg(&standalone)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            Checkout::resolve(&fixture.root, Some(OsStr::new("client-standalone")))
                .unwrap()
                .root,
            standalone.canonicalize().unwrap()
        );
    }

    #[test]
    fn cloud_branch_precedes_client_branch_and_client_branch_precedes_directory_alias() {
        let fixture = Fixture::cloud();
        let client = fixture.client(&fixture.root, "navigation");
        fixture.worktree("other", "navigation");
        assert_eq!(
            Checkout::resolve(&fixture.root, Some(OsStr::new("navigation")))
                .unwrap()
                .root,
            client
        );
        let selected = fixture.worktree("navigation", "cloud-priority");
        let selected_client = fixture.client(&selected, "unrelated-client-name");
        assert_eq!(
            Checkout::resolve(&fixture.root, Some(OsStr::new("navigation")))
                .unwrap()
                .root,
            selected_client
        );
    }

    #[test]
    fn ambiguous_client_branches_across_cloud_worktrees_are_refused() {
        let fixture = Fixture::cloud();
        fixture.client(&fixture.root, "client-main");
        for (branch, directory) in [("first", "first-cloud"), ("second", "second-cloud")] {
            let cloud = fixture.worktree(branch, directory);
            fixture.client(&cloud, "shared-client-branch");
        }
        assert!(
            Checkout::resolve(&fixture.root, Some(OsStr::new("shared-client-branch")))
                .unwrap_err()
                .to_string()
                .contains("multiple")
        );
    }

    #[test]
    fn build_titles_follow_the_selected_branch_and_detached_revision() {
        let fixture = Fixture::new();
        let worktree = fixture.worktree("fixture/topic", "different-directory");
        let checkout = Checkout::resolve(&worktree, None).unwrap();
        assert_eq!(
            checkout.title("dev", "Vmux").unwrap(),
            "Vmux Dev (fixture/topic)"
        );
        assert_eq!(
            checkout.title("local", "Vmux").unwrap(),
            "Vmux (fixture/topic)"
        );
        assert_eq!(checkout.title("release", "Vmux").unwrap(), "Vmux");
        let status = Command::new("git")
            .arg("-C")
            .arg(&worktree)
            .args(["branch", "-m", "fixture/renamed"])
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            checkout.title("dev", "Vmux").unwrap(),
            "Vmux Dev (fixture/renamed)"
        );
        let status = Command::new("git")
            .arg("-C")
            .arg(&worktree)
            .args(["checkout", "--detach"])
            .output()
            .unwrap()
            .status;
        assert!(status.success());
        let revision = Command::new("git")
            .arg("-C")
            .arg(&worktree)
            .args(["rev-parse", "--short=7", "HEAD"])
            .output()
            .unwrap();
        assert!(revision.status.success());
        assert_eq!(
            checkout.title("local", "Vmux").unwrap(),
            format!(
                "Vmux ({})",
                std::str::from_utf8(&revision.stdout).unwrap().trim()
            )
        );
    }

    #[test]
    fn branch_name_resolves_a_differently_named_worktree_and_directory_alias_remains_available() {
        let fixture = Fixture::new();
        let worktree = fixture.worktree("topic/navigation", "checkout-with-another-name");
        assert_eq!(
            Checkout::resolve(&fixture.root, Some(OsStr::new("topic/navigation")))
                .unwrap()
                .root,
            worktree
        );
        assert_eq!(
            Checkout::resolve(
                &fixture.root,
                Some(OsStr::new("checkout-with-another-name"))
            )
            .unwrap()
            .root,
            worktree
        );
        let head = Command::new("git")
            .arg("-C")
            .arg(&fixture.root)
            .args(["branch", "--show-current"])
            .output()
            .unwrap();
        assert_eq!(std::str::from_utf8(&head.stdout).unwrap().trim(), "main");
    }

    #[test]
    fn exact_branch_precedes_an_alias_and_ambiguous_directory_names_are_refused() {
        let fixture = Fixture::new();
        fixture.worktree("other", "navigation");
        let branch = fixture.worktree("navigation", "different-directory");
        assert_eq!(
            Checkout::resolve(&fixture.root, Some(OsStr::new("navigation")))
                .unwrap()
                .root,
            branch
        );
        fixture.worktree("first", "one/shared");
        fixture.worktree("second", "two/shared");
        assert!(
            Checkout::resolve(&fixture.root, Some(OsStr::new("shared")))
                .unwrap_err()
                .to_string()
                .contains("ambiguous")
        );
    }

    #[test]
    fn an_unchecked_out_branch_is_refused_and_no_selector_uses_the_current_checkout() {
        let fixture = Fixture::new();
        fixture.git(&["branch", "idle"]);
        assert!(Checkout::resolve(&fixture.root, Some(OsStr::new("idle"))).is_err());
        assert_eq!(
            Checkout::resolve(&fixture.root, None).unwrap().root,
            fixture.root.canonicalize().unwrap()
        );
    }
}
