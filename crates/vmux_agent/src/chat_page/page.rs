#![allow(non_snake_case)]

use crate::chat_page::composer::{
    PromptEdit, PromptHistoryDirection, ResumeMenuState, SelectorMode, approval_decision_for_index,
    chat_page_title, edit_prompt, filter_models, filter_sessions, is_handoff_boundary,
    move_prompt_history, prompt_history_direction, resume_menu_state, selector_mode,
    should_clear_draft_on_escape, should_fetch_resume,
};
use crate::chat_page::event::{
    CHAT_ATTACHMENT_PREVIEWS_EVENT, CHAT_ATTACHMENTS_EVENT, CHAT_HISTORY_PAGE_EVENT,
    CHAT_HISTORY_PAGE_SIZE, CHAT_MEDIA_ENTRIES_EVENT, CHAT_SNAPSHOT_EVENT, COMPOSER_CONTEXT_EVENT,
    ChatApproval, ChatAttachPaths, ChatAttachment, ChatAttachmentPreviewRequest, ChatAttachments,
    ChatBlock, ChatCancel, ChatCancelQueuedPrompt, ChatChoiceSelected, ChatClearQueue,
    ChatCreateWorktree, ChatEscape, ChatHistoryPage, ChatHistoryRequest, ChatItem,
    ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest, ChatOpenPage, ChatPasteMedia,
    ChatPickFiles, ChatResume, ChatSelectWorkspace, ChatSnapshot, ChatSubmit, ChatSubmitAttachment,
    ComposerContext, MODEL_STATE_EVENT, ModelOptionEntry, ModelState, QueuedPromptSnapshot,
    RESUMABLE_SESSIONS_EVENT, ResumableSessionEntry, ResumableSessions, ResumeListRequest,
    ResumeSession, RuntimeSwitchRequest, SLASH_COMMANDS_EVENT, SelectModel, SetAgentEffort,
    SlashCommandEntry, SlashCommands, latest_tool_location,
};
use crate::chat_page::scroll;
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use vmux_chat::activity::{
    ActivityIcon, activity_icon_paths, language_activity_icon, tool_activity_icon_for,
};
use vmux_chat::clipboard::copy_to_clipboard;
use vmux_chat::transcript::{ChatItemRow, MD_CSS};
#[cfg(web)]
use vmux_terminal::matrix_rain::MatrixRain;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::prompt_box::PromptPopup;
#[cfg(web)]
use vmux_ui::components::prompt_composer::prompt_textarea;
use vmux_ui::components::prompt_composer::{
    PROMPT_INPUT_ID, PromptComposer, PromptComposerAction, PromptComposerAttachment,
    focus_prompt_end,
};
use vmux_wire::prompt_media::{
    inline_media_query, media_display_path, media_reference, merge_chat_attachments,
    replace_inline_media_query,
};

use vmux_ui::components::prompt_media_options::{PromptMediaOption, PromptMediaOptions};
use vmux_ui::favicon::favicon_src_for_url;
use vmux_ui::hooks::{
    choice_number_index, menu_direction, move_selection, try_cef_bin_emit_rkyv, use_listener,
    use_selector, use_theme,
};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};

const APPROVAL_OPTION_COUNT: usize = 3;

fn set_if_changed<T: PartialEq + 'static>(mut signal: Signal<T>, value: T) {
    if signal.peek().ne(&value) {
        signal.set(value);
    }
}

fn slash_command_description(command: &SlashCommandEntry) -> String {
    match command.name.as_str() {
        "upload" => translate("agent-slash-attach-files"),
        "resume" => translate("agent-slash-resume-session"),
        "model" => translate("agent-slash-select-model"),
        "cli" => translate("agent-slash-continue-cli"),
        _ => command.description.clone(),
    }
}

fn session_age_label(seconds: u64) -> String {
    match seconds {
        0..=59 => translate("agent-session-just-now"),
        60..=3599 => translate_with(
            "agent-session-minutes-ago",
            &[("count", TranslationValue::Number((seconds / 60) as i64))],
        ),
        3600..=86399 => translate_with(
            "agent-session-hours-ago",
            &[("count", TranslationValue::Number((seconds / 3600) as i64))],
        ),
        _ => translate_with(
            "agent-session-days-ago",
            &[("count", TranslationValue::Number((seconds / 86400) as i64))],
        ),
    }
}

fn approval_detail_label(label: &str) -> String {
    match label {
        "Details" => translate("agent-details"),
        "Path" => translate("agent-path"),
        "Tool" => translate("agent-tool"),
        "Server" => translate("agent-server"),
        _ => label.to_string(),
    }
}

/// True when the page has a non-collapsed text selection — so Ctrl+C should copy, not interrupt.
#[cfg(web)]
fn has_text_selection() -> bool {
    web_sys::window()
        .and_then(|w| w.get_selection().ok().flatten())
        .map(|s| !s.is_collapsed())
        .unwrap_or(false)
}

/// A touch host has neither a caret nor a Ctrl+C, so the question never arises and the answer
/// that leaves the shortcut meaning "interrupt" is the right one.
#[cfg(not(web))]
fn has_text_selection() -> bool {
    false
}

/// Whether a startup/run error looks like a package registry/version block (npm 403, security
/// policy, forbidden version) — where the fix is usually pinning a different version.
fn is_version_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "403",
        "404",
        "forbidden",
        "security policy",
        "blocked",
        "eacces",
        "invalid tag",
        "einvalidtagname",
        "etarget",
        "no matching version",
        "notarget",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// The agent id from the page URL (`vmux://agent/<id>` → `<id>`); the chat UI is shared
/// across agents and only the id differs.
#[cfg(web)]
fn current_agent() -> String {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .and_then(|path| path.split('/').find(|s| !s.is_empty()).map(str::to_string))
        .unwrap_or_else(|| "agent".to_string())
}

/// A native host has no page URL to read the id out of, so it passes `agent_override` instead —
/// which takes precedence over this anyway.
#[cfg(not(web))]
fn current_agent() -> String {
    "agent".to_string()
}

/// Where the caret sits in the prompt, which decides whether Up moves within the text or recalls
/// the previous prompt.
#[cfg(web)]
fn prompt_caret() -> Option<(u32, u32)> {
    let textarea = prompt_textarea(PROMPT_INPUT_ID)?;
    let start = textarea
        .selection_start()
        .ok()
        .flatten()
        .unwrap_or_default();
    let end = textarea.selection_end().ok().flatten().unwrap_or(start);
    Some((start, end))
}

/// Nothing to measure without an element handle. Reporting the start is what makes Up recall
/// history rather than appear to do nothing.
#[cfg(not(web))]
fn prompt_caret() -> Option<(u32, u32)> {
    Some((0, 0))
}

fn hex_accent_rgb(color: &str) -> Option<(u8, u8, u8)> {
    let hex = color.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some((
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ))
}

fn normalized_accent(color: &str, fallback_rgb: &str) -> String {
    if hex_accent_rgb(color).is_some() {
        color.to_string()
    } else {
        format!("rgb({fallback_rgb})")
    }
}

fn accent_rgb(color: &str, fallback_rgb: &str) -> String {
    hex_accent_rgb(color)
        .map(|(red, green, blue)| format!("{red} {green} {blue}"))
        .unwrap_or_else(|| fallback_rgb.to_string())
}

fn request_chat_history(before: u32, mut loading: Signal<bool>) {
    if before == 0 || *loading.peek() {
        return;
    }
    if try_cef_bin_emit_rkyv(&ChatHistoryRequest {
        before,
        limit: CHAT_HISTORY_PAGE_SIZE,
    })
    .is_ok()
    {
        loading.set(true);
    }
}

fn merge_transcript_page(
    current: &mut Vec<ChatItem>,
    current_start: u32,
    incoming: Vec<ChatItem>,
    incoming_start: u32,
) -> u32 {
    if current_start <= incoming_start {
        let keep = incoming_start.saturating_sub(current_start) as usize;
        if keep <= current.len() {
            current.truncate(keep);
            current.extend(incoming);
            return current_start;
        }
    }
    *current = incoming;
    incoming_start
}

fn request_attachment_previews(
    items: &[ChatItem],
    previews: Signal<HashMap<String, ChatAttachment>>,
    mut requests: Signal<HashSet<String>>,
) {
    let known = previews.peek().keys().cloned().collect::<HashSet<_>>();
    let mut requested = requests.peek().clone();
    let paths = items
        .iter()
        .filter_map(|item| match item {
            ChatItem::User { attachments, .. } => Some(attachments),
            _ => None,
        })
        .flatten()
        .filter(|attachment| attachment.mime_type.starts_with("image/"))
        .filter(|attachment| {
            !known.contains(&attachment.path) && requested.insert(attachment.path.clone())
        })
        .map(|attachment| attachment.path.clone())
        .collect::<Vec<_>>();
    if !paths.is_empty() && try_cef_bin_emit_rkyv(&ChatAttachmentPreviewRequest { paths }).is_ok() {
        requests.set(requested);
    }
}
fn prompt_history(items: &[ChatItem], queued: &[QueuedPromptSnapshot]) -> Vec<String> {
    let mut history = items
        .iter()
        .filter_map(|item| match item {
            ChatItem::User { text, .. } if !text.trim().is_empty() => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    history.extend(
        queued
            .iter()
            .filter(|prompt| !prompt.text.trim().is_empty())
            .map(|prompt| prompt.text.clone()),
    );
    history
}

fn composer_activity_counts(items: &[ChatItem]) -> (usize, usize) {
    let mut subagents = 0usize;
    let mut tasks = 0usize;
    for item in items {
        let ChatItem::Turn(turn) = item else {
            continue;
        };
        for block in &turn.blocks {
            match block {
                ChatBlock::Subagent(subagent) if subagent.status == "in_progress" => {
                    subagents += 1;
                }
                ChatBlock::Plan { steps } => {
                    tasks += steps
                        .iter()
                        .filter(|step| step.status != "completed")
                        .count();
                }
                _ => {}
            }
        }
    }
    (subagents, tasks)
}

fn file_extension_label(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_uppercase())
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "FILE".to_string())
}

