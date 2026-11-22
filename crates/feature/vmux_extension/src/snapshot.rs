use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub popup: Option<String>,
    pub icon: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub profile_enabled: BTreeMap<String, bool>,
    #[serde(default)]
    pub profile_pinned: BTreeMap<String, bool>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub optional_permissions: Vec<String>,
    #[serde(default)]
    pub host_permissions: Vec<String>,
    #[serde(default)]
    pub optional_host_permissions: Vec<String>,
    #[serde(default)]
    pub approved_grants: BTreeMap<String, ExtensionGrants>,
    #[serde(default)]
    pub source_hash: String,
    #[serde(default)]
    pub public_key_b64: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionGrants {
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub host_permissions: Vec<String>,
}

impl ExtensionGrants {
    pub fn covers(&self, permissions: &[String], host_permissions: &[String]) -> bool {
        permissions
            .iter()
            .all(|permission| self.permissions.contains(permission))
            && host_permissions
                .iter()
                .all(|permission| self.host_permissions.contains(permission))
    }

    pub fn retain_declared(
        &mut self,
        permissions: &[String],
        optional_permissions: &[String],
        host_permissions: &[String],
        optional_host_permissions: &[String],
    ) {
        self.permissions.retain(|permission| {
            permissions.contains(permission) || optional_permissions.contains(permission)
        });
        self.host_permissions.retain(|permission| {
            host_permissions.contains(permission) || optional_host_permissions.contains(permission)
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    #[serde(default)]
    pub(crate) version: u32,
    pub entries: Vec<ExtEntry>,
    #[serde(skip)]
    pub(crate) migrated: bool,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            version: Index::VERSION,
            entries: Vec::new(),
            migrated: false,
        }
    }
}

impl Index {
    const VERSION: u32 = 3;

    pub fn requires_save(&self) -> bool {
        self.migrated
    }

    pub fn upsert(&mut self, entry: ExtEntry) {
        if let Some(slot) = self.entries.iter_mut().find(|slot| slot.id == entry.id) {
            *slot = entry;
        } else {
            self.entries.push(entry);
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version > Index::VERSION {
            return Err("unsupported extension index version".into());
        }
        let mut ids = std::collections::HashSet::new();
        for entry in &self.entries {
            if entry.id.len() != 32 || !entry.id.bytes().all(|byte| (b'a'..=b'p').contains(&byte)) {
                return Err("invalid extension package identity".into());
            }
            if !ids.insert(&entry.id) {
                return Err("duplicate extension package identity".into());
            }
            if entry.version.is_empty()
                || entry.version == "."
                || entry.version == ".."
                || entry.version.contains(['/', '\\'])
            {
                return Err("invalid extension package version".into());
            }
        }
        Ok(())
    }

    pub fn decode(source: &str) -> Result<Self, String> {
        let mut index: Self = serde_json::from_str(source).map_err(|error| error.to_string())?;
        if index.version > Index::VERSION {
            return Err("unsupported extension index version".into());
        }
        if index.version < Index::VERSION {
            for entry in &mut index.entries {
                if index.version == 0 {
                    entry.profile_enabled.clear();
                    entry
                        .profile_enabled
                        .insert("personal".into(), entry.enabled);
                }
                entry.enabled = false;
                for (profile, enabled) in &entry.profile_enabled {
                    if *enabled {
                        entry
                            .approved_grants
                            .entry(profile.clone())
                            .or_insert_with(|| ExtensionGrants {
                                permissions: entry.permissions.clone(),
                                host_permissions: entry.host_permissions.clone(),
                            });
                    }
                }
            }
            index.version = Index::VERSION;
            index.migrated = true;
        }
        index.validate()?;
        Ok(index)
    }
}

impl ExtEntry {
    pub fn installed_for(&self, profile: &str) -> bool {
        self.profile_enabled.contains_key(profile)
    }

    pub fn enabled_for(&self, profile: &str) -> bool {
        self.profile_enabled.get(profile).copied().unwrap_or(false)
    }

    pub fn pinned_for(&self, profile: &str) -> bool {
        self.profile_pinned.get(profile).copied().unwrap_or(false)
    }

    pub fn grants_for(&self, profile: &str) -> ExtensionGrants {
        self.approved_grants
            .get(profile)
            .cloned()
            .unwrap_or_default()
    }
}
