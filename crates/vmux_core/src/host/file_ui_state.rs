use bevy::prelude::*;
use bevy_cef::prelude::BinHostEmitEvent;
use rkyv::api::high::HighSerializer;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;
use vmux_api::HostEvent;

use crate::event::{FileUiStateEvent, FileUiStatePatch};

#[derive(Component, Default)]
pub struct FileUiStateUpdates {
    sequence: u64,
    patches: Vec<FileUiStatePatch>,
}

impl FileUiStateUpdates {
    pub fn push(&mut self, patch: FileUiStatePatch) {
        self.patches.push(patch);
    }

    pub fn take(&mut self) -> Option<FileUiStateEvent> {
        if self.patches.is_empty() {
            return None;
        }
        self.sequence = self.sequence.wrapping_add(1).max(1);
        Some(FileUiStateEvent {
            sequence: self.sequence,
            patches: std::mem::take(&mut self.patches),
        })
    }

    pub fn deliver<T>(
        pages: &Query<(), With<Self>>,
        commands: &mut Commands,
        webview: Entity,
        event: &T,
    ) where
        T: HostEvent
            + Clone
            + Into<FileUiStatePatch>
            + for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        if pages.contains(webview) {
            commands.trigger(FileUiStateWrite::from_event(webview, event));
        } else {
            commands.trigger(BinHostEmitEvent::from_event(webview, event));
        }
    }
}

#[derive(Clone, EntityEvent)]
pub struct FileUiStateWrite {
    #[event_target]
    pub webview: Entity,
    pub patch: FileUiStatePatch,
}

impl FileUiStateWrite {
    pub fn from_event<T>(webview: Entity, event: &T) -> Self
    where
        T: Clone + Into<FileUiStatePatch>,
    {
        Self {
            webview,
            patch: event.clone().into(),
        }
    }

    pub fn webview(&self) -> Entity {
        self.webview
    }

    pub fn patch(&self) -> &FileUiStatePatch {
        &self.patch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::git::GitChangedEvent;

    #[derive(Resource)]
    struct Targets {
        file: Entity,
        direct: Entity,
    }

    #[derive(Resource, Default)]
    struct Delivered {
        writes: Vec<Entity>,
        direct: Vec<Entity>,
    }

    impl Delivered {
        fn write(trigger: On<FileUiStateWrite>, mut delivered: ResMut<Self>) {
            delivered.writes.push(trigger.event().webview());
        }

        fn direct(trigger: On<BinHostEmitEvent>, mut delivered: ResMut<Self>) {
            delivered.direct.push(trigger.event().webview());
        }
    }

    fn deliver(
        targets: Res<Targets>,
        pages: Query<(), With<FileUiStateUpdates>>,
        mut commands: Commands,
    ) {
        FileUiStateUpdates::deliver(&pages, &mut commands, targets.file, &GitChangedEvent {});
        FileUiStateUpdates::deliver(&pages, &mut commands, targets.direct, &GitChangedEvent {});
    }

    #[test]
    fn file_pages_collect_updates_while_other_pages_receive_direct_events() {
        let mut app = App::new();
        let file = app.world_mut().spawn(FileUiStateUpdates::default()).id();
        let direct = app.world_mut().spawn_empty().id();
        app.insert_resource(Targets { file, direct })
            .init_resource::<Delivered>()
            .add_observer(Delivered::write)
            .add_observer(Delivered::direct)
            .add_systems(Update, deliver);

        app.update();

        let delivered = app.world().resource::<Delivered>();
        assert_eq!(delivered.writes, vec![file]);
        assert_eq!(delivered.direct, vec![direct]);
    }
}
