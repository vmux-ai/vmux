use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;
use crate::execution::Execution;

pub struct Scope {
    root: PathBuf,
}

impl Scope {
    pub fn new(root: PathBuf) -> Result<Self> {
        Ok(Self {
            root: root.canonicalize()?,
        })
    }

    pub fn run(&self, args: &[OsString]) -> Result<()> {
        let comparison = Comparison::parse(args)?;
        let workspace = Workspace::load(&self.root)?;
        let selection = match comparison.files(&self.root)? {
            Some(files) => workspace.select(&files),
            None => workspace.full(),
        };
        selection.emit();
        Ok(())
    }
}

struct Comparison {
    base: Option<OsString>,
    head: OsString,
}

impl Comparison {
    fn parse(args: &[OsString]) -> Result<Self> {
        let mut comparison = Self {
            base: std::env::var_os("VMUX_CI_BASE").filter(|base| !base.is_empty()),
            head: "HEAD".into(),
        };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            let value = args.next().ok_or("Missing CI comparison reference")?;
            if value.is_empty() || value.to_string_lossy().starts_with('-') {
                return Err("Invalid CI comparison reference".into());
            }
            match arg.to_str() {
                Some("--base") => comparison.base = Some(value.clone()),
                Some("--head") => comparison.head = value.clone(),
                _ => return Err(format!("Unknown CI scope argument: {arg:?}").into()),
            }
        }
        Ok(comparison)
    }

    fn files(&self, root: &Path) -> Result<Option<Vec<PathBuf>>> {
        let Some(base) = &self.base else {
            return Ok(None);
        };
        let mut command = Command::new("git");
        command
            .args(["merge-base", "--"])
            .arg(base)
            .arg(&self.head)
            .current_dir(root);
        let output = Execution::new(command).output(None)?;
        let base = String::from_utf8(output.stdout)?;
        let mut command = Command::new("git");
        command
            .args(["diff", "--name-only", "--no-renames", "-z"])
            .arg(base.trim())
            .arg(&self.head)
            .arg("--")
            .current_dir(root);
        let output = Execution::new(command).output(None)?;
        let paths = String::from_utf8(output.stdout)?;
        let mut files = Vec::new();
        for path in paths.split('\0') {
            if !path.is_empty() {
                files.push(PathBuf::from(path));
            }
        }
        Ok(Some(files))
    }
}

struct Workspace {
    packages: Vec<Package>,
}

impl Workspace {
    fn load(root: &Path) -> Result<Self> {
        let mut command =
            Command::new(std::env::var_os("CARGO_BIN").unwrap_or_else(|| "cargo".into()));
        command
            .args(["metadata", "--locked", "--no-deps", "--format-version=1"])
            .current_dir(root);
        let metadata = Execution::new(command).output(None)?;
        let mut command = Command::new("jq");
        command.args([
            "-j",
            r#".packages[] | .name, "\u0000", .manifest_path, "\u0000", ([.dependencies[].name] | join(",")), "\u0000""#,
        ]);
        let output = Execution::new(command).output(Some(&metadata.stdout))?;
        Self::parse(root, &String::from_utf8(output.stdout)?)
    }

    fn parse(root: &Path, records: &str) -> Result<Self> {
        let mut fields = records.split_terminator('\0');
        let mut packages = Vec::new();
        while let Some(name) = fields.next() {
            if name.is_empty()
                || name.starts_with('-')
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            {
                return Err("Invalid CI package name".into());
            }
            let manifest = fields.next().ok_or("Missing CI package manifest")?;
            let dependencies = fields.next().ok_or("Missing CI package dependencies")?;
            let directory = Path::new(manifest)
                .parent()
                .ok_or("Invalid CI package manifest")?
                .strip_prefix(root)?
                .to_path_buf();
            let mut uses = BTreeSet::new();
            for name in dependencies.split(',') {
                if !name.is_empty() {
                    uses.insert(name.to_owned());
                }
            }
            packages.push(Package {
                name: name.to_owned(),
                directory,
                dependencies: uses,
            });
        }
        if packages.is_empty() {
            return Err("Cargo metadata returned no workspace packages".into());
        }
        if packages
            .iter()
            .all(|package| package.directory.starts_with("patches"))
        {
            return Err("Cargo metadata returned no owned workspace packages".into());
        }
        Ok(Self { packages })
    }

