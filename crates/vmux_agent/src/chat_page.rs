//! The `vmux://agent` chat page: a native Dioxus UI that renders an agent session's
//! conversation + run-state (pushed from ECS) and sends prompt/approval intents back.
//! This is the single agent front-end; it replaced the legacy CLI-install setup page.

pub mod event;

#[cfg(target_arch = "wasm32")]
pub mod page;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy_cef::prelude::{BinEventEmitterPlugin, BinHostEmitEvent, BinReceive, Browsers};

#[cfg(not(target_arch = "wasm32"))]
use crate::chat_page::event::{CHAT_SNAPSHOT_EVENT, ChatApproval, ChatSnapshot, ChatSubmit};
#[cfg(not(target_arch = "wasm32"))]
use crate::components::{AgentMessages, PendingUserInput};
#[cfg(not(target_arch = "wasm32"))]
use crate::events::{AgentApprovalReply, ApprovalDecision};
#[cfg(not(target_arch = "wasm32"))]
use crate::run_state::AgentRunState;

#[cfg(not(target_arch = "wasm32"))]
pub const PAGE_MANIFEST: vmux_core::page::PageManifest = vmux_core::page::PageManifest {
    host: "agent",
    title: "Agent",
    keywords: &["ai", "chat", "assistant", "agent"],
    icon: Some(vmux_core::BuiltinIcon::Sparkles),
    command_bar: false,
};

#[cfg(not(target_arch = "wasm32"))]
pub struct AgentChatPagePlugin;

#[cfg(not(target_arch = "wasm32"))]
impl Plugin for AgentChatPagePlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().spawn(PAGE_MANIFEST);
        app.add_plugins(BinEventEmitterPlugin::<(ChatSubmit, ChatApproval)>::for_hosts(&["agent"]))
            .add_observer(on_chat_submit)
            .add_observer(on_chat_approval)
            .add_systems(Update, (push_chat_to_page, push_chat_on_ready));
    }
}

/// When a chat page first signals ready, push the current snapshot (the `Changed` push
/// alone would miss state that settled before the webview loaded — e.g. on restore).
#[cfg(not(target_arch = "wasm32"))]
fn push_chat_on_ready(
    newly_ready: Query<Entity, bevy::ecs::query::Added<vmux_core::page::PageReady>>,
    child_of: Query<&ChildOf>,
    sessions: Query<(&AgentMessages, &AgentRunState)>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for webview in &newly_ready {
        let Ok(parent) = child_of.get(webview) else {
            continue;
        };
        let Ok((messages, state)) = sessions.get(parent.parent()) else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &snapshot_of(messages, state),
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn snapshot_of(messages: &AgentMessages, state: &AgentRunState) -> ChatSnapshot {
    let messages_json = serde_json::to_string(&messages.0).unwrap_or_else(|_| "[]".to_string());
    let (status, error, call_id, name) = match state {
        AgentRunState::Idle => ("idle", String::new(), String::new(), String::new()),
        AgentRunState::Streaming => ("streaming", String::new(), String::new(), String::new()),
        AgentRunState::AwaitingApproval { call_id, name, .. } => {
            ("awaiting", String::new(), call_id.clone(), name.clone())
        }
        AgentRunState::Errored(message) => {
            ("errored", message.clone(), String::new(), String::new())
        }
    };
    ChatSnapshot {
        messages_json,
        status: status.to_string(),
        error,
        approval_call_id: call_id,
        approval_name: name,
    }
}

/// Push each changed session's conversation + run-state to its pane webview (the child
/// `Browser` of the session entity).
#[cfg(not(target_arch = "wasm32"))]
fn push_chat_to_page(
    sessions: Query<
        (Entity, &AgentMessages, &AgentRunState),
        Or<(Changed<AgentMessages>, Changed<AgentRunState>)>,
    >,
    children: Query<&Children>,
    is_browser: Query<(), With<vmux_layout::Browser>>,
    browsers: NonSend<Browsers>,
    mut commands: Commands,
) {
    for (stack, messages, state) in &sessions {
        let Ok(kids) = children.get(stack) else {
            continue;
        };
        let Some(webview) = kids.iter().find(|&e| is_browser.contains(e)) else {
            continue;
        };
        if !browsers.has_browser(webview) || !browsers.host_emit_ready(&webview) {
            continue;
        }
        commands.trigger(BinHostEmitEvent::from_rkyv(
            webview,
            CHAT_SNAPSHOT_EVENT,
            &snapshot_of(messages, state),
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_submit(
    trigger: On<BinReceive<ChatSubmit>>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let text = trigger.event().payload.text.clone();
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    commands
        .entity(parent.parent())
        .insert(PendingUserInput(text));
}

#[cfg(not(target_arch = "wasm32"))]
fn on_chat_approval(
    trigger: On<BinReceive<ChatApproval>>,
    child_of: Query<&ChildOf>,
    mut commands: Commands,
) {
    let webview = trigger.event().webview;
    let payload = &trigger.event().payload;
    let Ok(parent) = child_of.get(webview) else {
        return;
    };
    let decision = match payload.decision {
        1 => ApprovalDecision::Allow,
        2 => ApprovalDecision::AllowAlways,
        _ => ApprovalDecision::Deny,
    };
    commands.trigger(AgentApprovalReply {
        session: parent.parent(),
        call_id: payload.call_id.clone(),
        decision,
    });
}
