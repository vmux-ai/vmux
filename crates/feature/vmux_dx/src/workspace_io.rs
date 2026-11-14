use std::fs::File;
use std::io::Read;
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

    pub fn feature(&self) -> Result<FeatureManifest> {
        Ok(ron::from_str(&self.text()?)?)
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

#[derive(Component, Clone)]
pub struct Executable(Arc<Prepared>);

impl Executable {
    pub fn new(path: PathBuf, temporary: Arc<Temporary>) -> Self {
        Self(Arc::new(Prepared {
            path,
            _temporary: temporary,
        }))
    }

    pub fn path(&self) -> &Path {
        &self.0.path
    }
}

struct Prepared {
    path: PathBuf,
    _temporary: Arc<Temporary>,
}