    fn full(&self) -> Selection {
        let mut lint = BTreeSet::new();
        for package in &self.packages {
            if !package.directory.starts_with("patches") {
                lint.insert(package.name.clone());
            }
        }
        Selection {
            packages: BTreeSet::new(),
            lint,
            workspace: true,
            lean: true,
        }
    }

    fn select(&self, files: &[PathBuf]) -> Selection {
        let mut selected = BTreeSet::new();
        let mut lean = false;
        let mut policy = false;
        for file in files {
            if file == Path::new("ecs-policy.ron") {
                if !self.packages.iter().any(|package| {
                    package.name == "vmux_macro"
                        && package.directory == Path::new("crates/util/vmux_macro")
                }) {
                    return self.full();
                }
                policy = true;
                continue;
            }
            if Selection::documentation(file) {
                continue;
            }
            if Selection::global(file) {
                return self.full();
            }
            let owner = self
                .packages
                .iter()
                .filter(|package| file.starts_with(&package.directory))
                .max_by_key(|package| package.directory.components().count());
            let Some(owner) = owner else {
                return self.full();
            };
            lean |= owner.name == "vmux_desktop";
            selected.insert(owner.name.clone());
        }
        if selected.is_empty() && !policy {
            return self.full();
        }
        loop {
            let mut added = false;
            for package in &self.packages {
                if !selected.contains(&package.name) && !package.dependencies.is_disjoint(&selected)
                {
                    selected.insert(package.name.clone());
                    added = true;
                }
            }
            if !added {
                break;
            }
        }
        if policy {
            selected.insert("vmux_macro".into());
        }
        let mut lint = BTreeSet::new();
        for package in &self.packages {
            if selected.contains(&package.name) && !package.directory.starts_with("patches") {
                lint.insert(package.name.clone());
            }
        }
        Selection {
            packages: selected,
            lint,
            workspace: false,
            lean,
        }
    }
}

struct Package {
    name: String,
    directory: PathBuf,
    dependencies: BTreeSet<String>,
}

struct Selection {
    packages: BTreeSet<String>,
    lint: BTreeSet<String>,
    workspace: bool,
    lean: bool,
}

impl Selection {
    fn documentation(path: &Path) -> bool {
        path.starts_with("docs")
            || (!path.starts_with("crates")
                && path.extension().is_some_and(|extension| extension == "md"))
    }

