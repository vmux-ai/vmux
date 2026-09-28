use crate::event::{
    ChatAttachments, ChatBranchesState, ChatComposerEffect, ChatHistoryMoveEffect,
    ChatListChooseEffect, ChatListMoveEffect, ChatMediaState, ChatPromptFocusEffect,
    ChatResumeState, ChatSelectorDismissEffect, ChatSnapshot, ChatTranscriptState, ComposerContext,
    ModeState, ModelState, SlashCommands,
};
use crate::model::{Models, Picker};
use crate::prompt::{AttachmentPreviews, Attachments, Browsed, Media};
use crate::room::{Agents, Conversation, LiveTurn, Log, RoomTranscript, Snapshot};
use bevy_app::{App, Last, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
use vmux_api::page::PageEmit;

pub struct ChatUiStatePlugin;

impl Plugin for ChatUiStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PageEmit>()
            .add_message::<PublishComposerEffect>()
            .add_message::<RepublishChatUiState>()
            .add_systems(Startup, spawn_chat_runtime)
            .add_systems(Update, publish_composer_effects)
            .add_systems(Last, emit_ui_state);
    }
}

#[derive(Component)]
pub struct ChatRuntime;

#[derive(Message)]
pub struct PublishComposerEffect(pub ChatComposerEffect);

#[derive(Message)]
pub struct RepublishChatUiState;

#[vmux_api::ui_state_patch(Default)]
pub struct ChatUiStatePatch {
    pub snapshot: Option<Box<ChatSnapshot>>,
    pub composer: Option<ComposerContext>,
    pub mode: Option<ModeState>,
    pub model: Option<ModelState>,
    pub slash_commands: Option<SlashCommands>,
    pub list_move: Option<ChatListMoveEffect>,
    pub list_choose: Option<ChatListChooseEffect>,
    pub history_move: Option<ChatHistoryMoveEffect>,
    pub selector_dismiss: Option<ChatSelectorDismissEffect>,
    pub transcript: Option<Box<ChatTranscriptState>>,
    pub attachments: Option<Box<ChatAttachments>>,
    pub media: Option<Box<ChatMediaState>>,
    pub branches: Option<Box<ChatBranchesState>>,
    pub resume: Option<Box<ChatResumeState>>,
    pub composer_effect: Option<ChatComposerEffect>,
    pub prompt_focus: Option<ChatPromptFocusEffect>,
}

#[vmux_api::ui_state(Default)]
pub struct ChatUiState {
    pub sequence: u64,
    pub patches: Vec<ChatUiStatePatch>,
}

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

fn spawn_chat_runtime(mut commands: Commands) {
    commands.spawn((
        ChatRuntime,
        ChatUiStateProjection::default(),
        Models::default(),
        Picker::default(),
        Attachments::default(),
        crate::prompt::ChatPromptFocusRevision::default(),
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

fn emit_ui_state(
    mut runtimes: Query<&mut ChatUiStateProjection, With<ChatRuntime>>,
    mut emits: MessageWriter<PageEmit>,
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
    let Some(emit) = PageEmit::from_state(&state) else {
        return;
    };
    emits.write(emit);
}

#[cfg(test)]
mod tests {
    use super::*;

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
