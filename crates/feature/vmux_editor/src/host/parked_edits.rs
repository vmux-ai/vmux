use crate::host::editor::EditReadOnly;
use std::path::Path;
use std::time::SystemTime;

use bevy::prelude::*;
use vmux_ecs::Order;

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
    incoming: Query<'w, 's, (EditReadOnly, Option<&'static vmux_git::GitDiffSource>)>,
    keymaps: Query<'w, 's, (), With<super::keymap::EditorKeymap>>,
    buffers: Query<
        'w,
        's,
        (
            &'static FileId,
            &'static Order,
            EditReadOnly,
            &'static Modified,
        ),
        With<Parked>,
    >,
    commands: Commands<'w, 's>,
}

impl Buffers<'_, '_> {
    pub(super) fn retain(&mut self, owner: Entity, incoming: Entity) {
        if !self.owners.contains(owner) {
            return;
        }
        let Ok((edit, diff)) = self.incoming.get(incoming) else {
            return;
        };
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
                    entries.push((entity, order.0, edit.core.revision.dirty));
                }
            }
        }
        entries.sort_by_key(|(_, order, _)| *order);
        let dirty = edit.core.revision.dirty;
        if let Some(existing) = existing {
            self.commands.entity(existing).despawn();
        }
        self.commands.entity(incoming).insert((
            Name::new("Parked editor"),
            FileId(path),
            ChildOf(owner),
            Order(entries.len() as u32),
            Modified(modified),
            Parked,
        ));
        if diff.is_none() {
            self.commands
                .entity(incoming)
                .insert(vmux_git::GitDiffSource {
                    content: edit.core.buffer.text(),
                    dirty,
                });
        }
        let entity = incoming;
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
        let Some(entity) = self.selected(owner, path) else {
            return false;
        };
        self.commands.queue(move |world: &mut World| {
            if world.get_entity(owner).is_err() {
                return;
            }
            let Ok(mut entry) = world.get_entity_mut(entity) else {
                return;
            };
            let Some((edit, diff)) =
                entry.take::<(crate::host::editor::EditorBundle, vmux_git::GitDiffSource)>()
            else {
                return;
            };
            entry.despawn();
            world.entity_mut(owner).insert((edit, diff, keymap));
        });
        true
    }

    pub(super) fn resume_saved(&mut self, owner: Entity, path: &Path, keymap: Entity) -> bool {
        if !self.keymaps.contains(keymap) {
            return false;
        }
        let Some(entity) = self.selected(owner, path) else {
            return false;
        };
        self.commands.queue(move |world: &mut World| {
            if world.get_entity(owner).is_err() || world.get_entity(entity).is_err() {
                return;
            }
            let Ok(mut source) = world.get_entity_mut(keymap) else {
                return;
            };
            let Some(keymap) = source.take::<super::keymap::EditorKeymap>() else {
                return;
            };
            let mut entry = world.entity_mut(entity);
            let Some((edit, diff)) =
                entry.take::<(crate::host::editor::EditorBundle, vmux_git::GitDiffSource)>()
            else {
                return;
            };
            entry.despawn();
            world.entity_mut(owner).insert((edit, diff, keymap));
        });
        true
    }

    fn selected(&mut self, owner: Entity, path: &Path) -> Option<Entity> {
        let mut selected = None;
        if let Ok(children) = self.children.get(owner) {
            for entity in children.iter() {
                let Ok((file, _, edit, modified)) = self.buffers.get(entity) else {
                    continue;
                };
                if file.0 == path {
                    selected = Some((entity, edit.core.revision.dirty, modified.0));
                    break;
                }
            }
        }
        let (entity, dirty, modified) = selected?;
        if !dirty && modified != super::file_loading_io::Inspect::modified_at(path) {
            self.commands.entity(entity).despawn();
            return None;
        }
        Some(entity)
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
