use crate::event::{
    ChatAttachments, ChatBranchesState, ChatComposerEffect, ChatKey, ChatMediaState,
    ChatResumeState, ChatSnapshot, ChatTranscriptState, ComposerContext, ModeState, ModelState,
    SlashCommands,
};
use bevy_app::{App, Last, Plugin};
use bevy_ecs::prelude::*;
use vmux_api::page::PageEmit;

pub struct ChatUiStatePlugin;

impl Plugin for ChatUiStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatUiStateProjection>()
            .add_message::<PageEmit>()
            .add_systems(Last, emit_ui_state);
    }
}

#[vmux_api::ui_state_patch(Default)]
pub struct ChatUiStatePatch {
    pub snapshot: Option<Box<ChatSnapshot>>,
    pub composer: Option<ComposerContext>,
    pub mode: Option<ModeState>,
    pub model: Option<ModelState>,
    pub slash_commands: Option<SlashCommands>,
    pub key: Option<ChatKey>,
    pub transcript: Option<Box<ChatTranscriptState>>,
    pub attachments: Option<Box<ChatAttachments>>,
    pub media: Option<Box<ChatMediaState>>,
    pub branches: Option<Box<ChatBranchesState>>,
    pub resume: Option<Box<ChatResumeState>>,
    pub composer_effect: Option<ChatComposerEffect>,
}

#[vmux_api::ui_state(Default, targets = ["sessions", "agent", "start"])]
pub struct ChatUiState {
    pub sequence: u64,
    pub patches: Vec<ChatUiStatePatch>,
}

#[derive(Resource, Default)]
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

fn emit_ui_state(
    mut projection: ResMut<ChatUiStateProjection>,
    mut emits: MessageWriter<PageEmit>,
) {
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
