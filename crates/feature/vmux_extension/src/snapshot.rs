use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vmux_api::extension::{ExtRow, ExtStatus, ExtensionsUiState};

const INDEX_VERSION: u32 = 3;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnableForProfileResult {
    Updated,
    NeedsApproval,
    NotFound,
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
            version: INDEX_VERSION,
            entries: Vec::new(),
            migrated: false,
        }
    }
}

impl Index {
    pub fn requires_save(&self) -> bool {
        self.migrated
    }

    pub fn upsert(&mut self, e: ExtEntry) {
        if let Some(slot) = self.entries.iter_mut().find(|x| x.id == e.id) {
            *slot = e;
        } else {
            self.entries.push(e);
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.retain(|x| x.id != id);
    }

    pub fn set_enabled_for(
        &mut self,
        profile: &str,
        id: &str,
        enabled: bool,
        approve_permissions: bool,
    ) -> EnableForProfileResult {
        let Some(slot) = self.entries.iter_mut().find(|x| x.id == id) else {
            return EnableForProfileResult::NotFound;
        };
        if !slot.installed_for(profile) {
            return EnableForProfileResult::NotFound;
        }
        if !enabled {
            slot.profile_enabled.insert(profile.to_string(), false);
            return EnableForProfileResult::Updated;
        }
        if !slot
            .grants_for(profile)
            .covers(&slot.permissions, &slot.host_permissions)
        {
            if !approve_permissions {
                return EnableForProfileResult::NeedsApproval;
            }
            slot.approved_grants.insert(
                profile.to_string(),
                ExtensionGrants {
                    permissions: slot.permissions.clone(),
                    host_permissions: slot.host_permissions.clone(),
                },
            );
        }
        slot.profile_enabled.insert(profile.to_string(), true);
        EnableForProfileResult::Updated
    }

    pub fn set_pinned_for(&mut self, profile: &str, id: &str, pinned: bool) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        if !entry.installed_for(profile) {
            return false;
        }
        if entry.pinned_for(profile) == pinned {
            return false;
        }
        entry.profile_pinned.insert(profile.to_string(), pinned);
        true
    }

    pub fn enabled_ids_for(&self, profile: &str) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled_for(profile))
            .map(|e| e.id.clone())
            .collect()
    }

    pub fn is_dirty_for(&self, profile: &str, loaded: &[String]) -> bool {
        let mut a = self.enabled_ids_for(profile);
        let mut b = loaded.to_vec();
        a.sort();
        b.sort();
        a != b
    }

    pub fn snapshot(&self, profile: &str, loaded: &[String]) -> ExtensionsUiState {
        let mut extensions = Vec::new();
        for entry in &self.entries {
            if !entry.installed_for(profile) {
                continue;
            }
            let enabled = entry.enabled_for(profile);
            extensions.push(ExtRow {
                id: entry.id.clone(),
                name: entry.name.clone(),
                version: entry.version.clone(),
                icon: entry.icon.clone(),
                popup: entry.popup.clone(),
                enabled,
                pinned: entry.pinned_for(profile),
                needs_approval: !entry
                    .grants_for(profile)
                    .covers(&entry.permissions, &entry.host_permissions),
                required_permissions: entry.permissions.clone(),
                required_host_permissions: entry.host_permissions.clone(),
                status: if enabled {
                    ExtStatus::Installed
                } else {
                    ExtStatus::Disabled
                },
            });
        }
        let visible = extensions.clone();
        ExtensionsUiState {
            loaded: true,
            extensions,
            visible,
            query: String::new(),
            installing: Vec::new(),
            pending: self.is_dirty_for(profile, loaded),
        }
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

impl Index {
    pub fn validate(&self) -> Result<(), String> {
        if self.version > INDEX_VERSION {
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
}

impl Index {
    pub fn decode(source: &str) -> Result<Self, String> {
        let mut index: Self = serde_json::from_str(source).map_err(|error| error.to_string())?;
        if index.version > INDEX_VERSION {
            return Err("unsupported extension index version".into());
        }
        if index.version < INDEX_VERSION {
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
            index.version = INDEX_VERSION;
            index.migrated = true;
        }
        index.validate()?;
        Ok(index)
    }
}
