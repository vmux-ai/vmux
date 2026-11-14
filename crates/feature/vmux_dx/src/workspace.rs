use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy_ecs::prelude::*;
use serde::Deserialize;
use vmux_ecs::cli::{CliCommandManifest, CliInvocation};
use vmux_ecs::manifest::FeatureManifest;

use crate::Result;
use crate::repository::Checkout;
use crate::temporary::Temporary;

#[derive(Component, Clone)]
pub struct Provider {
    pub root: PathBuf,
    pub package: String,
    pub binary: String,
    pub cargo: PathBuf,
    prefix: String,
}

impl Provider {
    pub fn selected(name: &OsStr, arguments: impl Iterator<Item = OsString>) -> bool {
        match Self::definitions(arguments) {
            Ok(Some((_, declaration))) => declaration
                .providers
                .iter()
                .any(|provider| OsStr::new(&provider.name) == name),
            _ => false,
        }
    }

    pub fn discover(
        arguments: impl Iterator<Item = OsString>,
    ) -> Result<Vec<(Self, FeatureManifest)>> {
        let Some((root, declaration)) = Self::definitions(arguments)? else {
            return Ok(Vec::new());
        };
        let mut providers = Vec::new();
        for definition in declaration.providers {
            providers.push(Self::read(&root, definition)?);
        }
        Ok(providers)
    }

    fn definitions(
        arguments: impl Iterator<Item = OsString>,
    ) -> Result<Option<(PathBuf, Declaration)>> {
        let mut root = std::env::current_dir()?;
        let mut arguments = arguments;
        while let Some(argument) = arguments.next() {
            if argument == "--" {
                break;
            }
            if argument == "--root" {
                root = PathBuf::from(arguments.next().ok_or("--root requires a path")?);
            } else if let Some(value) = argument
                .to_str()
                .and_then(|value| value.strip_prefix("--root="))
            {
                root = PathBuf::from(value);
            }
        }
        root = root.canonicalize()?;
        loop {
            let cargo = root.join("Cargo.toml");
            if cargo.is_file() {
                let value: toml::Value = toml::from_str(&Self::text(&cargo)?)?;
                if let Some(configuration) = value
                    .get("workspace")
                    .and_then(|value| value.get("metadata"))
                    .and_then(|value| value.get("vmux"))
                    .and_then(|value| value.get("dx"))
                {
                    let declaration: Declaration = configuration.clone().try_into()?;
                    if declaration.providers.len() > 16 {
                        return Err("Workspace declares too many CLI providers".into());
                    }
                    return Ok(Some((root, declaration)));
                }
            }
            if root.join(".git").exists() {
                return Ok(None);
            }
            if !root.pop() {
                return Ok(None);
            }
        }
    }

