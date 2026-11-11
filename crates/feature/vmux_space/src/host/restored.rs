use bevy::prelude::*;
use bevy::reflect::ReflectMut;
use bevy_world_serialization::{DynamicEntity, DynamicWorld};
use std::any::TypeId;
use std::collections::HashSet;
use vmux_ecs::{Cwd, PageMetadata};

pub(super) struct Snapshot(pub(super) DynamicWorld);

impl Snapshot {
    pub(super) fn recover(mut self) -> Result<DynamicWorld, String> {
        let owners: HashSet<Entity> = self.0.entities.iter().map(|entity| entity.entity).collect();
        let mut discarded = HashSet::new();
        for entity in &self.0.entities {
            if StoredPage(entity).is_orphan(&owners) {
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
            "Discarded saved transient page metadata with missing owners"
        );
        Ok(self.0)
    }
}

struct StoredPage<'a>(&'a DynamicEntity);

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