    fn global(path: &Path) -> bool {
        if Self::documentation(path) {
            return false;
        }
        if matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("Cargo.toml" | "Cargo.lock" | "build.rs")
        ) {
            return true;
        }
        for directory in [
            ".github",
            "scripts",
            "cef",
            "patches",
            "packaging",
            "crates/util/vmux_api",
            "crates/util/vmux_ecs",
            "crates/util/vmux_macro",
            "crates/util/vmux_app",
        ] {
            if path.starts_with(directory) {
                return true;
            }
        }
        path.parent()
            .is_none_or(|parent| parent.as_os_str().is_empty() || parent == Path::new("crates"))
    }

    fn emit(&self) {
        if self.workspace {
            eprintln!("CI test scope: full workspace");
            println!("args=--workspace");
        } else {
            eprintln!("CI test scope: {} packages", self.packages.len());
            print!("args=");
            let mut separator = "";
            for package in &self.packages {
                print!("{separator}-p {package}");
                separator = " ";
            }
            println!();
        }
        print!("lint_args=");
        let mut separator = "";
        for package in &self.lint {
            print!("{separator}-p {package}");
            separator = " ";
        }
        println!();
        println!("lean={}", self.lean);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Workspace {
        fn fixture() -> Self {
            Self::parse(
                Path::new("/repo"),
                concat!(
                    "leaf\0/repo/crates/feature/leaf/Cargo.toml\0consumer\0",
                    "consumer\0/repo/crates/feature/consumer/Cargo.toml\0leaf\0",
                    "app\0/repo/crates/app/app/Cargo.toml\0consumer\0",
                    "unrelated\0/repo/crates/feature/leaf-extra/Cargo.toml\0\0",
                    "vmux_desktop\0/repo/crates/app/vmux_desktop/Cargo.toml\0app\0",
                ),
            )
            .unwrap()
        }
    }

    #[test]
    fn policy_ledger_inputs_do_not_expand_the_production_dependency_graph() {
        let mut workspace = Workspace::fixture();
        workspace.packages.push(Package {
            name: "vmux_macro".into(),
            directory: "crates/util/vmux_macro".into(),
            dependencies: BTreeSet::new(),
        });
        for package in &mut workspace.packages {
            if package.name == "unrelated" {
                package.dependencies.insert("vmux_macro".into());
            }
        }
        let policy = workspace.select(&["ecs-policy.ron".into()]);
        assert_eq!(policy.packages, BTreeSet::from(["vmux_macro".into()]));
        assert_eq!(policy.lint, policy.packages);
        assert!(!policy.workspace);
        assert!(!policy.lean);
        let feature = workspace.select(&[
            "ecs-policy.ron".into(),
            "crates/feature/leaf/src/lib.rs".into(),
        ]);
        assert_eq!(
            feature.packages,
            BTreeSet::from([
                "app".into(),
                "consumer".into(),
                "leaf".into(),
                "vmux_desktop".into(),
                "vmux_macro".into(),
            ])
        );
        assert_eq!(feature.lint, feature.packages);
        assert!(!feature.workspace);
        assert!(!feature.lean);
        for path in ["crates/util/vmux_macro/src/lib.rs", "other-policy.ron"] {
            let selection = workspace.select(&["ecs-policy.ron".into(), path.into()]);
            assert!(selection.workspace, "{path}");
            assert!(selection.lean, "{path}");
        }
        assert!(
            Workspace::fixture()
                .select(&["ecs-policy.ron".into()])
                .workspace
        );
        for package in &mut workspace.packages {
            if package.name == "vmux_macro" {
                package.directory = "crates/feature/foreign-policy".into();
            }
        }
        assert!(workspace.select(&["ecs-policy.ron".into()]).workspace);
    }

    #[test]
    fn feature_changes_follow_cyclic_and_transitive_consumers_without_a_lean_rebuild() {
        let selection = Workspace::fixture().select(&["crates/feature/leaf/src/lib.rs".into()]);
        assert_eq!(
            selection.packages,
            BTreeSet::from([
                "app".into(),
                "consumer".into(),
                "leaf".into(),
                "vmux_desktop".into()
            ])
        );
        assert!(!selection.workspace);
        assert!(!selection.lean);
    }

    #[test]
    fn package_prefixes_do_not_capture_a_sibling_with_a_similar_name() {
        let selection = Workspace::fixture().select(&[
            "crates/feature/leaf-extra/src/lib.rs".into(),
            "docs/architecture.md".into(),
        ]);
        assert_eq!(selection.packages, BTreeSet::from(["unrelated".into()]));
        assert!(!selection.workspace);
    }

    #[test]
    fn global_and_unmapped_changes_require_workspace_and_lean_tests() {
        for path in [
            "Cargo.lock",
            "tool-versions.env",
            "crates/feature/leaf/Cargo.toml",
            "crates/feature/leaf/build.rs",
            "crates/build_platform_cfg.rs",
            "crates/util/vmux_ecs/src/lib.rs",
            ".github/workflows/ci.yml",
            "crates/feature/removed/src/lib.rs",
        ] {
            let selection = Workspace::fixture().select(&[path.into()]);
            assert!(selection.workspace, "{path}");
            assert!(selection.lean, "{path}");
        }
    }

    #[test]
    fn desktop_changes_keep_lean_feature_tests() {
        let selection = Workspace::fixture().select(&["crates/app/vmux_desktop/src/lib.rs".into()]);
        assert_eq!(selection.packages, BTreeSet::from(["vmux_desktop".into()]));
        assert!(selection.lean);
    }

    #[test]
    fn vendored_consumers_are_tested_without_formatting_or_linting_them() {
        let workspace = Workspace::parse(
            Path::new("/repo"),
            concat!(
                "owned\0/repo/crates/feature/owned/Cargo.toml\0\0",
                "vendor\0/repo/patches/vendor/Cargo.toml\0owned\0",
            ),
        )
        .unwrap();
        let selection = workspace.select(&["crates/feature/owned/src/lib.rs".into()]);
        assert_eq!(
            selection.packages,
            BTreeSet::from(["owned".into(), "vendor".into()])
        );
        assert_eq!(selection.lint, BTreeSet::from(["owned".into()]));
        assert_eq!(workspace.full().lint, BTreeSet::from(["owned".into()]));
    }
}