fn attachment_label(attachment: &ChatAttachment) -> String {
    file_extension_label(&attachment.name)
}

fn select_media_entry(
    entry: &ChatMediaEntry,
    mut draft: Signal<String>,
    mut menu_sel: Signal<usize>,
) {
    let value = draft.peek().clone();
    let Some(query) = inline_media_query(&value) else {
        return;
    };
    let reference = media_reference(entry);
    let replacement = if entry.is_dir {
        format!("@{reference}/")
    } else {
        if try_cef_bin_emit_rkyv(&ChatAttachPaths {
            paths: vec![entry.path.clone()],
        })
        .is_err()
        {
            return;
        }
        String::new()
    };
    draft.set(replace_inline_media_query(&value, query, &replacement));
    menu_sel.set(0);
    focus_prompt_end(PROMPT_INPUT_ID);
}

/// The falling-glyphs backdrop shown while an agent installs.
///
/// `MatrixRain` is a canvas animation and exists only on the CEF host. Installing an agent is a
/// desktop act anyway, so a native host renders nothing rather than an approximation.
#[cfg(web)]
#[component]
fn InstallBackdrop(accent_rgb: String, title: String) -> Element {
    rsx! {
        div { class: "pointer-events-none absolute inset-0 z-0 overflow-hidden bg-background opacity-75",
            MatrixRain { accent_rgb, words: vec![title] }
        }
    }
}

#[cfg(not(web))]
#[component]
fn InstallBackdrop(accent_rgb: String, title: String) -> Element {
    // The prop names have to match the CEF impl, since callers name them.
    let _ = (accent_rgb, title);
    rsx! {}
}

