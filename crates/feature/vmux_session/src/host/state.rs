use super::conversation::{Agents, Conversation, ConversationTranscript, LiveTurn, Log, Snapshot};
use super::model::{Models, Picker};
use super::prompt::{AttachmentPreviews, Attachments, Browsed, Media};
use crate::event::ChatComposerEffect;
use crate::state::{ChatUiState, ChatUiStatePatch};
use bevy_app::{App, Last, Plugin, Startup, Update};
use bevy_ecs::prelude::*;
#[cfg(not(host))]
use vmux_api::page::UiStateEmit;
#[cfg(host)]
use vmux_ecs::{UiStatePlugin, UiStateWrite};

pub struct ChatUiStatePlugin;

impl Plugin for ChatUiStatePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(host))]
        app.add_message::<UiStateEmit>();
        #[cfg(host)]
        app.add_plugins(UiStatePlugin::<ChatUiState>::default())
            .add_observer(project_mcp);
        app.add_message::<PublishComposerEffect>()
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
    #[cfg(not(host))]
    state: ChatUiState,
    patches: Vec<ChatUiStatePatch>,
}

impl ChatUiStateProjection {
    pub fn write<T>(&mut self, payload: &T)
    where
        T: Clone + Into<ChatUiStatePatch>,
    {
        let patch = payload.clone().into();
        #[cfg(not(host))]
        vmux_api::UiStateProjection::apply(&mut self.state, patch.clone());
        self.patches.push(patch);
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
        ConversationTranscript::default(),
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

#[cfg(host)]
fn project_mcp(
    trigger: On<UiStateWrite<vmux_api::mcp::McpServersUiState>>,
    views: Query<(), With<super::session::ChatView>>,
    mut runtimes: Query<&mut ChatUiStateProjection, With<ChatRuntime>>,
) {
    if !views.contains(trigger.event().webview()) {
        return;
    }
    let Ok(mut projection) = runtimes.single_mut() else {
        return;
    };
    projection.write(trigger.event().update());
}

#[cfg(not(host))]
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
    projection.patches.clear();
    let Some(emit) = UiStateEmit::from_state(&projection.state) else {
        return;
    };
    emits.write(emit);
}

#[cfg(host)]
fn emit_ui(
    mut runtimes: Query<&mut ChatUiStateProjection, With<ChatRuntime>>,
    targets: Query<Entity, With<super::session::ChatView>>,
    mut commands: Commands,
) {
    let mut targets = targets.iter().peekable();
    if targets.peek().is_none() {
        return;
    }
    let Ok(mut projection) = runtimes.single_mut() else {
        return;
    };
    let patches = std::mem::take(&mut projection.patches);
    for target in targets {
        for patch in &patches {
            commands.trigger(UiStateWrite::<ChatUiState>::from_event(target, patch));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{ChatSnapshot, ModelState};

    #[test]
    fn patches_build_a_retained_tree() {
        let event = <ChatUiState as vmux_api::UiState>::from_updates(
            None,
            vec![ChatSnapshot::default().into(), ModelState::default().into()],
        );
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&event).unwrap();
        let decoded = rkyv::from_bytes::<ChatUiState, rkyv::rancor::Error>(&bytes).unwrap();
        assert!(decoded.snapshot_ready);
        assert!(decoded.model_ready);
    }
}
