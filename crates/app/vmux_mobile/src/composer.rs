//! What sits under the prompt box, and what submitting it does.

use crate::api::{Api, ApiError, next_client_op_id};
use dioxus::prelude::*;
use vmux_chat::page::composer::options::{EffortMenu, ModelPill};
use vmux_wire::room::{
    AgentAttachment, PromptRequest, RemoteMediaEntry, RemoteModelState, RemoteStatus,
    inline_media_query, replace_inline_media_query,
};

/// The model and effort pickers under the composer.
///
/// Fetched per session rather than carried on [`RemoteSession`], because the list arrives from the
/// agent after the session exists and a stale copy would offer models it has since dropped.
#[component]
pub(crate) fn ComposerOptions(
    state: Signal<RemoteModelState>,
    sid: String,
    api: Signal<Option<Api>>,
    mut draft: Signal<String>,
) -> Element {
    let current = state();
    if current.models.is_empty() && current.effort_levels.is_empty() {
        return rsx! {
            div { class: "truncate text-[10px] text-muted-foreground/55", "Enter to send" }
        };
    }
    let current_name = current
        .models
        .iter()
        .find(|model| model.id == current.selected_id)
        .map(|model| model.name.clone())
        .unwrap_or_default();
    rsx! {
        div { class: "flex min-w-0 flex-1 items-center gap-1 overflow-x-auto",
            ModelPill {
                name: current_name,
                // The software keyboard is up whenever the composer has focus, so `/model` filters
                // here exactly as it does on the desktop.
                on_open: move |_| draft.set("/model ".to_string()),
            }
            EffortMenu {
                levels: current.effort_levels.clone(),
                selected: current.effort.clone(),
                on_select: {
                    let sid = sid.clone();
                    move |level: String| {
                        let (sid, level) = (sid.clone(), level);
                        let Some(client) = api.peek().clone() else { return };
                        state.write().effort = level.clone();
                        spawn(async move {
                            let _ = client.set_effort(&sid, &level).await;
                        });
                    }
                },
            }
        }
    }
}

/// The session's models and effort, re-read whenever the session changes.
///
/// Fetched per session rather than carried on [`RemoteSession`], because the list arrives from the
/// agent after the session exists and a stale copy would offer models it has since dropped.
pub(crate) fn use_remote_model_state(
    sid: String,
    api: Signal<Option<Api>>,
) -> Signal<RemoteModelState> {
    let mut state = use_signal(RemoteModelState::default);
    use_effect(use_reactive!(|sid| {
        // Read reactively: pairing can finish after a session is selected, and a peek here would
        // leave the pickers empty until the next session change.
        let Some(client) = api() else {
            return;
        };
        if sid.is_empty() {
            state.set(RemoteModelState::default());
            return;
        }
        spawn(async move {
            if let Ok(fetched) = client.models(&sid).await {
                state.set(fetched);
            }
        });
    }));
    state
}

pub(crate) fn submit_remote_prompt(
    api: Signal<Option<Api>>,
    sid: String,
    mut draft: Signal<String>,
    mut attachments: Signal<Vec<RemoteMediaEntry>>,
    mut status: Signal<RemoteStatus>,
) {
    let Some(client) = api() else { return };
    let text = draft.peek().trim().to_string();
    let selected = attachments.peek().clone();
    if sid.is_empty() || (text.is_empty() && selected.is_empty()) {
        return;
    }
    let attachments_to_submit = selected
        .into_iter()
        .filter(|attachment| !attachment.is_dir)
        .map(|attachment| AgentAttachment {
            path: attachment.path,
            name: attachment.name,
            mime_type: attachment.mime_type,
            size: attachment.size,
        })
        .collect();
    draft.set(String::new());
    attachments.set(Vec::new());
    status.set(RemoteStatus::Streaming);
    spawn(async move {
        if let Err(ApiError::Message(message)) = client
            .send_prompt(
                &sid,
                &PromptRequest {
                    client_op_id: next_client_op_id(),
                    text,
                    attachments: attachments_to_submit,
                },
            )
            .await
        {
            status.set(RemoteStatus::Errored(message));
        }
    });
}

pub(crate) fn insert_media_token(mut draft: Signal<String>) {
    let mut value = draft.peek().clone();
    if !value.is_empty() && !value.ends_with(char::is_whitespace) {
        value.push(' ');
    }
    value.push('@');
    draft.set(value);
}

pub(crate) fn select_remote_media_entry(
    entry: &RemoteMediaEntry,
    mut draft: Signal<String>,
    mut attachments: Signal<Vec<RemoteMediaEntry>>,
    mut selected: Signal<usize>,
) {
    let value = draft.peek().clone();
    let Some(query) = inline_media_query(&value) else {
        return;
    };
    let replacement = if entry.is_dir {
        format!("@{}/", entry.reference())
    } else {
        let mut next = attachments.peek().clone();
        if !next.iter().any(|attached| attached.path == entry.path) {
            next.push(entry.clone());
            attachments.set(next);
        }
        String::new()
    };
    draft.set(replace_inline_media_query(&value, query, &replacement));
    selected.set(0);
}