    fn read(root: &Path, definition: Definition) -> Result<(Self, FeatureManifest)> {
        for name in [&definition.package, &definition.binary] {
            if name.is_empty()
                || name.len() > 128
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Err("Workspace provider package and binary must be Cargo names".into());
            }
        }
        let path = root.join(definition.manifest).canonicalize()?;
        if !path.starts_with(root) {
            return Err("Workspace provider manifest escapes its workspace".into());
        }
        let cargo = root
            .join(definition.cargo.unwrap_or_else(|| "Cargo.toml".into()))
            .canonicalize()?;
        if !cargo.starts_with(root) || !cargo.is_file() {
            return Err("Workspace provider Cargo manifest escapes its workspace".into());
        }
        let mut manifest: FeatureManifest = ron::from_str(&Self::text(&path)?)?;
        let cli = manifest
            .cli
            .take()
            .ok_or("Workspace provider requires a CLI manifest")?;
        if cli.default.is_some() || cli.commands.len() != 1 {
            return Err("Workspace provider must export one command and no default".into());
        }
        let command = &cli.commands[0];
        if command.name != definition.name {
            return Err("Workspace provider command differs from its declared name".into());
        }
        if command.id.is_empty() || command.name.is_empty() {
            return Err("Workspace provider command requires a name and namespace".into());
        }
        if command.id == "dx" || command.id.starts_with("dx.") {
            return Err("Workspace provider cannot use the built-in DX namespace".into());
        }
        let prefix = command.id.clone();
        let mut pending = vec![command];
        let mut count = 0;
        while let Some(command) = pending.pop() {
            count += 1;
            if count > 128 || command.arguments.len() > 64 {
                return Err("Workspace provider CLI manifest exceeds its bounds".into());
            }
            if command.name.is_empty() || command.id.is_empty() {
                return Err("Workspace provider command requires a name and namespace".into());
            }
            if command.id != prefix && !command.id.starts_with(&format!("{prefix}.")) {
                return Err("Workspace provider command escapes its namespace".into());
            }
            for child in &command.commands {
                pending.push(child);
            }
        }
        let mut projection = FeatureManifest::parse("()");
        projection.cli = Some(cli);
        Ok((
            Self {
                root: root.to_owned(),
                package: definition.package,
                binary: definition.binary,
                cargo,
                prefix,
            },
            projection,
        ))
    }

    fn text(path: &Path) -> Result<String> {
        let mut text = String::new();
        File::open(path)?
            .take(1024 * 1024 + 1)
            .read_to_string(&mut text)?;
        if text.len() > 1024 * 1024 {
            return Err("Workspace CLI metadata exceeds 1 MiB".into());
        }
        Ok(text)
    }

    pub fn owns(&self, command: &str) -> bool {
        command == self.prefix || command.starts_with(&format!("{}.", self.prefix))
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.owns(&other.prefix) || other.owns(&self.prefix)
    }

    pub fn arguments(
        &self,
        manifest: &FeatureManifest,
        invocation: &CliInvocation,
    ) -> Result<Vec<OsString>> {
        if !self.owns(&invocation.command) {
            return Err("Workspace provider does not own this command".into());
        }
        let cli = manifest
            .cli
            .as_ref()
            .ok_or("Workspace provider has no CLI manifest")?;
        let mut route = Vec::new();
        if !Self::route(&cli.commands[0], &invocation.command, &mut route) {
            return Err("Workspace provider command is not declared".into());
        }
        let mut arguments = Vec::new();
        let mut known = std::collections::BTreeSet::new();
        for command in route {
            arguments.push(command.name.clone().into());
            for argument in &command.arguments {
                known.insert(argument.id.as_str());
                let Some(values) = invocation.arguments.get(&argument.id) else {
                    continue;
                };
                if argument.trailing {
                    if !values.is_empty() {
                        arguments.push("--".into());
                        arguments.extend(values.iter().cloned());
                    }
                    continue;
                }
                if argument.flag && invocation.value(&argument.id) != Some("true") {
                    continue;
                }
                if let Some(long) = &argument.long {
                    arguments.push(format!("--{long}").into());
                } else if let Some(short) = argument.short {
                    arguments.push(format!("-{short}").into());
                }
                if !argument.flag {
                    arguments.extend(values.iter().cloned());
                }
            }
        }
        for key in invocation.arguments.keys() {
            if !known.contains(key.as_str()) {
                return Err("Workspace provider argument is not declared".into());
            }
        }
        Ok(arguments)
    }

    fn route<'a>(
        command: &'a CliCommandManifest,
        id: &str,
        route: &mut Vec<&'a CliCommandManifest>,
    ) -> bool {
        route.push(command);
        if command.id == id {
            return true;
        }
        for child in &command.commands {
            if Self::route(child, id, route) {
                return true;
            }
        }
        route.pop();
        false
    }

    pub fn checkout(&self) -> Result<Checkout> {
        let mut path = self.root.clone();
        for part in [".build-tools", "providers", &self.package] {
            path.push(part);
            if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
                return Err("Workspace provider cache cannot use symlinks".into());
            }
        }
        fs::create_dir_all(
            path.parent()
                .ok_or("Missing workspace provider cache parent")?,
        )?;
        Ok(Checkout {
            root: self.root.clone(),
            target: path,
            build_target: Some(env!("VMUX_DX_HOST_TARGET").into()),
        })
    }
}

#[derive(Deserialize)]
struct Declaration {
    providers: Vec<Definition>,
}

#[derive(Deserialize)]
struct Definition {
    name: String,
    package: String,
    binary: String,
    manifest: PathBuf,
    cargo: Option<PathBuf>,
}

#[derive(Component, Clone)]
pub struct Executable(Arc<Prepared>);

impl Executable {
    pub fn snapshot(source: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(source)?;
        if !metadata.is_file()
            || metadata.len() > 64 * 1024 * 1024
            || metadata.permissions().mode() & 0o111 == 0
        {
            return Err("Workspace provider build produced no bounded executable".into());
        }
        let temporary = Temporary::create(&std::env::temp_dir(), ".vmux-workspace-provider")?;
        let path = temporary.path().join("provider");
        fs::copy(source, &path)?;
        Ok(Self(Arc::new(Prepared {
            path,
            _temporary: temporary,
        })))
    }

    pub fn path(&self) -> &Path {
        &self.0.path
    }
}

struct Prepared {
    path: PathBuf,
    _temporary: Temporary,
}
