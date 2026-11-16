use bevy::prelude::*;
use bevy::reflect::ReflectMut;
use bevy_world_serialization::{DynamicEntity, DynamicWorld};
use std::any::TypeId;
use std::collections::HashSet;
use vmux_ecs::{Cwd, PageMetadata};
use vmux_session::AgentId;

pub(super) struct Snapshot(pub(super) DynamicWorld);

impl Snapshot {
    pub(super) fn recover(mut self) -> Result<DynamicWorld, String> {
        let mut owners = HashSet::new();
        for entity in &self.0.entities {
            if !owners.insert(entity.entity) {
                return Err(format!(
                    "duplicate stored entity identity: {:?}",
                    entity.entity
                ));
            }
        }
        let mut discarded = HashSet::new();
        for entity in &self.0.entities {
            if StoredPage(entity).is_orphan(&owners) || StoredAgent(entity).is_orphan(&owners) {
                discarded.insert(entity.entity);
            }
        }
        if discarded.is_empty() {
            return Ok(self.0);
        }
        self.0
            .entities
            .retain(|entity| !discarded.contains(&entity.entity));
        for entity in &mut self.0.entities {
            for component in &mut entity.components {
                if component
                    .get_represented_type_info()
                    .is_none_or(|info| info.type_id() != TypeId::of::<Children>())
                {
                    continue;
                }
                let ReflectMut::TupleStruct(tuple) = component.reflect_mut() else {
                    return Err("invalid stored children".into());
                };
                let Some(field) = tuple.field_mut(0) else {
                    return Err("invalid stored children".into());
                };
                let ReflectMut::List(children) = field.reflect_mut() else {
                    return Err("invalid stored children".into());
                };
                for index in (0..children.len()).rev() {
                    if children
                        .get(index)
                        .and_then(Entity::from_reflect)
                        .is_some_and(|child| discarded.contains(&child))
                    {
                        children.remove(index);
                    }
                }
            }
        }
        warn!(
            count = discarded.len(),
            "Discarded saved transient metadata with missing owners"
        );
        Ok(self.0)
    }
}

struct StoredPage<'a>(&'a DynamicEntity);

struct StoredAgent<'a>(&'a DynamicEntity);

impl StoredAgent<'_> {
    fn is_orphan(&self, owners: &HashSet<Entity>) -> bool {
        if self.0.components.len() != 3 {
            return false;
        }
        let mut agent = false;
        let mut parent = None;
        let mut children = None;
        for component in &self.0.components {
            let Some(info) = component.get_represented_type_info() else {
                return false;
            };
            if info.type_id() == TypeId::of::<AgentId>() {
                agent = true;
            } else if info.type_id() == TypeId::of::<ChildOf>() {
                parent = ChildOf::from_reflect(component.as_partial_reflect())
                    .map(|value| value.parent());
            } else if info.type_id() == TypeId::of::<Children>() {
                children = Children::from_reflect(component.as_partial_reflect());
            } else {
                return false;
            }
        }
        let (Some(parent), Some(children)) = (parent, children) else {
            return false;
        };
        agent && !owners.contains(&parent) && children.iter().all(|child| !owners.contains(&child))
    }
}

