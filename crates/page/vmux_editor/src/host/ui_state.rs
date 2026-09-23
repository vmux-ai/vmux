use bevy::prelude::*;
use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
use vmux_core::{
    event::{FileUiStateEvent, FileUiStatePatch},
    host::FileUiStateWrite,
};

pub(crate) struct UiStatePlugin;

impl Plugin for UiStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(FileUiStateUpdates::collect)
            .add_systems(Last, FileUiStateUpdates::emit);
    }
}

#[derive(Component, Default)]
pub(crate) struct FileUiStateUpdates {
    sequence: u64,
    patches: Vec<FileUiStatePatch>,
}

impl FileUiStateUpdates {
    fn collect(trigger: On<FileUiStateWrite>, mut updates: Query<&mut Self>) {
        let Ok(mut updates) = updates.get_mut(trigger.event().webview) else {
            return;
        };
        updates.patches.push(trigger.event().patch.clone());
    }

    fn emit(
        mut updates: Query<(Entity, &mut Self)>,
        browsers: Option<NonSend<Browsers>>,
        mut commands: Commands,
    ) {
        let Some(browsers) = browsers else {
            return;
        };
        for (entity, mut updates) in &mut updates {
            if updates.patches.is_empty() || !browsers.can_emit_to(&entity) {
                continue;
            }
            updates.sequence = updates.sequence.wrapping_add(1).max(1);
            let event = FileUiStateEvent {
                sequence: updates.sequence,
                patches: std::mem::take(&mut updates.patches),
            };
            commands.trigger(BinHostEmitEvent::from_event(entity, &event));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::BinEvent;
    use vmux_core::event::{FileDirtyEvent, FileScrollByEvent};

    #[derive(Resource, Default)]
    struct Emitted(Vec<FileUiStateEvent>);

    impl Emitted {
        fn record(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Self>) {
            if trigger.event().id() != FileUiStateEvent::id() {
                return;
            }
            let event = rkyv::from_bytes::<FileUiStateEvent, rkyv::rancor::Error>(
                trigger.event().payload(),
            )
            .unwrap();
            emitted.0.push(event);
        }
    }

    #[test]
    fn same_frame_updates_emit_one_ordered_batch() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, UiStatePlugin))
            .init_resource::<Emitted>()
            .add_observer(Emitted::record);
        let entity = app.world_mut().spawn(FileUiStateUpdates::default()).id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);

        app.world_mut().trigger(FileUiStateWrite::from_event(
            entity,
            &FileDirtyEvent { dirty: true },
        ));
        app.world_mut().trigger(FileUiStateWrite::from_event(
            entity,
            &FileScrollByEvent { lines: 4 },
        ));
        app.update();

        let emitted = &app.world().resource::<Emitted>().0;
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].sequence, 1);
        assert!(matches!(
            emitted[0].patches.as_slice(),
            [
                FileUiStatePatch::Dirty(FileDirtyEvent { dirty: true }),
                FileUiStatePatch::ScrollBy(FileScrollByEvent { lines: 4 })
            ]
        ));
    }
}