#[component]
pub fn Page(
    #[props(default)] agent_override: Option<String>,
    #[props(default)] transition_prompt: Option<String>,
    #[props(default)] transition_attachments: Option<Vec<ChatAttachment>>,
) -> Element {
    use_theme();
    let agent = agent_override.unwrap_or_else(current_agent);
    let transition_preview = use_signal(|| transition_prompt.unwrap_or_default());
    let transition_attachments = use_signal(|| transition_attachments.unwrap_or_default());
    let mut items = use_signal(Vec::<ChatItem>::new);
    let mut loaded_start = use_signal(|| 0u32);
    let mut messages_total = use_signal(|| 0u32);
    let mut history_loading = use_signal(|| false);
    let mut recent_messages_json = use_signal(String::new);
    let mut recent_messages_start = use_signal(|| u32::MAX);
    let status = use_signal(|| "installing".to_string());
    let error = use_signal(String::new);
    let mut approval = use_signal(|| Option::<(String, String, String)>::None);
    let mut approval_sel = use_signal(|| 0usize);
    let agent_name = use_signal(String::new);
    let conversation_title = use_signal(String::new);
    let agent_icon = use_signal(String::new);
    let accent = use_signal(String::new);
    let handoff_source = use_signal(String::new);
    let handoff_truncated = use_signal(|| false);
    let handoff_message_count = use_signal(|| 0u32);
    let mut choice_question = use_signal(String::new);
    let mut choice_options = use_signal(Vec::<String>::new);
    let mut draft = use_signal(String::new);
    let mut attachments = use_signal(Vec::<ChatAttachment>::new);
    let mut attachment_previews = use_signal(HashMap::<String, ChatAttachment>::new);
    let attachment_preview_requests = use_signal(HashSet::<String>::new);
    let mut history_cursor = use_signal(|| None::<usize>);
    let mut history_scratch = use_signal(String::new);
    let mut at_bottom = use_signal(|| true);
    let mut last_top = use_signal(|| 0i32);
    let mut scroll_container: scroll::Container = use_signal(|| None);
    let queued = use_signal(Vec::<QueuedPromptSnapshot>::new);
    let paused = use_signal(|| false);
    let mut slash_cmds = use_signal(Vec::<SlashCommandEntry>::new);
    let mut sessions = use_signal(Vec::<ResumableSessionEntry>::new);
    let mut models = use_signal(Vec::<ModelOptionEntry>::new);
    let mut media_entries = use_signal(Vec::<ChatMediaEntry>::new);
    let mut media_request_id = use_signal(|| 0u64);
    let mut media_requested_query = use_signal(|| None::<String>);
    let mut media_loading = use_signal(|| false);
    let mut current_model_id = use_signal(String::new);
    let mut current_model = use_signal(String::new);
    let mut effort_levels = use_signal(Vec::<String>::new);
    let mut effort_current = use_signal(String::new);
    let mut effort_agent_key = use_signal(String::new);
    let mut effort_menu_open = use_signal(|| false);
    let mut composer_context = use_signal(ComposerContext::default);
    let mut menu_sel = use_signal(|| 0usize);
    let mut resume_requested = use_signal(|| false);
    let mut resume_loading = use_signal(|| false);
    let activity_counts = use_memo(move || composer_activity_counts(&items.read()));
    let latest_tool = use_memo(move || latest_tool_location(&items.read()));

    use_effect(move || focus_prompt_end(PROMPT_INPUT_ID));

    use_effect(move || {
        // Subscribe to any transcript/status change (each snapshot is a fresh `set`). Only pin to
        // the bottom when the user is already there — if they scrolled up to read, leave them.
        let _ = items.read().len();
        let _ = status.read();
        if !*at_bottom.peek() {
            return;
        }
        scroll::to_bottom(scroll_container);
    });

    let _listener = use_listener::<ChatSnapshot, _>(CHAT_SNAPSHOT_EVENT, move |snap| {
        let messages_changed = recent_messages_start() != snap.messages_start
            || *recent_messages_json.peek() != snap.messages_json;
        if messages_changed
            && let Ok(parsed) = serde_json::from_str::<Vec<ChatItem>>(&snap.messages_json)
        {
            request_attachment_previews(&parsed, attachment_previews, attachment_preview_requests);
            let start = merge_transcript_page(
                &mut items.write(),
                loaded_start(),
                parsed,
                snap.messages_start,
            );
            set_if_changed(loaded_start, start);
            recent_messages_json.set(snap.messages_json.clone());
            recent_messages_start.set(snap.messages_start);
            if start == 0 {
                set_if_changed(history_loading, false);
            }
        }
        set_if_changed(messages_total, snap.messages_total);
        set_if_changed(status, snap.status.clone());
        set_if_changed(error, snap.error.clone());
        set_if_changed(queued, snap.queued.clone());
        set_if_changed(transition_preview, String::new());
        set_if_changed(transition_attachments, Vec::new());
        set_if_changed(paused, snap.paused);
        set_if_changed(agent_name, snap.agent_name.clone());
        set_if_changed(conversation_title, snap.conversation_title.clone());
        set_if_changed(agent_icon, snap.agent_icon.clone());
        set_if_changed(accent, snap.accent_color.clone());
        set_if_changed(handoff_source, snap.handoff_source.clone());
        set_if_changed(handoff_truncated, snap.handoff_truncated);
        set_if_changed(handoff_message_count, snap.handoff_message_count);
        set_if_changed(choice_question, snap.choice_question.clone());
        if choice_options.peek().as_slice() != snap.choice_options.as_slice() {
            set_if_changed(menu_sel, 0);
            choice_options.set(snap.choice_options.clone());
        }
        let next_approval = if snap.status == "awaiting" {
            Some((
                snap.approval_call_id.clone(),
                snap.approval_name.clone(),
                snap.approval_args_json.clone(),
            ))
        } else {
            None
        };
        if approval.peek().ne(&next_approval) {
            approval.set(next_approval);
            set_if_changed(approval_sel, 0);
        }
    });
    let _history = use_listener::<ChatHistoryPage, _>(CHAT_HISTORY_PAGE_EVENT, move |page| {
        history_loading.set(false);
        if page.end != loaded_start() {
            return;
        }
        let Ok(older) = serde_json::from_str::<Vec<ChatItem>>(&page.items_json) else {
            return;
        };
        request_attachment_previews(&older, attachment_previews, attachment_preview_requests);
        let metrics = scroll::metrics(scroll_container);
        drop(items.write().splice(0..0, older));
        loaded_start.set(page.start);
        messages_total.set(page.total);
        if let Some((height, top)) = metrics {
            scroll::restore(scroll_container, height, top);
        }
    });
    let _attachments =
        use_listener::<ChatAttachments, _>(CHAT_ATTACHMENTS_EVENT, move |selected| {
            let current = attachments.peek().clone();
            attachments.set(merge_chat_attachments(&current, &selected.attachments));
            focus_prompt_end(PROMPT_INPUT_ID);
        });
    let _attachment_previews =
        use_listener::<ChatAttachments, _>(CHAT_ATTACHMENT_PREVIEWS_EVENT, move |loaded| {
            let mut previews = attachment_previews.peek().clone();
            for attachment in &loaded.attachments {
                previews.insert(attachment.path.clone(), attachment.clone());
            }
            attachment_previews.set(previews);
        });
    let _media_entries =
        use_listener::<ChatMediaEntries, _>(CHAT_MEDIA_ENTRIES_EVENT, move |response| {
            if response.request_id != media_request_id() {
                return;
            }
            media_entries.set(response.entries.clone());
            media_loading.set(false);
            menu_sel.set(0);
        });

    let _cmds = use_listener::<SlashCommands, _>(SLASH_COMMANDS_EVENT, move |s| {
        slash_cmds.set(s.commands.clone());
    });
    let _models = use_listener::<ModelState, _>(MODEL_STATE_EVENT, move |state| {
        models.set(state.models.clone());
        current_model_id.set(state.current_model_id.clone());
        current_model.set(state.current_model_name.clone());
        effort_levels.set(state.effort_levels.clone());
        effort_current.set(state.effort_current.clone());
        effort_agent_key.set(state.agent_key.clone());
        menu_sel.set(0);
    });
    let _composer_context =
        use_listener::<ComposerContext, _>(COMPOSER_CONTEXT_EVENT, move |context| {
            composer_context.set(context.clone())
        });
    let _sess = use_listener::<ResumableSessions, _>(RESUMABLE_SESSIONS_EVENT, move |s| {
        sessions.set(s.sessions.clone());
        menu_sel.set(0);
        resume_loading.set(false);
    });

    use_effect(move || {
        let should_fetch = should_fetch_resume(&draft(), &slash_cmds.read());
        if should_fetch && !resume_requested() {
            resume_loading.set(true);
            if try_cef_bin_emit_rkyv(&ResumeListRequest).is_err() {
                resume_loading.set(false);
            }
            resume_requested.set(true);
        } else if !should_fetch && resume_requested() {
            resume_requested.set(false);
            resume_loading.set(false);
        }
    });

    use_effect(move || {
        let value = draft();
        let Some(query) = inline_media_query(&value).map(|query| query.query.to_string()) else {
            media_entries.set(Vec::new());
            if media_requested_query.peek().is_some() {
                media_request_id.set(media_request_id().wrapping_add(1).max(1));
            }
            media_requested_query.set(None);
            media_loading.set(false);
            return;
        };
        if media_requested_query().as_deref() == Some(query.as_str()) {
            return;
        }
        let request_id = media_request_id().wrapping_add(1).max(1);
        media_request_id.set(request_id);
        media_requested_query.set(Some(query.clone()));
        media_entries.set(Vec::new());
        media_loading.set(true);
        if try_cef_bin_emit_rkyv(&ChatMediaListRequest { request_id, query }).is_err() {
            media_loading.set(false);
        }
    });

    let favicon_agent = agent.clone();
    use_effect(move || {
        let name = {
            let n = agent_name();
            if n.is_empty() { current_agent() } else { n }
        };
        let title = chat_page_title(&conversation_title(), &name);
        let status = status();
        let items = items.read();
        let fallback = agent_accent(&favicon_agent).rain_rgb;
        let accent = normalized_accent(&accent(), fallback);
        let href = current_activity_icon(&items, &status)
            .map(|activity| activity_favicon(activity, &accent))
            .or_else(|| {
                favicon_src_for_url(&agent_icon(), &format!("vmux://agent/{favicon_agent}"))
            })
            .unwrap_or_else(|| activity_favicon(ActivityIcon::Tool, &accent));
        set_tab_identity(&title, &href);
    });

    let header_name = {
        let n = agent_name();
        if n.is_empty() { agent.clone() } else { n }
    };
    let conversation_title = chat_page_title(&conversation_title(), &header_name);
    let agent_accent = agent_accent(&agent);
    let profile_accent = accent();
    let theme_accent = normalized_accent(&profile_accent, agent_accent.rain_rgb);
    let rain_accent = accent_rgb(&theme_accent, agent_accent.rain_rgb);
    let installing = status() == "installing";
    let installing_splash = installing && items.read().is_empty();
    let show_capability_examples = items.read().is_empty()
        && queued.read().is_empty()
        && attachments.read().is_empty()
        && transition_attachments.read().is_empty();
    let install_detail = {
        let detail = error();
        if detail.is_empty() {
            translate("agent-preparing")
        } else {
            detail
        }
    };
    let draft_val = draft();
    let selector = selector_mode(&draft_val);
    let command_query = match selector {
        SelectorMode::Commands(query) => Some(query),
        _ => None,
    };
    let resume_query = match selector {
        SelectorMode::Resume(query) => Some(query),
        _ => None,
    };
    let model_query = match selector {
        SelectorMode::Models(query) => Some(query),
        _ => None,
    };
    let media_query = inline_media_query(&draft_val);
    let filtered_cmds: Vec<SlashCommandEntry> = command_query
        .map(|query| {
            let query = query.to_lowercase();
            slash_cmds
                .read()
                .iter()
                .filter(|command| command.name.starts_with(&query))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let filtered_sessions = resume_query
        .map(|query| filter_sessions(&sessions.read(), query))
        .unwrap_or_default();
    let filtered_models = model_query
        .map(|query| filter_models(&models.read(), query))
        .unwrap_or_default();
    let cmd_menu_open = command_query.is_some() && !filtered_cmds.is_empty();
    let session_menu_open = resume_query.is_some();
    let model_menu_open = model_query.is_some();
    let media_menu_open = media_query.is_some();
    let latest_tool = latest_tool();
    let resume_state = resume_query.map(|_| {
        resume_menu_state(
            resume_requested(),
            resume_loading(),
            sessions.read().len(),
            filtered_sessions.len(),
        )
    });
    let prompt_media_options = media_entries
        .read()
        .iter()
        .map(|entry| PromptMediaOption {
            key: format!("media-{}", entry.path),
            name: entry.name.clone(),
            display_path: media_display_path(entry),
            preview_data_url: entry.preview_data_url.clone(),
            label: file_extension_label(&entry.name),
            is_dir: entry.is_dir,
        })
        .collect::<Vec<_>>();
    let prompt_attachment_previews = attachment_previews.read();
    let prompt_attachments = transition_attachments
        .read()
        .iter()
        .map(|attachment| PromptComposerAttachment {
            key: format!("transition-attachment-{}", attachment.path),
            name: attachment.name.clone(),
            label: attachment_label(attachment),
            preview_data_url: prompt_attachment_previews
                .get(&attachment.path)
                .and_then(|preview| {
                    (!preview.preview_data_url.is_empty()).then(|| preview.preview_data_url.clone())
                })
                .unwrap_or_else(|| attachment.preview_data_url.clone()),
            remove_index: None,
        })
        .chain(
            attachments
                .read()
                .iter()
                .enumerate()
                .map(|(index, attachment)| PromptComposerAttachment {
                    key: format!("attachment-pill-{}", attachment.path),
                    name: attachment.name.clone(),
                    label: attachment_label(attachment),
                    preview_data_url: prompt_attachment_previews
                        .get(&attachment.path)
                        .and_then(|preview| {
                            (!preview.preview_data_url.is_empty())
                                .then(|| preview.preview_data_url.clone())
                        })
                        .unwrap_or_else(|| attachment.preview_data_url.clone()),
                    remove_index: Some(index),
                }),
        )
        .collect::<Vec<_>>();
    let prompt_streaming = matches!(status().as_str(), "streaming" | "awaiting");
    let prompt_action = if prompt_streaming && queued.read().is_empty() {
        PromptComposerAction::Stop
    } else {
        PromptComposerAction::Send
    };
    let prompt_action_title = if prompt_streaming && !queued.read().is_empty() {
        translate("agent-send-all-queued")
    } else if prompt_streaming {
        translate("common-stop")
    } else {
        translate("agent-send")
    };
    let choice_pending = !choice_options.read().is_empty() || approval.read().is_some();
    let prompt_action_enabled = !choice_pending
        && (prompt_streaming || !draft_val.trim().is_empty() || !attachments.read().is_empty());
    let prompt_keydown = move |e: KeyboardEvent| {
        // The page root also listens, to catch typing aimed elsewhere. Stopping here is what
        // tells it this keystroke already had a home — and it is why composition can never
        // reach the root, since an IME only ever composes into a focused field.
        e.stop_propagation();
        let active_approval = { approval.peek().clone() };
        if let Some((call_id, _, _)) = active_approval {
            let key = e.key().to_string();
            if !e.modifiers().meta()
                && !e.modifiers().alt()
                && let Some(direction) = menu_direction(&key, e.modifiers().ctrl())
            {
                e.prevent_default();
                approval_sel.set(move_selection(
                    approval_sel(),
                    APPROVAL_OPTION_COUNT,
                    direction,
                ));
                return;
            }
            let numbered = !e.modifiers().meta()
                && !e.modifiers().ctrl()
                && !e.modifiers().alt()
                && choice_number_index(&key, APPROVAL_OPTION_COUNT).is_some();
            let entered = e.key() == Key::Enter
                && !e.modifiers().shift()
                && !e.modifiers().meta()
                && !e.modifiers().ctrl()
                && !e.modifiers().alt();
            if numbered || entered {
                e.prevent_default();
                let index =
                    choice_number_index(&key, APPROVAL_OPTION_COUNT).unwrap_or(approval_sel());
                if let Some(decision) = approval_decision_for_index(index)
                    && send_approval(call_id, decision)
                {
                    approval.set(None);
                    approval_sel.set(0);
                }
                return;
            }
        }
        let pending_choices = choice_options.peek().clone();
        if !pending_choices.is_empty() {
            let key = e.key().to_string();
            if !e.modifiers().meta()
                && !e.modifiers().alt()
                && let Some(direction) = menu_direction(&key, e.modifiers().ctrl())
            {
                e.prevent_default();
                let selected = *menu_sel.peek();
                menu_sel.set(move_selection(selected, pending_choices.len(), direction));
                return;
            }
            let numbered = !e.modifiers().meta()
                && !e.modifiers().ctrl()
                && !e.modifiers().alt()
                && choice_number_index(&key, pending_choices.len()).is_some();
            let entered = e.key() == Key::Enter
                && !e.modifiers().shift()
                && !e.modifiers().meta()
                && !e.modifiers().ctrl()
                && !e.modifiers().alt();
            if numbered || entered {
                e.prevent_default();
                let selected = *menu_sel.peek();
                let index = choice_number_index(&key, pending_choices.len()).unwrap_or(selected);
                if try_cef_bin_emit_rkyv(&ChatChoiceSelected {
                    index: index as u32,
                })
                .is_ok()
                {
                    choice_question.set(String::new());
                    choice_options.set(Vec::new());
                    menu_sel.set(0);
                }
                return;
            }
        }
        let streaming = matches!(status().as_str(), "streaming" | "awaiting");
        let draft_now = draft.peek().clone();
        let (cmd_items, sess_items, model_items, session_selector_open, model_selector_open) =
            match selector_mode(&draft_now) {
                SelectorMode::Commands(query) => {
                    let query = query.to_lowercase();
                    (
                        slash_cmds
                            .peek()
                            .iter()
                            .filter(|command| command.name.starts_with(&query))
                            .cloned()
                            .collect::<Vec<_>>(),
                        Vec::new(),
                        Vec::new(),
                        false,
                        false,
                    )
                }
                SelectorMode::Resume(query) => (
                    Vec::new(),
                    filter_sessions(&sessions.peek(), query),
                    Vec::new(),
                    true,
                    false,
                ),
                SelectorMode::Models(query) => (
                    Vec::new(),
                    Vec::new(),
                    filter_models(&models.peek(), query),
                    false,
                    true,
                ),
                SelectorMode::None => (Vec::new(), Vec::new(), Vec::new(), false, false),
            };
        let media_selector_open = inline_media_query(&draft_now).is_some();
        let media_items = if media_selector_open {
            media_entries.peek().clone()
        } else {
            Vec::new()
        };
        let selector_open = media_selector_open
            || session_selector_open
            || model_selector_open
            || !cmd_items.is_empty();
        let selector_len = if media_selector_open {
            media_items.len()
        } else if session_selector_open {
            sess_items.len()
        } else if model_selector_open {
            model_items.len()
        } else {
            cmd_items.len()
        };
        let key = e.key().to_string();
        let command_modifier = e.modifiers().meta() || e.modifiers().ctrl() || e.modifiers().alt();
        let direction = if e.modifiers().meta() || e.modifiers().alt() {
            None
        } else {
            menu_direction(&key, e.modifiers().ctrl())
        };

        if selector_open && let Some(direction) = direction {
            e.prevent_default();
            let selected = *menu_sel.peek();
            menu_sel.set(move_selection(selected, selector_len, direction));
            return;
        }
        if selector_open && e.key() == Key::Enter && !e.modifiers().shift() && !command_modifier {
            e.prevent_default();
            let selected = *menu_sel.peek();
            if media_selector_open {
                if let Some(entry) = media_items.get(selected) {
                    select_media_entry(entry, draft, menu_sel);
                }
            } else if session_selector_open {
                if let Some(session) = sess_items.get(selected) {
                    select_resume_session(session, draft);
                }
            } else if model_selector_open {
                if let Some(model) = model_items.get(selected) {
                    select_model(model, draft);
                }
            } else if let Some(command) = cmd_items.get(selected) {
                run_slash_command(&command.name, draft, menu_sel);
            }
            return;
        }
        if selector_open && e.key() == Key::Escape && !command_modifier {
            e.prevent_default();
            if let Some(query) = inline_media_query(&draft_now) {
                draft.set(replace_inline_media_query(&draft_now, query, ""));
                focus_prompt_end(PROMPT_INPUT_ID);
            } else {
                draft.set(String::new());
            }
            menu_sel.set(0);
            return;
        }
        if (media_selector_open || session_selector_open || model_selector_open)
            && matches!(e.key(), Key::Enter | Key::Escape)
        {
            return;
        }

        if !selector_open
            && !e.modifiers().meta()
            && !e.modifiers().alt()
            && let Some((start, end)) = prompt_caret()
            && let Some(direction) =
                prompt_history_direction(&key, e.modifiers().ctrl(), &draft_now, start, end)
        {
            {
                let history = prompt_history(&items.peek(), &queued.peek());
                let current_cursor = *history_cursor.peek();
                let should_handle = match direction {
                    PromptHistoryDirection::Older => !history.is_empty(),
                    PromptHistoryDirection::Newer => current_cursor.is_some(),
                };
                if should_handle {
                    e.prevent_default();
                    let (value, cursor, scratch) = move_prompt_history(
                        &history,
                        current_cursor,
                        &history_scratch.peek(),
                        &draft_now,
                        direction,
                    );
                    draft.set(value);
                    history_cursor.set(cursor);
                    history_scratch.set(scratch);
                    focus_prompt_end(PROMPT_INPUT_ID);
                    return;
                }
            }
        }

        if e.key() == Key::Enter && !e.modifiers().shift() {
            e.prevent_default();
            do_submit(
                draft,
                attachments,
                history_cursor,
                history_scratch,
                at_bottom,
            );
        } else if e.key() == Key::Escape {
            e.prevent_default();
            let _ = try_cef_bin_emit_rkyv(&ChatEscape);
            if should_clear_draft_on_escape(
                streaming,
                queued.peek().is_empty(),
                draft.peek().is_empty(),
            ) {
                draft.set(String::new());
            }
        } else if e.modifiers().ctrl()
            && matches!(e.key(), Key::Character(c) if c == "c")
            && !has_text_selection()
        {
            e.prevent_default();
            let _ = try_cef_bin_emit_rkyv(&ChatCancel);
        }
    };

    // Keys that arrive with the prompt unfocused. The composer stops propagation while it has
    // focus, so anything reaching the page root was aimed somewhere else — the transcript, a
    // button — and the affordance is that typing still goes to the prompt.
    let root_keydown = move |e: KeyboardEvent| {
        let key = e.key().to_string();
        let modifiers = e.modifiers();
        let selector_open = {
            let draft_value = draft.peek();
            inline_media_query(&draft_value).is_some()
                || match selector_mode(&draft_value) {
                    SelectorMode::Resume(_) | SelectorMode::Models(_) => true,
                    SelectorMode::Commands(query) => {
                        let query = query.to_lowercase();
                        slash_cmds
                            .peek()
                            .iter()
                            .any(|command| command.name.starts_with(&query))
                    }
                    SelectorMode::None => false,
                }
        };
        let approval_open = approval.peek().is_some();
        let choice_len = choice_options.peek().len();
        let unmodified = !modifiers.meta() && !modifiers.ctrl() && !modifiers.alt();
        let direction = if modifiers.meta() || modifiers.alt() {
            None
        } else {
            menu_direction(&key, modifiers.ctrl())
        };
        let choice_key = direction.is_some()
            || (unmodified && (key == "Enter" || choice_number_index(&key, choice_len).is_some()));
        let approval_key = direction.is_some()
            || (unmodified
                && (key == "Enter" || choice_number_index(&key, APPROVAL_OPTION_COUNT).is_some()));
        let selector_key =
            direction.is_some() || (unmodified && matches!(key.as_str(), "Enter" | "Escape"));

        // Navigation, approvals and choices mean exactly what they mean with the prompt focused,
        // so hand the event to that handler rather than reproducing its rules.
        if (approval_open && approval_key)
            || (choice_len > 0 && choice_key)
            || direction.is_some()
            || (selector_open && selector_key)
        {
            let mut forward = prompt_keydown;
            forward(e);
            return;
        }

        if !unmodified {
            return;
        }
        let edit = match key.as_str() {
            "Backspace" => PromptEdit::Backspace,
            "Delete" => PromptEdit::Delete,
            _ if key.chars().count() == 1 => PromptEdit::Insert(&key),
            _ => return,
        };
        e.prevent_default();
        // The draft signal is the source of truth, so editing it is enough — the textarea is
        // rendered from it. Appending at the end matches where focus_prompt_end puts the caret.
        let current = draft.peek().clone();
        let end = current.encode_utf16().count() as u32;
        let (value, _caret) = edit_prompt(&current, end, end, edit);
        draft.set(value);
        focus_prompt_end(PROMPT_INPUT_ID);
    };

    use_selector(menu_sel, move |selected| {
        let media_open = {
            let draft = draft.read();
            inline_media_query(&draft).is_some()
        };
        let _ = sessions.read().len();
        let _ = models.read().len();
        let _ = media_entries.read().len();
        if !choice_options.read().is_empty() {
            format!("agent-choice-item-{selected}")
        } else if media_open {
            format!("prompt-media-item-{selected}")
        } else {
            format!("agent-selector-item-{selected}")
        }
    });

    let context = composer_context();
    let model_name = current_model();
    let (active_subagents, active_tasks) = activity_counts();
    let queued_count = queued.read().len();
    let workspace_label = if context.workspace_selected && !context.workspace_name.is_empty() {
        context.workspace_name.clone()
    } else {
        "Select project".to_string()
    };
    let access_label = if context.auto_allow_count == 0 {
        "Ask".to_string()
    } else {
        format!("Ask · {} allowed", context.auto_allow_count)
    };
    let workspace_title = if context.cwd.is_empty() {
        "Choose project".to_string()
    } else {
        format!("Choose project · {}", context.cwd)
    };
    let branch_title = if context.branch.is_empty() {
        "Git repository".to_string()
    } else {
        format!("Branch {}", context.branch)
    };
    let worktree_title = if context.base_ref.is_empty() {
        "Linked worktree".to_string()
    } else {
        format!("Worktree from {}", context.base_ref)
    };
    let run_label = match status().as_str() {
        "streaming" => "Running",
        "awaiting" => "Approval",
        "installing" => "Starting",
        "errored" => "Error",
        _ => "Ready",
    };
    let composer_footer = rsx! {
        div { class: "flex min-w-0 items-center justify-between gap-1",
            div { class: "flex min-w-0 flex-1 items-center gap-1 overflow-x-auto",
                if !model_name.is_empty() {
                    button {
                        class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] font-medium text-foreground/70 transition hover:bg-foreground/[0.08] hover:text-foreground",
                        title: "Change model",
                        onmousedown: move |event| event.prevent_default(),
                        onclick: move |_| {
                            draft.set("/model ".to_string());
                            menu_sel.set(0);
                            focus_prompt_end(PROMPT_INPUT_ID);
                        },
                        svg {
                            class: "h-3.5 w-3.5 shrink-0",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.8",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M12 3l1.7 4.6L18 9.3l-4.3 1.7L12 16l-1.7-5L6 9.3l4.3-1.7L12 3Z" }
                            path { d: "M19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8L19 15Z" }
                        }
                        span { class: "truncate", "{model_name}" }
                        svg {
                            class: "h-3 w-3 shrink-0 opacity-50",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path { d: "m8 10 4 4 4-4" }
                        }
                    }
                }
                if !effort_levels().is_empty() {
                    div { class: "relative shrink-0",
                        button {
                            id: "chat-effort-trigger",
                            class: "flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] font-medium text-foreground/70 transition hover:bg-foreground/[0.08] hover:text-foreground",
                            title: translate("agent-effort-tooltip"),
                            onmousedown: move |event| event.prevent_default(),
                            onclick: move |_| {
                                let next = !effort_menu_open();
                                effort_menu_open.set(next);
                                focus_prompt_end(PROMPT_INPUT_ID);
                            },
                            svg {
                                class: "h-3.5 w-3.5 shrink-0",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.8",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M12 20a8 8 0 1 1 8-8" }
                                path { d: "M12 12l3.5-2" }
                            }
                            span { class: "truncate capitalize",
                                {if effort_current().is_empty() { translate("agent-effort") } else { effort_current() }}
                            }
                            svg {
                                class: "h-3 w-3 shrink-0 opacity-50",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path { d: "m8 10 4 4 4-4" }
                            }
                        }
                        if effort_menu_open() {
                            div { class: "absolute bottom-full left-0 z-20 mb-2 min-w-[9rem] rounded-2xl border border-foreground/10 bg-background/95 p-1.5 shadow-xl backdrop-blur-xl",
                                div { class: "px-2 pb-1 pt-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60", {translate("agent-effort")} }
                                {
                                    let key = effort_agent_key();
                                    let is_default = effort_current().is_empty();
                                    rsx! {
                                        button {
                                            class: if is_default { "flex w-full items-center gap-2 rounded-xl bg-foreground/[0.08] px-2.5 py-1.5 text-left text-sm text-foreground" } else { "flex w-full items-center gap-2 rounded-xl px-2.5 py-1.5 text-left text-sm text-foreground/75 transition hover:bg-foreground/[0.06] hover:text-foreground" },
                                            onmousedown: move |event| event.prevent_default(),
                                            onclick: move |_| {
                                                effort_current.set(String::new());
                                                effort_menu_open.set(false);
                                                let _ = try_cef_bin_emit_rkyv(&SetAgentEffort { agent_key: key.clone(), level: String::new() });
                                                focus_prompt_end(PROMPT_INPUT_ID);
                                            },
                                            span { class: "min-w-0 flex-1 truncate", {translate("agent-effort-default")} }
                                            if is_default {
                                                svg { class: "h-3.5 w-3.5 shrink-0 text-emerald-500", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.2", stroke_linecap: "round", stroke_linejoin: "round", path { d: "m5 12 4 4L19 6" } }
                                            }
                                        }
                                    }
                                }
                                for level in effort_levels() {
                                    {
                                        let level_value = level.clone();
                                        let key = effort_agent_key();
                                        let selected = level == effort_current();
                                        rsx! {
                                            button {
                                                key: "effort-{level}",
                                                class: if selected { "flex w-full items-center gap-2 rounded-xl bg-foreground/[0.08] px-2.5 py-1.5 text-left text-sm text-foreground" } else { "flex w-full items-center gap-2 rounded-xl px-2.5 py-1.5 text-left text-sm text-foreground/75 transition hover:bg-foreground/[0.06] hover:text-foreground" },
                                                onmousedown: move |event| event.prevent_default(),
                                                onclick: move |_| {
                                                    effort_current.set(level_value.clone());
                                                    effort_menu_open.set(false);
                                                    let _ = try_cef_bin_emit_rkyv(&SetAgentEffort { agent_key: key.clone(), level: level_value.clone() });
                                                    focus_prompt_end(PROMPT_INPUT_ID);
                                                },
                                                span { class: "min-w-0 flex-1 truncate capitalize", "{level}" }
                                                if selected {
                                                    svg { class: "h-3.5 w-3.5 shrink-0 text-emerald-500", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2.2", stroke_linecap: "round", stroke_linejoin: "round", path { d: "m5 12 4 4L19 6" } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                span {
                    class: "flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground",
                    title: "Tools ask before protected actions; Allow always is remembered per agent, repository or working directory, and tool",
                    svg {
                        class: "h-3.5 w-3.5",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.8",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M12 3 5 6v5c0 4.8 2.9 8.2 7 10 4.1-1.8 7-5.2 7-10V6l-7-3Z" }
                        path { d: "m9 12 2 2 4-4" }
                    }
                    "{access_label}"
                }
                if context.can_manage_workspace {
                    button {
                        class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground transition hover:bg-foreground/[0.08] hover:text-foreground",
                        title: "{workspace_title}",
                        onmousedown: move |event| event.prevent_default(),
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&ChatSelectWorkspace);
                            focus_prompt_end(PROMPT_INPUT_ID);
                        },
                        svg {
                            class: "h-3.5 w-3.5 shrink-0",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.8",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M3 6.5h6l2 2h10v9.5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6.5Z" }
                        }
                        span { class: "truncate", "{workspace_label}" }
                    }
                } else if !context.cwd.is_empty() {
                    span {
                        class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground",
                        title: "{context.cwd}",
                        svg {
                            class: "h-3.5 w-3.5 shrink-0",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.8",
                            path { d: "M3 6.5h6l2 2h10v9.5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6.5Z" }
                        }
                        span { class: "truncate", "{workspace_label}" }
                    }
                }
                if context.is_git_repo {
                    span {
                        class: "flex h-7 max-w-40 shrink-0 items-center gap-1.5 rounded-lg px-2 font-mono text-[10px] text-muted-foreground",
                        title: "{branch_title}",
                        svg {
                            class: "h-3.5 w-3.5 shrink-0",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.8",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "6", cy: "5", r: "2" }
                            circle { cx: "6", cy: "19", r: "2" }
                            circle { cx: "18", cy: "12", r: "2" }
                            path { d: "M8 5h3a3 3 0 0 1 3 3v1a3 3 0 0 0 3 3" }
                            path { d: "M6 7v10" }
                        }
                        span { class: "truncate", if context.branch.is_empty() { "Git" } else { "{context.branch}" } }
                    }
                    if context.is_worktree {
                        span {
                            class: "flex h-7 shrink-0 items-center gap-1 rounded-lg bg-violet-500/[0.08] px-2 text-[10px] font-medium text-violet-600 ring-1 ring-inset ring-violet-500/15 dark:text-violet-300",
                            title: "{worktree_title}",
                            "Worktree"
                        }
                    } else if context.can_manage_workspace {
                        button {
                            class: "flex h-7 shrink-0 items-center gap-1 rounded-lg px-2 text-[10px] font-medium text-muted-foreground transition hover:bg-violet-500/[0.08] hover:text-violet-600 dark:hover:text-violet-300",
                            title: "Create or select a worktree for this project",
                            onmousedown: move |event| event.prevent_default(),
                            onclick: move |_| {
                                let _ = try_cef_bin_emit_rkyv(&ChatCreateWorktree);
                                focus_prompt_end(PROMPT_INPUT_ID);
                            },
                            "+ Worktree"
                        }
                    }
                    if context.uncommitted > 0 {
                        span { class: "shrink-0 font-mono text-[10px] text-amber-500", title: "Uncommitted changes", "● {context.uncommitted}" }
                    }
                    if context.ahead > 0 {
                        span { class: "shrink-0 font-mono text-[10px] text-sky-500", title: "Commits ahead of upstream", "↑{context.ahead}" }
                    }
                } else if context.workspace_selected {
                    span { class: "h-7 shrink-0 content-center rounded-lg px-2 text-[10px] text-muted-foreground/70", "No Git" }
                }
            }
            div { class: "flex shrink-0 items-center gap-1 text-[10px] text-muted-foreground",
                span { class: "flex h-7 items-center gap-1.5 rounded-lg px-2",
                    span { class: "h-1.5 w-1.5 rounded-full {status_dot_class(&status())}" }
                    "{run_label}"
                }
                if active_subagents > 0 {
                    span { class: "flex h-7 items-center gap-1 rounded-lg bg-violet-500/[0.07] px-2 text-violet-600 dark:text-violet-300", title: "Active subagents",
                        svg {
                            class: "h-3.5 w-3.5",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.8",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "9", cy: "8", r: "3" }
                            path { d: "M3.5 19a5.5 5.5 0 0 1 11 0" }
                            circle { cx: "17", cy: "9", r: "2.5" }
                            path { d: "M15.5 14.5A4.5 4.5 0 0 1 21 19" }
                        }
                        "{active_subagents}"
                    }
                }
                if active_tasks > 0 {
                    span { class: "flex h-7 items-center gap-1 rounded-lg px-2", title: "Open plan tasks", "{active_tasks} tasks" }
                }
                if queued_count > 0 {
                    span { class: "flex h-7 items-center gap-1 rounded-lg px-2", title: "Queued prompts", "{queued_count} queued" }
                }
            }
        }
    };

    rsx! {
        main {
            class: "agent-chat-page relative isolate flex h-screen flex-col overflow-hidden bg-background text-foreground outline-none",
            style: "--agent-accent:{theme_accent};",
            // Focusable so a click on the transcript lands focus here rather than on the body,
            // which would put keystrokes out of reach of the handler below. Deliberately not
            // autofocused: `focus_prompt_end` already claims focus for the prompt on mount.
            tabindex: "-1",
            onkeydown: root_keydown,
            style { dangerous_inner_html: MD_CSS }
            if installing_splash {
                InstallBackdrop { accent_rgb: rain_accent, title: header_name.to_uppercase() }
            }
            header { class: "agent-chat-header vmux-agent-surface-enter relative z-10 flex min-w-0 items-center gap-2.5 border-b bg-background/95 px-3 py-3 shadow-[0_1px_0_rgba(255,255,255,0.02)] sm:px-5",
                AgentAvatar {
                    icon: agent_icon(),
                    accent: accent(),
                    agent: agent.clone(),
                    name: header_name.clone(),
                    size_class: "h-6 w-6 text-[11px]",
                }
                span { class: "h-2.5 w-2.5 rounded-full {status_dot_class(&status())}" }
                div { class: "min-w-0 flex-1",
                    div { class: "truncate bg-gradient-to-b from-foreground to-foreground/60 bg-clip-text text-sm font-semibold text-transparent", title: "{conversation_title}",
                        "{conversation_title}"
                    }
                    div { class: "truncate text-[10px] text-muted-foreground/60", "{header_name}" }
                }
            }
            div {
                id: "chat-scroll",
                onmounted: move |e| scroll_container.set(Some(e.data())),
                class: "vmux-agent-surface-enter vmux-agent-surface-enter-delayed relative z-10 flex-1 overflow-y-auto overscroll-contain px-3 py-6 sm:px-4 md:px-6",
                onscroll: move |e: Event<ScrollData>| {
                    let top = e.scroll_top() as i32;
                    let dist = e.scroll_height() - top - e.client_height();
                    // Re-pin once the user reaches the bottom; unpin only when they scroll UP
                    // (scroll_top decreases). Never unpin from our own programmatic
                    // scroll-to-bottom, which only moves down and would otherwise poison
                    // `at_bottom` with a stale, mid-stream scroll height.
                    if dist <= 48 {
                        at_bottom.set(true);
                    } else if top < *last_top.peek() - 4 {
                        at_bottom.set(false);
                    }
                    last_top.set(top);
                    if top <= 160 {
                        request_chat_history(loaded_start(), history_loading);
                    }
                },
                div { class: "mx-auto flex min-h-full max-w-none flex-col gap-5 md:max-w-3xl",
                    if loaded_start() > 0 {
                        button {
                            id: "chat-load-older",
                            class: "mx-auto rounded-full border border-foreground/10 bg-background/90 px-3 py-1.5 text-xs text-muted-foreground shadow-sm transition-colors hover:bg-foreground/[0.06] hover:text-foreground disabled:opacity-50",
                            disabled: history_loading(),
                            onclick: move |_| request_chat_history(loaded_start(), history_loading),
                            {if history_loading() { translate("agent-loading-older") } else { translate("agent-load-older") }}
                        }
                    }
                    if installing_splash {
                        div { class: "my-auto flex flex-col items-center gap-3 py-16 text-center",
                            AgentAvatar {
                    icon: agent_icon(),
                    accent: accent(),
                    agent: agent.clone(),
                    name: header_name.clone(),
                    size_class: "h-14 w-14 text-xl",
                }
                            h2 { class: "bg-gradient-to-b from-foreground to-foreground/50 bg-clip-text text-3xl font-semibold capitalize tracking-tight text-transparent",
                                "{header_name}"
                            }
                            div { class: "flex max-w-sm items-center gap-2 rounded-full bg-background/90 px-3 py-1.5 text-xs text-muted-foreground ring-1 ring-inset ring-foreground/10",
                                span { class: "h-1.5 w-1.5 shrink-0 rounded-full {agent_accent.accent_bg}" }
                                span { class: "truncate", "{install_detail}" }
                            }
                        }
                    } else if items.read().is_empty() && status() == "idle" {
                        div { class: "vmux-agent-ready-enter flex flex-col items-center gap-3 py-24 text-center",
                            AgentAvatar {
                    icon: agent_icon(),
                    accent: accent(),
                    agent: agent.clone(),
                    name: header_name.clone(),
                    size_class: "h-14 w-14 text-xl",
                }
                            h2 { class: "bg-gradient-to-b from-foreground to-foreground/50 bg-clip-text text-3xl font-semibold capitalize tracking-tight text-transparent",
                                "{header_name}"
                            }
                            p { class: "text-sm text-muted-foreground", {translate("agent-ready")} }
                        }
                    }
                    for (i, item) in items.read().iter().cloned().enumerate() {
                        ChatItemRow {
                            key: "{loaded_start() as usize + i}",
                            absolute_index: loaded_start() as usize + i,
                            item,
                            attachment_previews,
                            latest_tool_block: latest_tool
                                .filter(|(item_index, _)| *item_index == i)
                                .map(|(_, block_index)| block_index),
                        }
                        if !handoff_source().is_empty()
                            && is_handoff_boundary(
                                loaded_start() as usize + i,
                                handoff_message_count(),
                            )
                        {
                            div { class: "flex items-center gap-2 py-1 text-xs text-muted-foreground",
                                span { class: "h-px flex-1 bg-foreground/10" }
                                span {
                                    {translate_with(
                                        "agent-continued-from",
                                        &[("source", TranslationValue::String(&handoff_source()))],
                                    )}
                                }
                                if handoff_truncated() {
                                    span { class: "text-amber-500/80", {format!("· {}", translate("agent-older-context-omitted"))} }
                                }
                                span { class: "h-px flex-1 bg-foreground/10" }
                            }
                        }
                    }
                    if status() == "errored" {
                        {
                            let message = error();
                            let is_startup = message.to_lowercase().contains("startup");
                            let version_hint = is_version_error(&message);
                            let title = if is_startup {
                                translate("agent-error-startup-title")
                            } else {
                                translate("common-error")
                            };
                            let copy_label = translate("common-copy");
                            let copy_text = message.clone();
                            rsx! {
                                div { class: "flex flex-col gap-2 rounded-xl bg-red-500/[0.07] px-4 py-3 ring-1 ring-inset ring-red-500/20",
                                    div { class: "flex items-center gap-2",
                                        svg {
                                            class: "h-4 w-4 shrink-0 text-red-500",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "1.8",
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            path { d: "M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z" }
                                            path { d: "M12 9v4" }
                                            path { d: "M12 17h.01" }
                                        }
                                        span { class: "text-sm font-semibold text-red-600 dark:text-red-300", "{title}" }
                                        button {
                                            class: "ml-auto flex h-6 w-6 items-center justify-center rounded-md text-red-500/70 transition hover:bg-red-500/10 hover:text-red-500",
                                            title: "{copy_label}",
                                            aria_label: "{copy_label}",
                                            onclick: move |_| copy_to_clipboard(&copy_text),
                                            svg {
                                                class: "h-3.5 w-3.5",
                                                view_box: "0 0 24 24",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "1.8",
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                rect { x: "9", y: "9", width: "13", height: "13", rx: "2" }
                                                path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                                            }
                                        }
                                    }
                                    div { class: "max-h-40 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-red-500/[0.06] px-3 py-2 font-mono text-[11px] leading-relaxed text-red-700/90 dark:text-red-200/80",
                                        "{message}"
                                    }
                                }
                                if version_hint {
                                    div { class: "flex items-start gap-3 rounded-xl bg-foreground/[0.04] px-4 py-3 ring-1 ring-inset ring-foreground/10",
                                        div { class: "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-amber-500/15 text-amber-500",
                                            svg {
                                                class: "h-4 w-4",
                                                view_box: "0 0 24 24",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "1.8",
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                path { d: "M9 18h6" }
                                                path { d: "M10 22h4" }
                                                path { d: "M12 2a7 7 0 0 0-4 12.7c.6.5 1 1.3 1 2.1h6c0-.8.4-1.6 1-2.1A7 7 0 0 0 12 2Z" }
                                            }
                                        }
                                        div { class: "flex min-w-0 flex-1 flex-col gap-2.5",
                                            p { class: "text-sm leading-relaxed text-foreground", {translate("agent-error-version-suggestion")} }
                                            button {
                                                class: "vmux-gradient-outline inline-flex items-center gap-2 self-end rounded-xl px-6 py-3 text-sm font-semibold transition hover:-translate-y-0.5 hover:shadow-lg active:scale-[0.98]",
                                                onclick: move |_| {
                                                    let _ = try_cef_bin_emit_rkyv(&ChatOpenPage { url: "vmux://agents".to_string() });
                                                },
                                                svg {
                                                    class: "h-4 w-4 text-indigo-500",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "1.8",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    path { d: "M15 3h6v6" }
                                                    path { d: "M10 14 21 3" }
                                                    path { d: "M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5" }
                                                }
                                                span { class: "bg-gradient-to-r from-indigo-500 via-purple-500 to-pink-500 bg-clip-text text-transparent",
                                                    {translate("agent-error-open-agents")}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if paused() {
                        div { class: "flex items-center gap-3 py-1 text-xs text-muted-foreground",
                            span { class: "h-px flex-1 bg-foreground/10" }
                            span { class: "shrink-0", {translate("agent-interrupted")} }
                            span { class: "h-px flex-1 bg-foreground/10" }
                        }
                    }
                }
            }

            if !installing && let Some((call_id, name, args_json)) = approval() {
                {
                    let details = super::approval_details(&args_json);
                    rsx! {
                        div { class: "border-t border-foreground/10 bg-foreground/[0.04] px-4 py-3",
                            div { class: "mx-auto flex max-w-3xl flex-col gap-3",
                                div { class: "min-w-0",
                                    div { class: "text-sm text-foreground",
                                        {translate_with(
                                            "agent-allow-tool",
                                            &[("tool", TranslationValue::String(&name))],
                                        )}
                                    }
                                    if !details.is_empty() {
                                        div { class: "mt-2 max-h-40 overflow-auto rounded-lg bg-foreground/[0.05] ring-1 ring-inset ring-foreground/10",
                                            for (i , detail) in details.iter().enumerate() {
                                                div {
                                                    key: "approval-detail-{i}",
                                                    class: "grid grid-cols-[7rem_minmax(0,1fr)] items-start gap-3 border-b border-foreground/10 px-3 py-2 last:border-b-0",
                                                    span { class: "pt-0.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/70", "{approval_detail_label(&detail.label)}" }
                                                    pre { class: "overflow-x-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-muted-foreground", "{detail.value}" }
                                                }
                                            }
                                        }
                                    }
                                }
                                div { class: "flex flex-col gap-1.5",
                                    for (index , label) in [translate("agent-allow"), translate("agent-allow-always"), translate("agent-deny")].into_iter().enumerate() {
                                        button {
                                            key: "approval-option-{index}",
                                            class: if approval_sel() == index { "flex items-center gap-3 rounded-xl bg-foreground px-3 py-2 text-left text-sm text-background" } else { "flex items-center gap-3 rounded-xl bg-foreground/[0.045] px-3 py-2 text-left text-sm text-foreground hover:bg-foreground/[0.08]" },
                                            onclick: {
                                                let call_id = call_id.clone();
                                                move |_| {
                                                    if let Some(decision) = approval_decision_for_index(index)
                                                        && send_approval(call_id.clone(), decision)
                                                    {
                                                        approval.set(None);
                                                        approval_sel.set(0);
                                                    }
                                                }
                                            },
                                            span { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-current/20 font-mono text-[10px]", "{index + 1}" }
                                            span { class: "min-w-0 flex-1", "{label}" }
                                        }
                                    }
                                    div { class: "mt-1 text-[11px] text-muted-foreground", {translate("agent-choice-help").replace("1–9", "1–3")} }
                                }
                            }
                        }
                    }
                }
            }

            div {
                class: "relative z-10 bg-gradient-to-t from-background via-background/95 to-transparent px-4 pb-4 pt-8",
                div {
                    class: "agent-chat-prompt-shell vmux-agent-prompt-dock-enter relative mx-auto flex max-w-3xl flex-col gap-2",
                    if media_menu_open {
                        PromptPopup {
                            PromptMediaOptions {
                                items: prompt_media_options,
                                selected: menu_sel(),
                                loading: media_loading(),
                                loading_label: translate("agent-loading-media"),
                                empty_label: translate("agent-no-matching-media"),
                                on_hover: move |index| menu_sel.set(index),
                                on_select: move |index| {
                                    if let Some(entry) = media_entries.peek().get(index).cloned() {
                                        select_media_entry(&entry, draft, menu_sel);
                                    }
                                },
                            }
                        }
                    }
                    if cmd_menu_open {
                        PromptPopup {
                            for (i , command) in filtered_cmds.iter().enumerate() {
                                {
                                    let command = command.clone();
                                    rsx! {
                                        div {
                                            key: "sc{i}",
                                            id: "agent-selector-item-{i}",
                                            class: if i == menu_sel() { "flex cursor-pointer items-baseline gap-3 px-3.5 py-2 text-sm bg-foreground/10" } else { "flex cursor-pointer items-baseline gap-3 px-3.5 py-2 text-sm" },
                                            onmouseenter: move |_| menu_sel.set(i),
                                            onclick: move |_| run_slash_command(&command.name, draft, menu_sel),
                                            span { class: "font-medium text-foreground", "/{command.name}" }
                                            span { class: "text-xs text-muted-foreground", "{slash_command_description(&command)}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if session_menu_open {
                        PromptPopup {
                            if resume_state == Some(ResumeMenuState::Loading) {
                                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("agent-loading-sessions")} }
                            } else if resume_state == Some(ResumeMenuState::Empty) {
                                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("agent-no-resumable-sessions")} }
                            } else if resume_state == Some(ResumeMenuState::NoMatch) {
                                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("agent-no-matching-sessions")} }
                            } else {
                                for (i , session) in filtered_sessions.iter().enumerate() {
                                    {
                                        let session = session.clone();
                                        rsx! {
                                            div {
                                                key: "rs{i}",
                                                id: "agent-selector-item-{i}",
                                                class: if i == menu_sel() { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2 bg-foreground/10" } else { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2" },
                                                onmouseenter: move |_| menu_sel.set(i),
                                                onclick: move |_| select_resume_session(&session, draft),
                                                div { class: "flex min-w-0 items-baseline gap-2",
                                                    span { class: "min-w-0 flex-1 truncate text-sm text-foreground", "{session.title}" }
                                                    if !session.agent_name.is_empty() {
                                                        span { class: "max-w-[40%] shrink-0 truncate text-xs text-muted-foreground", "{session.agent_name}" }
                                                    }
                                                }
                                                span { class: "truncate text-xs text-muted-foreground", "{session_age_label(session.age_seconds)} · {session.subtitle}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if model_menu_open {
                        PromptPopup {
                            if filtered_models.is_empty() {
                                div { class: "px-3.5 py-2 text-sm text-muted-foreground", {translate("agent-no-matching-models")} }
                            } else {
                                for (i , model) in filtered_models.iter().enumerate() {
                                    {
                                        let model = model.clone();
                                        let selected = model.id == current_model_id();
                                        rsx! {
                                            div {
                                                key: "model{i}",
                                                id: "agent-selector-item-{i}",
                                                class: if i == menu_sel() { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2 bg-foreground/10" } else { "flex cursor-pointer flex-col gap-0.5 px-3.5 py-2" },
                                                onmouseenter: move |_| menu_sel.set(i),
                                                onclick: move |_| select_model(&model, draft),
                                                div { class: "flex min-w-0 items-baseline gap-2",
                                                    span { class: "min-w-0 flex-1 truncate text-sm text-foreground", "{model.name}" }
                                                    if selected {
                                                        span { class: "shrink-0 text-[10px] uppercase tracking-wide text-emerald-500", {translate("common-current")} }
                                                    }
                                                }
                                                if !model.description.is_empty() {
                                                    span { class: "truncate text-xs text-muted-foreground", "{model.description}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !choice_options.read().is_empty() {
                        div { class: "rounded-2xl border border-foreground/10 bg-foreground/[0.045] p-3.5 shadow-sm",
                            div { class: "mb-3 text-sm font-medium text-foreground", "{choice_question}" }
                            div { class: "flex flex-col gap-1.5",
                                for (index, option) in choice_options.read().iter().cloned().enumerate() {
                                    button {
                                        key: "choice-{index}",
                                        id: "agent-choice-item-{index}",
                                        onmouseenter: move |_| menu_sel.set(index),
                                        class: if index == menu_sel() { "flex items-center gap-3 rounded-xl bg-foreground px-3 py-2 text-left text-sm text-background" } else { "flex items-center gap-3 rounded-xl bg-foreground/[0.045] px-3 py-2 text-left text-sm text-foreground hover:bg-foreground/[0.08]" },
                                        onclick: move |_| {
                                            if try_cef_bin_emit_rkyv(&ChatChoiceSelected { index: index as u32 }).is_ok() {
                                                choice_question.set(String::new());
                                                choice_options.set(Vec::new());
                                                menu_sel.set(0);
                                            }
                                        },
                                        span { class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-md border border-current/20 font-mono text-[10px]", "{index + 1}" }
                                        span { class: "min-w-0 flex-1", "{option}" }
                                    }
                                }
                            }
                            div { class: "mt-2.5 text-[11px] text-muted-foreground", {translate("agent-choice-help")} }
                        }
                    }
                    if transition_preview.read().is_empty() && !queued.read().is_empty() {
                        div { class: "flex flex-col items-end gap-1.5",
                            for queued_prompt in queued.read().iter().cloned() {
                                div {
                                    key: "q{queued_prompt.id}",
                                    class: "group flex max-w-[80%] items-center gap-2 rounded-2xl border border-dashed border-foreground/20 bg-foreground/[0.03] py-2 pl-3.5 pr-2 text-sm text-muted-foreground",
                                    span { class: "shrink-0 text-[10px] uppercase tracking-wide text-foreground/40", {translate("agent-queued")} }
                                    span { class: "min-w-0 flex-1 whitespace-pre-wrap break-words",
                                        if !queued_prompt.text.is_empty() {
                                            "{queued_prompt.text}"
                                        }
                                        if !queued_prompt.attachment_names.is_empty() {
                                            span { class: "block text-xs text-foreground/45",
                                                {format!("{} ", translate("agent-attached"))}
                                                for (i , name) in queued_prompt.attachment_names.iter().enumerate() {
                                                    if i > 0 { ", " }
                                                    "{name}"
                                                }
                                            }
                                        }
                                    }
                                    button {
                                        class: "flex shrink-0 items-center rounded-lg p-1 text-foreground/35 opacity-70 transition hover:bg-foreground/10 hover:text-foreground hover:opacity-100 focus:opacity-100",
                                        title: translate("agent-cancel-queued"),
                                        onclick: move |_| {
                                            let _ = try_cef_bin_emit_rkyv(&ChatCancelQueuedPrompt {
                                                id: queued_prompt.id,
                                            });
                                        },
                                        svg {
                                            class: "h-3.5 w-3.5",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            stroke_linecap: "round",
                                            path { d: "M6 6l12 12M18 6L6 18" }
                                        }
                                    }
                                }
                            }
                            if paused() {
                                div { class: "flex items-center gap-1",
                                    button {
                                        class: "flex items-center gap-1 rounded-lg px-2 py-1 text-xs text-muted-foreground transition hover:bg-foreground/10 hover:text-foreground",
                                        title: translate("agent-resume-queued"),
                                        onclick: move |_| {
                                            let _ = try_cef_bin_emit_rkyv(&ChatResume);
                                        },
                                        svg {
                                            class: "h-3.5 w-3.5",
                                            view_box: "0 0 24 24",
                                            fill: "currentColor",
                                            path { d: "M8 5v14l11-7z" }
                                        }
                                        span { class: "tabular-nums", "{queued.read().len()}" }
                                    }
                                    button {
                                        class: "flex items-center rounded-lg p-1 text-muted-foreground transition hover:bg-foreground/10 hover:text-foreground",
                                        title: translate("agent-clear-queue"),
                                        onclick: move |_| {
                                            let _ = try_cef_bin_emit_rkyv(&ChatClearQueue);
                                        },
                                        svg {
                                            class: "h-3.5 w-3.5",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            stroke_linecap: "round",
                                            path { d: "M6 6l12 12M18 6L6 18" }
                                        }
                                    }
                                }
                            }
                            div { class: "flex items-center gap-2 pr-1 text-[10px] text-foreground/40",
                                kbd { class: "inline-flex h-5 items-center rounded border border-foreground/15 bg-foreground/[0.06] px-1.5 font-mono text-[10px] font-medium text-foreground/60 shadow-sm", "Esc" }
                                span { {translate("agent-send-all-now")} }
                            }
                        }
                    }
                    PromptComposer {
                        value: draft_val.clone(),
                        preview: transition_preview(),
                        attachments: prompt_attachments,
                        show_examples: show_capability_examples,
                        placeholder: if choice_pending { translate("agent-choose-option") } else { translate("command-composer-placeholder") },
                        accent_bg: agent_accent.accent_bg.to_string(),
                        accent_color: theme_accent.clone(),
                        accent_gradient: agent_accent.grad.to_string(),
                        footer: Some(composer_footer),
                        action: prompt_action,
                        action_title: prompt_action_title,
                        action_enabled: prompt_action_enabled,
                        on_input: move |value| {
                            draft.set(value);
                            history_cursor.set(None);
                            history_scratch.set(String::new());
                            menu_sel.set(0);
                        },
                        on_keydown: prompt_keydown,
                        on_paste: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&ChatPasteMedia);
                        },
                        on_attach: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&ChatPickFiles);
                        },
                        on_remove_attachment: move |index| {
                            let mut next = attachments.peek().clone();
                            if index < next.len() {
                                next.remove(index);
                                attachments.set(next);
                            }
                        },
                        on_action: move |_| {
                            if prompt_streaming {
                                if queued.peek().is_empty() {
                                    let _ = try_cef_bin_emit_rkyv(&ChatCancel);
                                } else {
                                    let _ = try_cef_bin_emit_rkyv(&ChatEscape);
                                }
                            } else {
                                do_submit(
                                    draft,
                                    attachments,
                                    history_cursor,
                                    history_scratch,
                                    at_bottom,
                                );
                            }
                        },
                    }
                }
            }
        }
    }
}

/// Run a selected vmux slash command. `resume` opens the session picker; `cli`/`acp` hand the
/// current session to the other runtime. Unknown names are ignored (the raw text still submits
/// via the normal Enter path).
fn run_slash_command(name: &str, mut draft: Signal<String>, mut menu_sel: Signal<usize>) {
    match name {
        "upload" => {
            let _ = try_cef_bin_emit_rkyv(&ChatPickFiles);
            draft.set(String::new());
        }
        "resume" => {
            menu_sel.set(0);
            draft.set("/resume ".to_string());
        }
        "model" => {
            menu_sel.set(0);
            draft.set("/model ".to_string());
        }
        "cli" => {
            let _ = try_cef_bin_emit_rkyv(&RuntimeSwitchRequest { to: "cli".into() });
            draft.set(String::new());
        }
        "acp" => {
            let _ = try_cef_bin_emit_rkyv(&RuntimeSwitchRequest { to: "acp".into() });
            draft.set(String::new());
        }
        _ => {}
    }
}

fn select_model(model: &ModelOptionEntry, mut draft: Signal<String>) {
    let _ = try_cef_bin_emit_rkyv(&SelectModel {
        model_id: model.id.clone(),
    });
    draft.set(String::new());
}

fn select_resume_session(session: &ResumableSessionEntry, mut draft: Signal<String>) {
    let _ = try_cef_bin_emit_rkyv(&ResumeSession {
        kind: session.kind.clone(),
        sid: session.sid.clone(),
        cwd: session.cwd.clone(),
    });
    draft.set(String::new());
}

/// Emit the draft as a submit intent, clearing the input only if the IPC succeeded so a failed
/// emit never silently swallows the user's message. The queued/sent turn arrives via snapshot.
fn do_submit(
    mut draft: Signal<String>,
    mut attachments: Signal<Vec<ChatAttachment>>,
    mut history_cursor: Signal<Option<usize>>,
    mut history_scratch: Signal<String>,
    mut at_bottom: Signal<bool>,
) {
    let text = draft.peek().trim().to_string();
    let selected = attachments.peek().clone();
    if text.is_empty() && selected.is_empty() {
        return;
    }
    let attachments_to_submit = selected
        .iter()
        .map(|attachment| ChatSubmitAttachment {
            path: attachment.path.clone(),
            name: attachment.name.clone(),
            mime_type: attachment.mime_type.clone(),
            size: attachment.size,
        })
        .collect();
    if try_cef_bin_emit_rkyv(&ChatSubmit {
        text,
        attachments: attachments_to_submit,
    })
    .is_err()
    {
        return;
    }
    at_bottom.set(true);
    draft.set(String::new());
    attachments.set(Vec::new());
    history_cursor.set(None);
    history_scratch.set(String::new());
}

fn send_approval(call_id: String, decision: u8) -> bool {
    try_cef_bin_emit_rkyv(&ChatApproval { call_id, decision }).is_ok()
}

fn current_activity_icon(items: &[ChatItem], status: &str) -> Option<ActivityIcon> {
    match status {
        "installing" => Some(ActivityIcon::Installing),
        "awaiting" => Some(ActivityIcon::Awaiting),
        "errored" => Some(ActivityIcon::Error),
        "streaming" => {
            let block = items.iter().rev().find_map(|item| match item {
                ChatItem::Turn(turn) if turn.running => turn.blocks.last(),
                _ => None,
            });
            Some(match block {
                Some(ChatBlock::Text(_)) => ActivityIcon::Writing,
                Some(ChatBlock::Thinking(_)) | None => ActivityIcon::Thinking,
                Some(ChatBlock::ToolUse { name, args, .. }) => tool_activity_icon_for(name, args),
                Some(ChatBlock::Subagent(_)) => ActivityIcon::Subagent,
                Some(ChatBlock::Diff { path, .. }) => {
                    language_activity_icon(path).unwrap_or(ActivityIcon::Diff)
                }
                Some(ChatBlock::Plan { .. }) => ActivityIcon::Plan,
                Some(ChatBlock::ToolResult { is_error: true, .. }) => ActivityIcon::Error,
                Some(ChatBlock::ToolResult { .. }) => ActivityIcon::Output,
                Some(ChatBlock::Reconnect { .. }) => ActivityIcon::Reconnect,
            })
        }
        _ => None,
    }
}

fn svg_data_url(svg: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(svg.len() * 2);
    encoded.push_str("data:image/svg+xml,");
    for byte in svg.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

fn activity_favicon(kind: ActivityIcon, accent: &str) -> String {
    if kind == ActivityIcon::Python {
        return svg_data_url(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect x='1' y='1' width='30' height='30' rx='8' fill='#151515' stroke='#3776ab' stroke-opacity='.7'/><path fill='#3776ab' d='M15.6 4C9.3 4 9.7 6.7 9.7 6.7v2.8h6v1.2H7.3s-4.6-.5-4.6 6.9 4.1 7.1 4.1 7.1h2.4v-3.3s-.1-4 3.9-4h6.3s3.6 0 3.6-3.6V7.7S23.4 4 15.6 4Zm-3.3 2a1.1 1.1 0 1 1 0 2.2 1.1 1.1 0 0 1 0-2.2Z'/><path fill='#ffd43b' d='M16.4 28c6.3 0 5.9-2.7 5.9-2.7v-2.8h-6v-1.2h8.4s4.6.5 4.6-6.9-4.1-7.1-4.1-7.1h-2.4v3.3s.1 4-3.9 4h-6.3S9 14.6 9 18.2v6.1S8.6 28 16.4 28Zm3.3-2a1.1 1.1 0 1 1 0-2.2 1.1 1.1 0 0 1 0 2.2Z'/></svg>",
        );
    }
    let mut paths = String::new();
    for path in activity_icon_paths(kind) {
        paths.push_str("<path d='");
        paths.push_str(path);
        paths.push_str("'/>");
    }
    svg_data_url(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect x='1' y='1' width='30' height='30' rx='8' fill='{accent}' fill-opacity='.15' stroke='{accent}' stroke-opacity='.45'/><g transform='translate(4 4)' fill='none' stroke='{accent}' stroke-width='1.9' stroke-linecap='round' stroke-linejoin='round'>{paths}</g></svg>"
    ))
}

/// Reflect the conversation in the tab that holds the page.
///
/// A pane is a browser tab, so its title and favicon are how the conversation identifies itself in
/// the layout. A native host has no tab and shows this in its own chrome instead.
#[cfg(web)]
fn set_tab_identity(title: &str, favicon_href: &str) {
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        if document.title() != title {
            document.set_title(title);
        }
        set_page_favicon(favicon_href);
    }
}

#[cfg(not(web))]
fn set_tab_identity(_title: &str, _favicon_href: &str) {}

#[cfg(web)]
fn set_page_favicon(href: &str) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let link = document
        .query_selector("link[rel~='icon']")
        .ok()
        .flatten()
        .or_else(|| {
            let link = document.create_element("link").ok()?;
            link.set_attribute("rel", "icon").ok()?;
            document
                .query_selector("head")
                .ok()
                .flatten()?
                .append_child(&link)
                .ok()?;
            Some(link)
        });
    if let Some(link) = link {
        let _ = link.set_attribute("href", href);
    }
}

fn status_dot_class(status: &str) -> &'static str {
    match status {
        "streaming" => "bg-amber-400 shadow-[0_0_8px_rgba(251,191,36,0.65)]",
        "installing" => "bg-sky-400 shadow-[0_0_8px_rgba(56,189,248,0.65)]",
        "awaiting" => "bg-violet-400 shadow-[0_0_8px_rgba(167,139,250,0.65)]",
        "errored" => "bg-red-500 shadow-[0_0_8px_rgba(239,68,68,0.65)]",
        _ => "bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.65)]",
    }
}

/// The agent avatar: its favicon if resolvable, else an accent-filled circle with the initial.
/// The agent's face: its favicon when it has one, else an initial on its accent.
#[component]
fn AgentAvatar(
    icon: String,
    accent: String,
    agent: String,
    name: String,
    size_class: String,
) -> Element {
    let (icon, accent, agent, name, size_class) = (
        icon.as_str(),
        accent.as_str(),
        agent.as_str(),
        name.as_str(),
        size_class.as_str(),
    );
    let url = format!("vmux://agent/{agent}");
    let src = favicon_src_for_url(icon, &url);
    let initial: String = name
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_default();
    let fallback = if accent.is_empty() { "#6366f1" } else { accent };
    let style = if src.is_some() {
        String::new()
    } else {
        format!("background:{fallback}")
    };
    rsx! {
        div {
            class: "flex shrink-0 items-center justify-center overflow-hidden rounded-full font-semibold text-white {size_class}",
            style: "{style}",
            if let Some(src) = src.as_ref() {
                img { class: "h-full w-full object-cover", src: "{src}" }
            } else {
                "{initial}"
            }
        }
    }
}
