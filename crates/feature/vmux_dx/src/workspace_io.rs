use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy_ecs::prelude::Component;
use serde::Deserialize;
use vmux_ecs::manifest::FeatureManifest;

use crate::Result;
use crate::temporary::Temporary;

pub struct MetadataFile(pub PathBuf);

impl MetadataFile {
    pub fn cargo(&self) -> Result<Option<Declaration>> {
        let value: toml::Value = toml::from_str(&self.text()?)?;
        let Some(configuration) = value
            .get("workspace")
            .and_then(|value| value.get("metadata"))
            .and_then(|value| value.get("vmux"))
            .and_then(|value| value.get("dx"))
        else {
            return Ok(None);
        };
        Ok(Some(configuration.clone().try_into()?))
    }

    pub fn feature(&self) -> Result<(PathBuf, FeatureManifest)> {
        Ok((self.0.canonicalize()?, ron::from_str(&self.text()?)?))
    }

    fn text(&self) -> Result<String> {
        let mut text = String::new();
        File::open(&self.0)?
            .take(1024 * 1024 + 1)
            .read_to_string(&mut text)?;
        if text.len() > 1024 * 1024 {
            return Err("Workspace CLI metadata exceeds 1 MiB".into());
        }
        Ok(text)
    }
}

#[derive(Deserialize)]
pub struct Declaration {
    pub providers: Vec<Definition>,
}

#[derive(Deserialize)]
pub struct Definition {
    pub name: String,
    pub package: String,
    pub binary: String,
    pub manifest: PathBuf,
    pub cargo: Option<PathBuf>,
}

pub struct Cache(pub PathBuf);

impl Cache {
    pub fn prepare(self, provider: &crate::workspace::Provider) -> Result<PathBuf> {
        let mut path = provider.root.clone();
        for component in self.0.strip_prefix(&provider.root)?.components() {
            path.push(component);
            if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
                return Err("Workspace provider cache cannot use symlinks".into());
            }
        }
        fs::create_dir_all(self.0.parent().ok_or("Missing provider cache parent")?)?;
        Ok(self.0)
    }
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