impl StoredPage<'_> {
    fn is_orphan(&self, owners: &HashSet<Entity>) -> bool {
        if self.0.components.len() != 3 {
            return false;
        }
        let mut cwd = false;
        let mut metadata = false;
        let mut parent = None;
        for component in &self.0.components {
            let Some(info) = component.get_represented_type_info() else {
                return false;
            };
            if info.type_id() == TypeId::of::<Cwd>() {
                cwd = true;
            } else if info.type_id() == TypeId::of::<PageMetadata>() {
                metadata = true;
            } else if info.type_id() == TypeId::of::<ChildOf>() {
                parent = ChildOf::from_reflect(component.as_partial_reflect())
                    .map(|value| value.parent());
            } else {
                return false;
            }
        }
        cwd && metadata && parent.is_some_and(|parent| !owners.contains(&parent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::relationship::RelationshipTarget;
    use vmux_ecs::store::WorkspaceRestoreValidator;
    use vmux_session::{Session, SessionId};

    #[test]
    fn recovery_refuses_duplicate_identities_before_discarding_orphaned_scaffolds() {
        for agent in [false, true] {
            for saved_first in [false, true] {
                let duplicate = Entity::from_bits(1);
                let missing = Entity::from_bits(2);
                let components: Vec<Box<dyn PartialReflect>> = if agent {
                    vec![
                        Box::new(AgentId("codex-acp".into())),
                        Box::new(ChildOf(missing)),
                        Box::new(Children::from_collection_risky(Vec::new())),
                    ]
                } else {
                    vec![
                        Box::new(Cwd("/saved".into())),
                        Box::new(PageMetadata::default()),
                        Box::new(ChildOf(missing)),
                    ]
                };
                let mut entities = vec![
                    DynamicEntity {
                        entity: duplicate,
                        components,
                    },
                    DynamicEntity {
                        entity: duplicate,
                        components: vec![Box::new(Name::new("Saved content"))],
                    },
                ];
                if saved_first {
                    entities.reverse();
                }
                let snapshot = DynamicWorld {
                    resources: Vec::new(),
                    entities,
                };
                assert!(Snapshot(snapshot).recover().is_err());
            }
        }
    }

    #[test]
    fn recovery_discards_only_empty_orphaned_agent_scaffolds() {
        let mut world = World::new();
        let missing = world.spawn_empty().id();
        let absent_child = world.spawn_empty().id();
        let discarded = world.spawn_empty().id();
        let retained = world.spawn_empty().id();
        let saved_child = world.spawn_empty().id();
        let snapshot = DynamicWorld {
            resources: Vec::new(),
            entities: vec![
                DynamicEntity {
                    entity: discarded,
                    components: vec![
                        Box::new(AgentId("codex-acp".into())),
                        Box::new(ChildOf(missing)),
                        Box::new(Children::from_collection_risky(vec![absent_child])),
                    ],
                },
                DynamicEntity {
                    entity: retained,
                    components: vec![
                        Box::new(AgentId("saved".into())),
                        Box::new(ChildOf(missing)),
                        Box::new(Children::from_collection_risky(vec![saved_child])),
                    ],
                },
                DynamicEntity {
                    entity: saved_child,
                    components: vec![Box::new(Name::new("Saved content"))],
                },
            ],
        };
        let recovered = Snapshot(snapshot).recover().unwrap();
        assert_eq!(
            recovered
                .entities
                .iter()
                .map(|entity| entity.entity)
                .collect::<HashSet<_>>(),
            HashSet::from([retained, saved_child]),
        );
        assert!(
            (WorkspaceRestoreValidator::ownership::<vmux_layout::space::Space>().validate)(
                &recovered
            )
            .is_err()
        );
    }

    #[test]
    fn recovery_drops_only_orphaned_runtime_pages_and_preserves_session_children() {
        let mut world = World::new();
        let session = world.spawn_empty().id();
        let keep = world.spawn(ChildOf(session)).id();
        let discarded = world.spawn(ChildOf(session)).id();
        let missing = world.spawn_empty().id();
        let children = Children::from_collection_risky(vec![keep, discarded]);
        let snapshot = DynamicWorld {
            resources: Vec::new(),
            entities: vec![
                DynamicEntity {
                    entity: session,
                    components: vec![
                        Box::new(Session),
                        Box::new(SessionId("saved".into())),
                        Box::new(Name::new("Saved session")),
                        Box::new(children),
                    ],
                },
                DynamicEntity {
                    entity: keep,
                    components: vec![
                        Box::new(ChildOf(session)),
                        Box::new(Name::new("Saved content")),
                    ],
                },
                DynamicEntity {
                    entity: discarded,
                    components: vec![
                        Box::new(ChildOf(missing)),
                        Box::new(Cwd("/saved".into())),
                        Box::new(PageMetadata::default()),
                    ],
                },
            ],
        };
        let recovered = Snapshot(snapshot).recover().unwrap();
        assert_eq!(recovered.entities.len(), 2);
        assert!(
            (WorkspaceRestoreValidator::ownership::<vmux_layout::space::Space>().validate)(
                &recovered
            )
            .is_ok()
        );
        let saved = recovered
            .entities
            .iter()
            .find(|entity| entity.entity == session)
            .unwrap();
        let children = saved
            .components
            .iter()
            .find_map(|value| value.try_downcast_ref::<Children>())
            .unwrap();
        assert_eq!(children.iter().collect::<Vec<_>>(), [keep]);
        assert!(saved.components.iter().any(|value| {
            SessionId::from_reflect(value.as_partial_reflect())
                .is_some_and(|value| value.0 == "saved")
        }));
    }

    #[test]
    fn recovery_keeps_named_records_and_valid_pages_instead_of_hiding_other_corruption() {
        let root = Entity::from_bits(1);
        let page = Entity::from_bits(2);
        let record = Entity::from_bits(3);
        let missing = Entity::from_bits(4);
        let snapshot = DynamicWorld {
            resources: Vec::new(),
            entities: vec![
                DynamicEntity {
                    entity: root,
                    components: vec![Box::new(Name::new("Root"))],
                },
                DynamicEntity {
                    entity: page,
                    components: vec![
                        Box::new(ChildOf(root)),
                        Box::new(Cwd("/root".into())),
                        Box::new(PageMetadata::default()),
                    ],
                },
                DynamicEntity {
                    entity: record,
                    components: vec![
                        Box::new(ChildOf(missing)),
                        Box::new(Name::new("Saved record")),
                    ],
                },
            ],
        };
        let recovered = Snapshot(snapshot).recover().unwrap();
        assert_eq!(recovered.entities.len(), 3);
        assert!(
            (WorkspaceRestoreValidator::ownership::<vmux_layout::space::Space>().validate)(
                &recovered
            )
            .is_err()
        );
    }
}
