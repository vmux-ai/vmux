use std::path::Path;
use std::time::SystemTime;

use bevy::prelude::*;
use vmux_ecs::Order;

use super::editor::Editor;
use super::fold_preferences::FileId;

#[derive(Component)]
pub(super) struct Parked;

#[derive(Component)]
pub(super) struct Modified(Option<SystemTime>);

impl Parked {
    pub(super) fn retain(
        world: &mut World,
        owner: Entity,
        edit: Editor,
        diff: vmux_git::GitDiffSource,
    ) {
        let path = edit.core.buffer.path.clone();
        let modified = super::file_loading_io::Inspect::modified_at(&path);
        let mut entries = Vec::new();
        let mut existing = None;
        if let Some(children) = world.get::<Children>(owner) {
            for entity in children.iter() {
                if world.get::<Self>(entity).is_none() {
                    continue;
                }
                let file = world.get::<FileId>(entity).unwrap();
                let order = world.get::<Order>(entity).unwrap();
                if file.0 == path {
                    existing = Some(entity);
                } else {
                    entries.push((
                        entity,
                        order.0,
                        world.get::<Editor>(entity).unwrap().core.dirty,
                    ));
                }
            }
        }
        entries.sort_by_key(|(_, order, _)| *order);
        let dirty = edit.core.dirty;
        let entity = match existing {
            Some(entity) => {
                world
                    .entity_mut(entity)
                    .insert((Modified(modified), edit, diff));
                entity
            }
            None => world
                .spawn((
                    Name::new("Parked editor"),
                    FileId(path),
                    ChildOf(owner),
                    Order(entries.len() as u32),
                    Modified(modified),
                    Self,
                    edit,
                    diff,
                ))
                .id(),
        };
        entries.push((entity, entries.len() as u32, dirty));
        for (order, (entity, _, _)) in entries.iter().enumerate() {
            world.entity_mut(*entity).insert(Order(order as u32));
        }
        while entries.len() > 8 {
            let Some(index) = entries.iter().position(|(_, _, dirty)| !dirty) else {
                break;
            };
            let (evicted, _, _) = entries.remove(index);
            world.despawn(evicted);
        }
    }

    pub(super) fn resume(
        world: &mut World,
        owner: Entity,
        path: &Path,
    ) -> Option<(Editor, vmux_git::GitDiffSource)> {
        let mut selected = None;
        if let Some(children) = world.get::<Children>(owner) {
            for entity in children.iter() {
                if world.get::<Self>(entity).is_none() {
                    continue;
                }
                let file = world.get::<FileId>(entity).unwrap();
                if file.0 == path {
                    selected = Some((entity, world.get::<Modified>(entity).unwrap().0));
                    break;
                }
            }
        }
        let (entity, modified) = selected?;
        let mut parked = world.entity_mut(entity);
        let edit = parked.take::<Editor>()?;
        let diff = parked.take::<vmux_git::GitDiffSource>()?;
        world.despawn(entity);
        if edit.core.dirty || modified == super::file_loading_io::Inspect::modified_at(path) {
            return Some((edit, diff));
        }
        None
    }

    pub(super) fn transfer(world: &mut World, source: Entity, target: Entity) {
        if source == target || world.get_entity(target).is_err() {
            return;
        }
        let children = world
            .get::<Children>(target)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        for entity in children {
            if world.get::<Self>(entity).is_some() {
                world.despawn(entity);
            }
        }
        let children = world
            .get::<Children>(source)
            .map(|children| children.to_vec())
            .unwrap_or_default();
        for entity in children {
            if world.get::<Self>(entity).is_some() {
                world.entity_mut(entity).insert(ChildOf(target));
            }
        }
    }
}
