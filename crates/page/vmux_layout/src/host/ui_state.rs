use bevy::prelude::*;
use bevy_cef::prelude::{BinHostEmitEvent, Browsers};
use rkyv::api::high::HighSerializer;
use rkyv::ser::allocator::ArenaHandle;
use rkyv::util::AlignedVec;
use vmux_api::HostEvent;

use crate::ui_state::{LayoutUiStateEvent, LayoutUiStatePatch};

pub(crate) struct UiStatePlugin;

impl Plugin for UiStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(collect).add_systems(Last, emit);
    }
}

#[derive(Component, Default)]
pub struct LayoutUiStateUpdates {
    sequence: u64,
    patches: Vec<LayoutUiStatePatch>,
}

impl LayoutUiStateUpdates {
    pub fn write<T>(commands: &mut Commands, webview: Entity, event: &T)
    where
        T: Clone + Into<LayoutUiStatePatch>,
    {
        commands.trigger(LayoutUiStateWrite::from_event(webview, event));
    }

    pub fn deliver<T>(
        pages: &Query<(), With<Self>>,
        commands: &mut Commands,
        webview: Entity,
        event: &T,
    ) where
        T: HostEvent
            + Clone
            + Into<LayoutUiStatePatch>
            + for<'a> rkyv::Serialize<
                HighSerializer<AlignedVec, ArenaHandle<'a>, rkyv::rancor::Error>,
            >,
    {
        if pages.contains(webview) {
            Self::write(commands, webview, event);
        } else {
            commands.trigger(BinHostEmitEvent::from_event(webview, event));
        }
    }

    fn push(&mut self, patch: LayoutUiStatePatch) {
        self.patches.push(patch);
    }

    fn take(&mut self) -> Option<LayoutUiStateEvent> {
        if self.patches.is_empty() {
            return None;
        }
        self.sequence = self.sequence.wrapping_add(1).max(1);
        Some(LayoutUiStateEvent {
            sequence: self.sequence,
            patches: std::mem::take(&mut self.patches),
        })
    }
}

#[derive(Clone, EntityEvent)]
struct LayoutUiStateWrite {
    #[event_target]
    webview: Entity,
    patch: LayoutUiStatePatch,
}

impl LayoutUiStateWrite {
    fn from_event<T>(webview: Entity, event: &T) -> Self
    where
        T: Clone + Into<LayoutUiStatePatch>,
    {
        Self {
            webview,
            patch: event.clone().into(),
        }
    }
}

fn collect(trigger: On<LayoutUiStateWrite>, mut updates: Query<&mut LayoutUiStateUpdates>) {
    let Ok(mut updates) = updates.get_mut(trigger.event().webview) else {
        return;
    };
    updates.push(trigger.event().patch.clone());
}

fn emit(
    mut updates: Query<(Entity, &mut LayoutUiStateUpdates)>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{LayoutStateEvent, StacksHostEvent};
    use vmux_api::BinEvent;

    #[derive(Resource, Default)]
    struct Emitted(Vec<LayoutUiStateEvent>);

    impl Emitted {
        fn record(trigger: On<BinHostEmitEvent>, mut emitted: ResMut<Self>) {
            if trigger.event().id() != LayoutUiStateEvent::id() {
                return;
            }
            let event = rkyv::from_bytes::<LayoutUiStateEvent, rkyv::rancor::Error>(
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
        let entity = app.world_mut().spawn(LayoutUiStateUpdates::default()).id();
        let mut browsers = Browsers::default();
        browsers.set_externally_hosted(entity);
        app.world_mut().insert_non_send(browsers);

        app.world_mut().trigger(LayoutUiStateWrite::from_event(
            entity,
            &LayoutStateEvent::default(),
        ));
        app.world_mut().trigger(LayoutUiStateWrite::from_event(
            entity,
            &StacksHostEvent::default(),
        ));
        app.update();

        let emitted = &app.world().resource::<Emitted>().0;
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].sequence, 1);
        assert!(matches!(
            emitted[0].patches.as_slice(),
            [LayoutUiStatePatch::Layout(_), LayoutUiStatePatch::Stacks(_)]
        ));
    }
}
