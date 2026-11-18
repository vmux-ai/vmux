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

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct Buffers<'w, 's> {
    owners: Query<'w, 's, ()>,
    children: Query<'w, 's, &'static Children>,
    parked: Query<'w, 's, (), With<Parked>>,
    buffers: Query<
        'w,
        's,
        (
            &'static FileId,
            &'static Order,
            &'static Editor,
            &'static Modified,
        ),
        With<Parked>,
    >,
    commands: Commands<'w, 's>,
}

impl Buffers<'_, '_> {
    pub(super) fn retain(&mut self, owner: Entity, edit: Editor, diff: vmux_git::GitDiffSource) {
        if !self.owners.contains(owner) {
            return;
        }
        let path = edit.core.buffer.path.clone();
        let modified = super::file_loading_io::Inspect::modified_at(&path);
        let mut entries = Vec::new();
        let mut existing = None;
        if let Ok(children) = self.children.get(owner) {
            for entity in children.iter() {
                let Ok((file, order, edit, _)) = self.buffers.get(entity) else {
                    continue;
                };
                if file.0 == path {
                    existing = Some(entity);
                } else {
                    entries.push((entity, order.0, edit.core.dirty));
                }
            }
        }
        entries.sort_by_key(|(_, order, _)| *order);
        let dirty = edit.core.dirty;
        let entity = match existing {
            Some(entity) => {
                self.commands
                    .entity(entity)
                    .insert((Modified(modified), edit, diff));
                entity
            }
            None => self
                .commands
                .spawn((
                    Name::new("Parked editor"),
                    FileId(path),
                    ChildOf(owner),
                    Order(entries.len() as u32),
                    Modified(modified),
                    Parked,
                    edit,
                    diff,
                ))
                .id(),
        };
        entries.push((entity, entries.len() as u32, dirty));
        for (order, (entity, _, _)) in entries.iter().enumerate() {
            self.commands.entity(*entity).insert(Order(order as u32));
        }
        while entries.len() > 8 {
            let Some(index) = entries.iter().position(|(_, _, dirty)| !dirty) else {
                break;
            };
            let (evicted, _, _) = entries.remove(index);
            self.commands.entity(evicted).despawn();
        }
    }

    pub(super) fn resume(
        &mut self,
        owner: Entity,
        path: &Path,
        keymap: super::keymap::EditorKeymap,
    ) -> bool {
        let mut selected = None;
        if let Ok(children) = self.children.get(owner) {
            for entity in children.iter() {
                let Ok((file, _, edit, modified)) = self.buffers.get(entity) else {
                    continue;
                };
                if file.0 == path {
                    selected = Some((entity, edit.core.dirty, modified.0));
                    break;
                }
            }
        }
        let Some((entity, dirty, modified)) = selected else {
            return false;
        };
        if !dirty && modified != super::file_loading_io::Inspect::modified_at(path) {
            self.commands.entity(entity).despawn();
            return false;
        }
        self.commands.queue(move |world: &mut World| {
            if world.get_entity(owner).is_err() {
                return;
            }
            let Ok(mut entry) = world.get_entity_mut(entity) else {
                return;
            };
            let Some((edit, diff)) = entry.take::<(Editor, vmux_git::GitDiffSource)>() else {
                return;
            };
            entry.despawn();
            world.entity_mut(owner).insert((edit, diff, keymap));
        });
        true
    }

    pub(super) fn transfer(&mut self, source: Entity, target: Entity) {
        if source == target || !self.owners.contains(target) {
            return;
        }
        if let Ok(children) = self.children.get(target) {
            for entity in children.iter() {
                if self.parked.contains(entity) {
                    self.commands.entity(entity).despawn();
                }
            }
        }
        if let Ok(children) = self.children.get(source) {
            for entity in children.iter() {
                if self.parked.contains(entity) {
                    self.commands.entity(entity).insert(ChildOf(target));
                }
            }
        }
    }
}
