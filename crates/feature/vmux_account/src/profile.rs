use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vmux_ecs::record::{Record, StoredComponent, StoredRelationship};

#[derive(Component, Serialize, Deserialize)]
pub struct Account;

impl Account {
    pub(crate) fn record(id: Uuid) -> Record {
        Record {
            scope_id: id,
            id,
            revision: 1,
            deleted: false,
            components: vec![
                StoredComponent {
                    key: "account".into(),
                    version: 1,
                    value: serde_json::Value::Null,
                },
                StoredComponent {
                    key: "name".into(),
                    version: 1,
                    value: serde_json::json!("Local account"),
                },
            ],
            relationships: Vec::new(),
        }
    }
}

#[derive(Component, Serialize, Deserialize)]
pub struct Profile;

impl Profile {
    pub(crate) fn record(scope: Uuid, id: Uuid, directory: &str, name: &str) -> Record {
        Record {
            scope_id: scope,
            id,
            revision: 1,
            deleted: false,
            components: vec![
                StoredComponent {
                    key: "profile".into(),
                    version: 1,
                    value: serde_json::Value::Null,
                },
                StoredComponent {
                    key: "directory".into(),
                    version: 1,
                    value: serde_json::json!(directory),
                },
                StoredComponent {
                    key: "name".into(),
                    version: 1,
                    value: serde_json::json!(name),
                },
            ],
            relationships: vec![StoredRelationship {
                key: "account".into(),
                version: 1,
                target: scope,
                position: 0,
                owned: true,
            }],
        }
    }

    pub(crate) fn name(raw: &str) -> Result<String, Failure> {
        let name = raw.trim();
        if name.is_empty() || name.chars().count() > 256 || name.chars().any(char::is_control) {
            return Err(Failure::InvalidName);
        }
        Ok(name.to_string())
    }

    pub(crate) fn label(directory: &str) -> String {
        let mut characters = directory.chars();
        match characters.next() {
            Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
            None => "Personal".into(),
        }
    }
}

#[derive(Component)]
#[relationship(relationship_target = Profiles)]
pub struct Owner(pub Entity);

#[derive(Component, Default)]
#[relationship_target(relationship = Owner, linked_spawn)]
pub struct Profiles(Vec<Entity>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    InvalidName,
    UnknownProfile,
    NotReady,
    Conflict,
    Storage(String),
    InvalidStore,
}
