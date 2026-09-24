use bevy::prelude::*;
use bevy_cef::prelude::{BinHostEmitEvent, Browsers};

use vmux_chat::ui_state::{ChatUiStateEvent, ChatUiStatePatch};

pub(super) struct ChatUiStatePlugin;

impl Plugin for ChatUiStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(ChatUiStateUpdates::collect)
            .add_systems(Last, ChatUiStateUpdates::emit);
    }
}

#[derive(Component, Default)]
pub(super) struct ChatUiStateUpdates {
    sequence: u64,
    patches: Vec<ChatUiStatePatch>,
}

impl ChatUiStateUpdates {
    pub(super) fn write<T>(commands: &mut Commands, webview: Entity, event: &T)
    where
        T: Clone + Into<ChatUiStatePatch>,
    {
        commands.trigger(ChatUiStateWrite::new(webview, event));
    }

    fn push(&mut self, patch: ChatUiStatePatch) {
        self.patches.push(patch);
    }

    fn take(&mut self) -> Option<ChatUiStateEvent> {
        if self.patches.is_empty() {
            return None;
        }
        self.sequence = self.sequence.wrapping_add(1).max(1);
        Some(ChatUiStateEvent {
            sequence: self.sequence,
            patches: std::mem::take(&mut self.patches),
        })
    }

    fn collect(trigger: On<ChatUiStateWrite>, mut updates: Query<&mut Self>) {
        let Ok(mut updates) = updates.get_mut(trigger.event().webview) else {
            return;
        };
        updates.push(trigger.event().patch.clone());
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
            if !browsers.can_emit_to(&entity) {
                continue;
            }
            let Some(event) = updates.take() else {
                continue;
            };
            commands.trigger(BinHostEmitEvent::from_event(entity, &event));
        }
    }
}

#[derive(Clone, EntityEvent)]
struct ChatUiStateWrite {
    #[event_target]
    webview: Entity,
    patch: ChatUiStatePatch,
}

impl ChatUiStateWrite {
    fn new<T>(webview: Entity, event: &T) -> Self
    where
        T: Clone + Into<ChatUiStatePatch>,
    {
        Self {
            webview,
            patch: event.clone().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmux_api::BinEvent;
    use vmux_chat::event::{ChatSnapshot, ModelState};

    #[derive(Resource, Default)]
    struct Emitted(Vec<ChatUiStateEvent>);

    impl Emitted {
        fn record(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Self>) {
            if trigger.event().id() != ChatUiStateEvent::id() {
                return;
            }
            let event = rkyv::from_bytes::<ChatUiStateEvent, rkyv::rancor::Error>(
                trigger.event().payload(),
            )
            .unwrap();
            emitted.0.push(event);
        }
    }

    #[test]
    fn same_frame_updates_emit_one_ordered_batch() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, ChatUiStatePlugin))
            .init_resource::<Emitted>()
            .add_observer(Emitted::record);
        let entity = app.world_mut().spawn(ChatUiStateUpdates::default()).id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);

        app.world_mut()
            .trigger(ChatUiStateWrite::new(entity, &ChatSnapshot::default()));
        app.world_mut()
            .trigger(ChatUiStateWrite::new(entity, &ModelState::default()));
        app.update();

        let emitted = &app.world().resource::<Emitted>().0;
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].sequence, 1);
        assert!(matches!(
            emitted[0].patches.as_slice(),
            [ChatUiStatePatch::Snapshot(_), ChatUiStatePatch::Model(_)]
        ));
    }
}
