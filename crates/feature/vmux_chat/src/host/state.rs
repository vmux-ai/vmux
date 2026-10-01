use super::model::{Models, Picker};
use super::prompt::{AttachmentPreviews, Attachments, Browsed, Media};
use super::room::{Agents, Conversation, LiveTurn, Log, RoomTranscript, Snapshot};
use crate::event::ChatComposerEffect;
use crate::state::{ChatUiState, ChatUiStatePatch};
use bevy_app::{App, Last, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use vmux_api::page::UiStateEmit;

pub struct ChatUiStatePlugin;

impl Plugin for ChatUiStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<UiStateEmit>()
            .add_message::<PublishComposerEffect>()
            .add_message::<RepublishChatUiState>()
            .add_systems(Startup, spawn_runtime)
            .add_systems(Update, publish_composer_effects)
            .add_systems(Last, emit_ui);
    }
}

#[derive(Component)]
pub struct ChatRuntime;

#[derive(Message)]
pub struct PublishComposerEffect(pub ChatComposerEffect);

#[derive(Message)]
pub struct RepublishChatUiState;

#[derive(Component, Default)]
pub struct ChatUiStateProjection {
    sequence: u64,
    patches: Vec<ChatUiStatePatch>,
}

impl ChatUiStateProjection {
    pub fn write<T>(&mut self, payload: &T)
    where
        T: Clone + Into<ChatUiStatePatch>,
    {
        self.patches.push(payload.clone().into());
    }
}

fn spawn_runtime(mut commands: Commands) {
    commands.spawn((
        ChatRuntime,
        ChatUiStateProjection::default(),
        Models::default(),
        Picker::default(),
        Attachments::default(),
        super::prompt::ChatPromptFocusRevision::default(),
        AttachmentPreviews::default(),
        Browsed::default(),
        Media::default(),
        Conversation::default(),
        Log::default(),
        LiveTurn::default(),
        Agents::default(),
        Snapshot::default(),
        RoomTranscript::default(),
    ));
}

fn publish_composer_effects(
    mut effects: MessageReader<PublishComposerEffect>,
    mut runtimes: Query<&mut ChatUiStateProjection, With<ChatRuntime>>,
) {
    let Ok(mut projection) = runtimes.single_mut() else {
        return;
    };
    for PublishComposerEffect(effect) in effects.read() {
        projection.write(effect);
    }
}

fn emit_ui(
    mut runtimes: Query<&mut ChatUiStateProjection, With<ChatRuntime>>,
    mut emits: MessageWriter<UiStateEmit>,
) {
    let Ok(mut projection) = runtimes.single_mut() else {
        return;
    };
    if projection.patches.is_empty() {
        return;
    }
    projection.sequence = projection.sequence.wrapping_add(1).max(1);
    let state = ChatUiState {
        sequence: projection.sequence,
        patches: std::mem::take(&mut projection.patches),
    };
    let Some(emit) = UiStateEmit::from_state(&state) else {
        return;
    };
    emits.write(emit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ChatSnapshot, ModelState};

    #[test]
    fn batches_preserve_patch_order() {
        let event = ChatUiState {
            sequence: 3,
            patches: vec![ChatSnapshot::default().into(), ModelState::default().into()],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<ChatUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(decoded.sequence, 3);
        assert!(decoded.patches[0].snapshot.is_some());
        assert!(decoded.patches[1].model.is_some());
    }
}
