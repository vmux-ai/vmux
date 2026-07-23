#![allow(non_snake_case)]

use crate::chat_page::composer::{
    PromptEdit, PromptHistoryDirection, ResumeMenuState, SelectorMode, ToolActivity,
    approval_decision_for_index, chat_page_title, choice_number_index, edit_prompt, filter_models,
    filter_sessions, is_handoff_boundary, menu_direction, move_prompt_history, move_selection,
    prompt_history_direction, resume_menu_state, selector_mode, should_clear_draft_on_escape,
    should_expand_thinking, should_fetch_resume, tool_activity,
};
use crate::chat_page::event::{
    CHAT_ATTACHMENT_PREVIEWS_EVENT, CHAT_ATTACHMENTS_EVENT, CHAT_HISTORY_PAGE_EVENT,
    CHAT_HISTORY_PAGE_SIZE, CHAT_MEDIA_ENTRIES_EVENT, CHAT_SNAPSHOT_EVENT, COMPOSER_CONTEXT_EVENT,
    ChatApproval, ChatAttachPaths, ChatAttachment, ChatAttachmentPreviewRequest, ChatAttachments,
    ChatBlock, ChatCancel, ChatCancelQueuedPrompt, ChatChoiceSelected, ChatClearQueue,
    ChatCreateWorktree, ChatEscape, ChatHistoryPage, ChatHistoryRequest, ChatItem,
    ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest, ChatPasteMedia, ChatPickFiles,
    ChatResume, ChatSelectWorkspace, ChatSnapshot, ChatSubmit, ChatSubmitAttachment, ChatTurn,
    ComposerContext, MODEL_STATE_EVENT, ModelOptionEntry, ModelState, QueuedPromptSnapshot,
    RESUMABLE_SESSIONS_EVENT, ResumableSessionEntry, ResumableSessions, ResumeListRequest,
    ResumeSession, RuntimeSwitchRequest, SLASH_COMMANDS_EVENT, SelectModel, SlashCommandEntry,
    SlashCommands, WORKING_VERB_IDS, latest_tool_location,
};
use dioxus::prelude::*;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use vmux_command::prompt_media::{
    inline_media_query, media_display_path, media_reference, merge_chat_attachments,
    replace_inline_media_query,
};
use vmux_terminal::matrix_rain::MatrixRain;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::prompt_box::PromptPopup;
use vmux_ui::components::prompt_composer::{
    PROMPT_INPUT_ID, PromptComposer, PromptComposerAction, PromptComposerAttachment,
    focus_prompt_end, prompt_textarea,
};

use vmux_ui::components::prompt_media_options::{PromptMediaOption, PromptMediaOptions};
use vmux_ui::favicon::favicon_src_for_url;
use vmux_ui::file_icon::{FileIcon, file_icon_kind, type_icon};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

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
fn has_text_selection() -> bool {
    web_sys::window()
        .and_then(|w| w.get_selection().ok().flatten())
        .map(|s| !s.is_collapsed())
        .unwrap_or(false)
}

/// The agent id from the page URL (`vmux://agent/<id>` → `<id>`); the chat UI is shared
/// across agents and only the id differs.
fn current_agent() -> String {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .and_then(|path| path.split('/').find(|s| !s.is_empty()).map(str::to_string))
        .unwrap_or_else(|| "agent".to_string())
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

fn chat_scroll_element() -> Option<web_sys::Element> {
    web_sys::window()?
        .document()?
        .get_element_by_id("chat-scroll")
}

thread_local! {
    static SCROLL_TO_BOTTOM_PENDING: Cell<bool> = const { Cell::new(false) };
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

fn schedule_scroll_to_bottom() {
    if SCROLL_TO_BOTTOM_PENDING.replace(true) {
        return;
    }
    let callback = Closure::once_into_js(move || {
        SCROLL_TO_BOTTOM_PENDING.set(false);
        if let Some(element) = chat_scroll_element() {
            element.set_scroll_top(element.scroll_height());
        }
    })
    .unchecked_into::<js_sys::Function>();
    if let Some(window) = web_sys::window()
        && window.request_animation_frame(&callback).is_ok()
    {
        return;
    }
    let _ = callback.call0(&JsValue::NULL);
}

fn schedule_scroll_restore(previous_height: i32, previous_top: i32) {
    let callback = Closure::once_into_js(move || {
        if let Some(element) = chat_scroll_element() {
            let added_height = element.scroll_height().saturating_sub(previous_height);
            element.set_scroll_top(previous_top.saturating_add(added_height));
        }
    })
    .unchecked_into::<js_sys::Function>();
    if let Some(window) = web_sys::window()
        && window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&callback, 0)
            .is_ok()
    {
        return;
    }
    let _ = callback.call0(&JsValue::NULL);
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

fn dispatch_input_event(textarea: &web_sys::HtmlTextAreaElement) {
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    if let Ok(event) = web_sys::Event::new_with_event_init_dict("input", &init) {
        let _ = textarea.dispatch_event(&event);
    }
}

fn dispatch_keyboard_event(
    textarea: &web_sys::HtmlTextAreaElement,
    source: &web_sys::KeyboardEvent,
) {
    let init = web_sys::KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_key(&source.key());
    init.set_code(&source.code());
    init.set_ctrl_key(source.ctrl_key());
    init.set_shift_key(source.shift_key());
    init.set_alt_key(source.alt_key());
    init.set_meta_key(source.meta_key());
    if let Ok(event) = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init) {
        let _ = textarea.dispatch_event(&event);
    }
}

fn install_global_prompt_input(
    draft: Signal<String>,
    slash_cmds: Signal<Vec<SlashCommandEntry>>,
    choice_options: Signal<Vec<String>>,
    mut approval: Signal<Option<(String, String, String)>>,
    mut approval_sel: Signal<usize>,
) {
    let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
        let Some(textarea) = prompt_textarea(PROMPT_INPUT_ID) else {
            return;
        };
        let prompt_focused = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element())
            .is_some_and(|element| element.id() == PROMPT_INPUT_ID);
        if prompt_focused {
            return;
        }

        let selector_open = {
            let draft_value = draft.peek();
            inline_media_query(&draft_value).is_some()
                || match selector_mode(&draft_value) {
                    SelectorMode::Resume(_) => true,
                    SelectorMode::Models(_) => true,
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
        let key = event.key();
        let active_approval = approval.peek().clone();
        let approval_open = active_approval.is_some();
        let choice_len = choice_options.peek().len();
        let choice_open = choice_len > 0;
        let direction = if event.meta_key() || event.alt_key() {
            None
        } else {
            menu_direction(&key, event.ctrl_key())
        };
        let plain_invoke_or_close = !event.meta_key()
            && !event.ctrl_key()
            && !event.alt_key()
            && matches!(key.as_str(), "Enter" | "Escape");
        let selector_key = direction.is_some() || plain_invoke_or_close;
        let choice_key = direction.is_some()
            || (!event.meta_key()
                && !event.ctrl_key()
                && !event.alt_key()
                && (key == "Enter" || choice_number_index(&key, choice_len).is_some()));
        let approval_key = direction.is_some()
            || (!event.meta_key()
                && !event.ctrl_key()
                && !event.alt_key()
                && (key == "Enter" || choice_number_index(&key, APPROVAL_OPTION_COUNT).is_some()));
        if approval_open && approval_key {
            event.prevent_default();
            event.stop_propagation();
            if let Some(direction) = direction {
                approval_sel.set(move_selection(
                    approval_sel(),
                    APPROVAL_OPTION_COUNT,
                    direction,
                ));
            } else if let Some((call_id, _, _)) = active_approval {
                let index =
                    choice_number_index(&key, APPROVAL_OPTION_COUNT).unwrap_or(approval_sel());
                if let Some(decision) = approval_decision_for_index(index)
                    && send_approval(call_id, decision)
                {
                    approval.set(None);
                    approval_sel.set(0);
                }
            }
            return;
        }
        if (choice_open && choice_key) || direction.is_some() || (selector_open && selector_key) {
            event.prevent_default();
            event.stop_propagation();
            let _ = textarea.focus();
            dispatch_keyboard_event(&textarea, &event);
            return;
        }

        if event.meta_key() || event.ctrl_key() || event.alt_key() {
            return;
        }
        let edit = match key.as_str() {
            "Backspace" => PromptEdit::Backspace,
            "Delete" => PromptEdit::Delete,
            _ if key.chars().count() == 1 => PromptEdit::Insert(&key),
            _ => return,
        };
        event.prevent_default();
        event.stop_propagation();
        let start = textarea
            .selection_start()
            .ok()
            .flatten()
            .unwrap_or_else(|| textarea.value().encode_utf16().count() as u32);
        let end = textarea.selection_end().ok().flatten().unwrap_or(start);
        let (value, caret) = edit_prompt(&textarea.value(), start, end, edit);
        let _ = textarea.focus();
        textarea.set_value(&value);
        let _ = textarea.set_selection_range(caret, caret);
        dispatch_input_event(&textarea);
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);

    if let Some(window) = web_sys::window() {
        let _ =
            window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
    }
    closure.forget();
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
    let mut composer_context = use_signal(ComposerContext::default);
    let mut menu_sel = use_signal(|| 0usize);
    let mut resume_requested = use_signal(|| false);
    let mut resume_loading = use_signal(|| false);
    let activity_counts = use_memo(move || composer_activity_counts(&items.read()));
    let latest_tool = use_memo(move || latest_tool_location(&items.read()));

    use_effect(move || {
        install_global_prompt_input(draft, slash_cmds, choice_options, approval, approval_sel)
    });
    use_effect(move || focus_prompt_end(PROMPT_INPUT_ID));

    use_effect(move || {
        // Subscribe to any transcript/status change (each snapshot is a fresh `set`). Only pin to
        // the bottom when the user is already there — if they scrolled up to read, leave them.
        let _ = items.read().len();
        let _ = status.read();
        if !*at_bottom.peek() {
            return;
        }
        schedule_scroll_to_bottom();
    });

    let _listener = use_bin_event_listener::<ChatSnapshot, _>(CHAT_SNAPSHOT_EVENT, move |snap| {
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
    let _history =
        use_bin_event_listener::<ChatHistoryPage, _>(CHAT_HISTORY_PAGE_EVENT, move |page| {
            history_loading.set(false);
            if page.end != loaded_start() {
                return;
            }
            let Ok(older) = serde_json::from_str::<Vec<ChatItem>>(&page.items_json) else {
                return;
            };
            request_attachment_previews(&older, attachment_previews, attachment_preview_requests);
            let metrics = chat_scroll_element()
                .map(|element| (element.scroll_height(), element.scroll_top()));
            drop(items.write().splice(0..0, older));
            loaded_start.set(page.start);
            messages_total.set(page.total);
            if let Some((height, top)) = metrics {
                schedule_scroll_restore(height, top);
            }
        });
    let _attachments =
        use_bin_event_listener::<ChatAttachments, _>(CHAT_ATTACHMENTS_EVENT, move |selected| {
            let current = attachments.peek().clone();
            attachments.set(merge_chat_attachments(&current, &selected.attachments));
            focus_prompt_end(PROMPT_INPUT_ID);
        });
    let _attachment_previews = use_bin_event_listener::<ChatAttachments, _>(
        CHAT_ATTACHMENT_PREVIEWS_EVENT,
        move |loaded| {
            let mut previews = attachment_previews.peek().clone();
            for attachment in &loaded.attachments {
                previews.insert(attachment.path.clone(), attachment.clone());
            }
            attachment_previews.set(previews);
        },
    );
    let _media_entries =
        use_bin_event_listener::<ChatMediaEntries, _>(CHAT_MEDIA_ENTRIES_EVENT, move |response| {
            if response.request_id != media_request_id() {
                return;
            }
            media_entries.set(response.entries.clone());
            media_loading.set(false);
            menu_sel.set(0);
        });

    let _cmds = use_bin_event_listener::<SlashCommands, _>(SLASH_COMMANDS_EVENT, move |s| {
        slash_cmds.set(s.commands.clone());
    });
    let _models = use_bin_event_listener::<ModelState, _>(MODEL_STATE_EVENT, move |state| {
        models.set(state.models.clone());
        current_model_id.set(state.current_model_id.clone());
        current_model.set(state.current_model_name.clone());
        menu_sel.set(0);
    });
    let _composer_context =
        use_bin_event_listener::<ComposerContext, _>(COMPOSER_CONTEXT_EVENT, move |context| {
            composer_context.set(context.clone())
        });
    let _sess =
        use_bin_event_listener::<ResumableSessions, _>(RESUMABLE_SESSIONS_EVENT, move |s| {
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
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            if document.title() != title {
                document.set_title(&title);
            }
            let fallback = agent_accent(&favicon_agent).rain_rgb;
            let accent = normalized_accent(&accent(), fallback);
            let href = current_activity_icon(&items, &status)
                .map(|activity| activity_favicon(activity, &accent))
                .or_else(|| {
                    favicon_src_for_url(&agent_icon(), &format!("vmux://agent/{favicon_agent}"))
                })
                .unwrap_or_else(|| activity_favicon(ActivityIcon::Tool, &accent));
            set_page_favicon(&href);
        }
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
            && let Some(textarea) = prompt_textarea(PROMPT_INPUT_ID)
        {
            let start = textarea
                .selection_start()
                .ok()
                .flatten()
                .unwrap_or_default();
            let end = textarea.selection_end().ok().flatten().unwrap_or(start);
            if let Some(direction) =
                prompt_history_direction(&key, e.modifiers().ctrl(), &draft_now, start, end)
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

    use_effect(move || {
        let selected = menu_sel();
        let media_open = {
            let draft = draft.read();
            inline_media_query(&draft).is_some()
        };
        let _ = sessions.read().len();
        let _ = models.read().len();
        let _ = media_entries.read().len();
        let choice_open = !choice_options.read().is_empty();
        let item_id = if choice_open {
            format!("agent-choice-item-{selected}")
        } else if media_open {
            format!("prompt-media-item-{selected}")
        } else {
            format!("agent-selector-item-{selected}")
        };
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&item_id))
        {
            let options = web_sys::ScrollIntoViewOptions::new();
            options.set_block(web_sys::ScrollLogicalPosition::Nearest);
            element.scroll_into_view_with_scroll_into_view_options(&options);
        }
    });

    let context = composer_context();
    let model_name = current_model();
    let (active_subagents, active_tasks) = activity_counts();
    let queued_count = queued.read().len();
    let workspace_label = if context.workspace_selected && !context.workspace_name.is_empty() {
        context.workspace_name.clone()
    } else {
        "Select workspace".to_string()
    };
    let access_label = if context.auto_allow_count == 0 {
        "Ask".to_string()
    } else {
        format!("Ask · {} allowed", context.auto_allow_count)
    };
    let workspace_title = if context.cwd.is_empty() {
        "Create or select workspace".to_string()
    } else {
        format!("Create or select workspace · {}", context.cwd)
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
                span {
                    class: "flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground",
                    title: "Tools ask before protected actions; Allow always is remembered per agent, repository, and tool",
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
                            title: "Create or select a worktree for this workspace",
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
            class: "agent-chat-page relative isolate flex h-screen flex-col overflow-hidden bg-background text-foreground",
            style: "--agent-accent:{theme_accent};",
            style { dangerous_inner_html: MD_CSS }
            if installing_splash {
                div { class: "pointer-events-none absolute inset-0 z-0 overflow-hidden bg-background opacity-75",
                    MatrixRain {
                        accent_rgb: rain_accent,
                        words: vec![header_name.to_uppercase()],
                    }
                }
            }
            header { class: "agent-chat-header vmux-agent-surface-enter relative z-10 flex min-w-0 items-center gap-2.5 border-b bg-background/95 px-5 py-3 shadow-[0_1px_0_rgba(255,255,255,0.02)]",
                {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-6 w-6 text-[11px]")}
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
                class: "vmux-agent-surface-enter vmux-agent-surface-enter-delayed relative z-10 flex-1 overflow-y-auto overscroll-contain px-4 py-6",
                onscroll: move |_| {
                    if let Some(el) = chat_scroll_element() {
                        let top = el.scroll_top();
                        let dist = el.scroll_height() - top - el.client_height();
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
                    }
                },
                div { class: "mx-auto flex min-h-full max-w-3xl flex-col gap-5",
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
                            {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-14 w-14 text-xl")}
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
                            {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-14 w-14 text-xl")}
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
                        div { class: "rounded-xl bg-red-500/10 px-4 py-3 text-sm text-red-600 ring-1 ring-inset ring-red-500/20 dark:text-red-300",
                            "{error}"
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

#[component]
fn ChatItemRow(
    absolute_index: usize,
    item: ChatItem,
    attachment_previews: Signal<HashMap<String, ChatAttachment>>,
    latest_tool_block: Option<usize>,
) -> Element {
    render_item(
        absolute_index,
        &item,
        attachment_previews,
        latest_tool_block,
    )
}

fn render_item(
    key: usize,
    item: &ChatItem,
    attachment_previews: Signal<HashMap<String, ChatAttachment>>,
    latest_tool_block: Option<usize>,
) -> Element {
    match item {
        ChatItem::User {
            text,
            context,
            attachments,
        } => rsx! {
            div {
                key: "{key}",
                class: "chat-user-bubble flex max-w-[80%] self-end flex-col gap-2 rounded-[1.35rem] rounded-tr-md border p-2.5 text-sm",
                style: "content-visibility:auto;contain-intrinsic-size:auto 96px;",
                if let Some(context) = context {
                    details { class: "disclosure user-context-panel rounded-xl border",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 px-2.5 py-2 text-xs list-none [&::-webkit-details-marker]:hidden",
                            span { class: "agent-themed-activity flex h-5 w-5 shrink-0 items-center justify-center rounded-md",
                                svg {
                                    class: "h-3 w-3",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "1.8",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M20 13c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V5l8-3 8 3v8Z" }
                                }
                            }
                            span { class: "font-medium", {translate("agent-prompt-context")} }
                            span {
                                class: "text-[10px] text-muted-foreground",
                                {translate_with(
                                    "agent-bytes",
                                    &[("count", TranslationValue::Number(context.len() as i64))],
                                )}
                            }
                            {render_disclosure_icon()}
                        }
                        pre { class: "user-context-content max-h-72 overflow-auto whitespace-pre-wrap rounded-lg px-3 py-2.5 font-mono text-[11px] leading-relaxed text-muted-foreground", "{context}" }
                    }
                }
                if !text.is_empty() {
                    div { class: "whitespace-pre-wrap px-1.5", "{text}" }
                }
                if !attachments.is_empty() {
                    div { class: "flex flex-wrap justify-end gap-2",
                        for attachment in attachments {
                            {render_user_attachment(attachment, attachment_previews)}
                        }
                    }
                }
            }
        },
        ChatItem::Turn(turn) => render_turn(key, turn, latest_tool_block),
    }
}

fn render_user_attachment(
    attachment: &ChatSubmitAttachment,
    previews: Signal<HashMap<String, ChatAttachment>>,
) -> Element {
    let preview_data_url = previews
        .peek()
        .get(&attachment.path)
        .map(|preview| preview.preview_data_url.clone())
        .unwrap_or_default();
    if attachment.mime_type.starts_with("image/") && !preview_data_url.is_empty() {
        return rsx! {
            figure {
                key: "message-attachment-{attachment.path}",
                class: "max-w-full overflow-hidden rounded-xl bg-black/10 ring-1 ring-inset ring-foreground/10",
                img {
                    src: "{preview_data_url}",
                    alt: "{attachment.name}",
                    loading: "lazy",
                    decoding: "async",
                    class: "max-h-80 max-w-full object-contain",
                }
                figcaption { class: "max-w-72 truncate px-2.5 py-1.5 text-[10px] text-muted-foreground", "{attachment.name}" }
            }
        };
    }
    rsx! {
        div {
            key: "message-attachment-{attachment.path}",
            class: "flex min-w-32 max-w-64 items-center gap-2 rounded-xl bg-foreground/[0.06] px-3 py-2 ring-1 ring-inset ring-foreground/10",
            span { class: "font-mono text-[10px] font-semibold tracking-wide text-muted-foreground", "{file_extension_label(&attachment.name)}" }
            span { class: "truncate text-xs text-muted-foreground", "{attachment.name}" }
        }
    }
}

fn render_turn(key: usize, turn: &ChatTurn, latest_tool_index: Option<usize>) -> Element {
    let reconnecting = matches!(turn.blocks.last(), Some(ChatBlock::Reconnect { .. }));
    let block_count = turn.blocks.len();
    let blocks = turn
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(key, block)| {
            if turn.parent_tool_index(key).is_some() {
                return None;
            }
            let children = turn
                .blocks
                .iter()
                .enumerate()
                .filter(|(child_key, _)| turn.parent_tool_index(*child_key) == Some(key))
                .collect::<Vec<_>>();
            Some((key, block, children))
        })
        .collect::<Vec<_>>();
    let duration_label = turn.duration_secs.map(|duration| {
        if turn.step_count == 0 {
            let elapsed = fmt_elapsed(duration);
            translate_with(
                "agent-worked-for",
                &[("duration", TranslationValue::String(&elapsed))],
            )
        } else if turn.step_count == 1 {
            let elapsed = fmt_elapsed(duration);
            translate_with(
                "agent-worked-for-steps",
                &[
                    ("duration", TranslationValue::String(&elapsed)),
                    ("count", TranslationValue::Number(1)),
                ],
            )
        } else {
            let elapsed = fmt_elapsed(duration);
            translate_with(
                "agent-worked-for-steps",
                &[
                    ("duration", TranslationValue::String(&elapsed)),
                    ("count", TranslationValue::Number(turn.step_count as i64)),
                ],
            )
        }
    });
    rsx! {
        div {
            key: "{key}",
            class: "flex max-w-[92%] flex-col gap-2 self-start",
            style: "content-visibility:auto;contain-intrinsic-size:auto 180px;",
            if !blocks.is_empty() {
                div { class: "chat-assistant-turn relative flex flex-col gap-2.5 overflow-hidden rounded-2xl border px-3.5 py-3",
                    for (j , block , children) in blocks {
                        {render_block(
                            j,
                            block,
                            &children,
                            should_expand_thinking(j, block_count),
                            latest_tool_index == Some(j),
                        )}
                    }
                }
            }
            if turn.running && !reconnecting {
                WorkingIndicator {}
            } else if let Some(label) = duration_label {
                div { class: "flex items-center gap-2 px-1 text-sm text-muted-foreground/70",
                    span { class: "h-1.5 w-1.5 rounded-full bg-[color:var(--agent-accent)]" }
                    span { class: "tabular-nums", "{label}" }
                }
            }
        }
    }
}

fn render_disclosure_icon() -> Element {
    rsx! {
        span {
            class: "disclosure-icon relative inline-block h-3 w-3 shrink-0 text-muted-foreground",
            aria_hidden: "true",
        }
    }
}

#[component]
fn WorkingIndicator() -> Element {
    let mut elapsed = use_signal(|| 0u32);
    let mut verb = use_signal(|| translate("agent-working-working"));
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1000).await;
            elapsed.set(elapsed() + 1);
        }
    });
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(2500).await;
            let count = WORKING_VERB_IDS.len();
            let index = ((js_sys::Math::random() * count as f64) as usize).min(count - 1);
            verb.set(translate(WORKING_VERB_IDS[index]));
        }
    });
    let verb_text = verb();
    let elapsed_text = fmt_elapsed(elapsed());
    rsx! {
        div { class: "flex items-center gap-2 px-1 text-sm text-muted-foreground",
            span { class: "agent-working-label font-medium", "{verb_text}" }
            span { class: "flex items-end gap-0.5 text-[color:var(--agent-accent)]",
                span { class: "agent-working-dot h-1 w-1 rounded-full bg-current" }
                span { class: "agent-working-dot h-1 w-1 rounded-full bg-current [animation-delay:120ms]" }
                span { class: "agent-working-dot h-1 w-1 rounded-full bg-current [animation-delay:240ms]" }
            }
            span { class: "tabular-nums text-xs", "{elapsed_text}" }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivityIcon {
    Thinking,
    Writing,
    Installing,
    Awaiting,
    Python,
    ReadFile,
    WriteFile,
    Layout,
    Worktree,
    Search,
    Image,
    Screenshot,
    OpenPage,
    Command,
    Browser,
    Guardian,
    Subagent,
    Tool,
    Output,
    Error,
    Plan,
    Diff,
    Reconnect,
}

fn activity_icon_paths(kind: ActivityIcon) -> &'static [&'static str] {
    match kind {
        ActivityIcon::Thinking => &[
            "M9.5 4.5a3.2 3.2 0 0 1 5.35 1.05 3.35 3.35 0 0 1 2.8 3.35 3.5 3.5 0 0 1 .55 6.45A3.4 3.4 0 0 1 15 18.5H9a4 4 0 0 1-3.75-5.4 3.5 3.5 0 0 1 1.2-6.3A3.2 3.2 0 0 1 9.5 4.5Z",
            "M14.5 18.5c0 1.4.9 2.5 2.5 2.5v-4.4",
            "M9.4 4.7c-.9 1.2-.8 2.8.3 3.8",
            "M6.2 9.4c1.3-.7 2.8-.4 3.8.6",
            "M13.9 5.8c-.7 1-.6 2.2.2 3.1",
            "M14.1 9c1.4-.2 2.6.6 3.1 1.7",
            "M8.5 13.2c1-.7 2.4-.5 3.2.4",
            "M12.6 11.9c-.1 1.9.8 3.6 2.4 4.4",
        ],
        ActivityIcon::Writing => &["M12 20h9", "M16.5 3.5a2.12 2.12 0 0 1 3 3L8 18l-4 1 1-4Z"],
        ActivityIcon::Installing => &[
            "m7.5 4.27 9 5.15",
            "M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z",
            "M3.3 7 12 12l8.7-5",
            "M12 22V12",
        ],
        ActivityIcon::Awaiting => &["M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z", "M12 6v6l4 2"],
        ActivityIcon::Python => &[],
        ActivityIcon::ReadFile => &[
            "M12 7v14",
            "M3 18a1 1 0 0 1-1-1V5a2 2 0 0 1 2-2h5a3 3 0 0 1 3 3v15a3 3 0 0 0-3-3Z",
            "M21 18a1 1 0 0 0 1-1V5a2 2 0 0 0-2-2h-5a3 3 0 0 0-3 3v15a3 3 0 0 1 3-3Z",
        ],
        ActivityIcon::WriteFile => &["M12 20h9", "M16.5 3.5a2.12 2.12 0 0 1 3 3L8 18l-4 1 1-4Z"],
        ActivityIcon::Layout => &["M4 4h9v16H4Z", "M15 4h5v7h-5Z", "M15 13h5v7h-5Z"],
        ActivityIcon::Worktree => &[
            "M6 3v12",
            "M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
            "M6 6a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
            "M6 15c0 3 2 5 5 5h4",
        ],
        ActivityIcon::Search => &["M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z", "m21 21-4.35-4.35"],
        ActivityIcon::Image => &[
            "M19 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Z",
            "M10.5 8.5a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Z",
            "m21 15-5-5L5 21",
        ],
        ActivityIcon::Screenshot => &[
            "M9 4 7.5 6H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-2.5L15 4Z",
            "M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z",
        ],
        ActivityIcon::OpenPage => &[
            "M14 3h7v7",
            "m21 3-9 9",
            "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",
        ],
        ActivityIcon::Command => &["m4 17 6-6-6-6", "M12 19h8"],
        ActivityIcon::Browser => &[
            "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z",
            "M2 12h20",
            "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z",
        ],
        ActivityIcon::Guardian => &[
            "M20 13c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V5l8-3 8 3v8Z",
            "m9 12 2 2 4-4",
        ],
        ActivityIcon::Subagent => &[
            "M12 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
            "M5 21v-2a7 7 0 0 1 14 0v2",
            "M5.5 11a2.5 2.5 0 1 0 0-5",
            "M18.5 11a2.5 2.5 0 1 1 0-5",
        ],
        ActivityIcon::Tool => &[
            "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76Z",
        ],
        ActivityIcon::Output => &[
            "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5Z",
            "M14 2v6h6",
            "m10 17 3-3-3-3",
            "M13 14H7",
        ],
        ActivityIcon::Error => &[
            "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z",
            "M12 8v4",
            "M12 16h.01",
        ],
        ActivityIcon::Plan => &[
            "M4 19.5A2.5 2.5 0 0 1 6.5 17H20",
            "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z",
        ],
        ActivityIcon::Diff => &[
            "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z",
            "M14 2v4a2 2 0 0 0 2 2h4",
        ],
        ActivityIcon::Reconnect => &[
            "M5 12.55a11 11 0 0 1 14.08 0",
            "M1.42 9a16 16 0 0 1 21.16 0",
            "M8.53 16.11a6 6 0 0 1 6.95 0",
            "M12 20h.01",
        ],
    }
}

fn render_activity_icon(kind: ActivityIcon) -> Element {
    if kind == ActivityIcon::Thinking {
        return rsx! {
            span { class: "flex h-6 w-6 shrink-0 items-center justify-center text-[17px] leading-none", aria_hidden: "true", "🧠" }
        };
    }
    if kind == ActivityIcon::Python {
        return rsx! {
            span { class: "python-activity-icon flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ring-1 ring-inset", aria_hidden: "true",
                svg {
                    class: "h-[17px] w-[17px]",
                    view_box: "0 0 24 24",
                    path {
                        fill: "#3776ab",
                        d: "M11.7 2C7 2 7.3 4 7.3 4v2.1h4.5V7H5.5S2 6.6 2 12.2s3.1 5.4 3.1 5.4h1.8v-2.5s-.1-3 2.9-3h4.7s2.7 0 2.7-2.7V4.8S17.6 2 11.7 2Zm-2.5 1.5a.8.8 0 1 1 0 1.6.8.8 0 0 1 0-1.6Z",
                    }
                    path {
                        fill: "#ffd43b",
                        d: "M12.3 22c4.7 0 4.4-2 4.4-2v-2.1h-4.5V17h6.3s3.5.4 3.5-5.2-3.1-5.4-3.1-5.4h-1.8v2.5s.1 3-2.9 3H9.5s-2.7 0-2.7 2.7v4.6S6.4 22 12.3 22Zm2.5-1.5a.8.8 0 1 1 0-1.6.8.8 0 0 1 0 1.6Z",
                    }
                }
            }
        };
    }
    let paths = activity_icon_paths(kind);
    let tone = match kind {
        ActivityIcon::Thinking
        | ActivityIcon::Writing
        | ActivityIcon::Installing
        | ActivityIcon::Awaiting => "agent-themed-activity",
        ActivityIcon::Python => unreachable!(),
        ActivityIcon::ReadFile => "bg-sky-500/10 text-sky-600 ring-sky-500/20 dark:text-sky-300",
        ActivityIcon::WriteFile => {
            "bg-green-500/10 text-green-600 ring-green-500/20 dark:text-green-300"
        }
        ActivityIcon::Layout => {
            "bg-violet-500/10 text-violet-600 ring-violet-500/20 dark:text-violet-300"
        }
        ActivityIcon::Worktree => {
            "bg-emerald-500/10 text-emerald-600 ring-emerald-500/20 dark:text-emerald-300"
        }
        ActivityIcon::Search => "bg-cyan-500/10 text-cyan-600 ring-cyan-500/20 dark:text-cyan-300",
        ActivityIcon::Image => "bg-pink-500/10 text-pink-600 ring-pink-500/20 dark:text-pink-300",
        ActivityIcon::Screenshot => {
            "bg-fuchsia-500/10 text-fuchsia-600 ring-fuchsia-500/20 dark:text-fuchsia-300"
        }
        ActivityIcon::OpenPage => {
            "bg-blue-500/10 text-blue-600 ring-blue-500/20 dark:text-blue-300"
        }
        ActivityIcon::Command => {
            "bg-amber-500/10 text-amber-600 ring-amber-500/20 dark:text-amber-300"
        }
        ActivityIcon::Browser => "bg-blue-500/10 text-blue-600 ring-blue-500/20 dark:text-blue-300",
        ActivityIcon::Guardian => {
            "bg-emerald-500/10 text-emerald-600 ring-emerald-500/20 dark:text-emerald-300"
        }
        ActivityIcon::Subagent => {
            "bg-violet-500/10 text-violet-600 ring-violet-500/20 dark:text-violet-300"
        }
        ActivityIcon::Tool => {
            "bg-orange-500/10 text-orange-600 ring-orange-500/20 dark:text-orange-300"
        }
        ActivityIcon::Output => "bg-teal-500/10 text-teal-600 ring-teal-500/20 dark:text-teal-300",
        ActivityIcon::Error => "bg-red-500/10 text-red-600 ring-red-500/20 dark:text-red-300",
        ActivityIcon::Plan => {
            "bg-indigo-500/10 text-indigo-600 ring-indigo-500/20 dark:text-indigo-300"
        }
        ActivityIcon::Diff => {
            "bg-green-500/10 text-green-600 ring-green-500/20 dark:text-green-300"
        }
        ActivityIcon::Reconnect => {
            "bg-amber-500/10 text-amber-600 ring-amber-500/20 dark:text-amber-300"
        }
    };
    rsx! {
        span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ring-1 ring-inset {tone}", aria_hidden: "true",
            svg {
                class: "h-4 w-4",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.8",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                for path in paths {
                    path { d: "{path}" }
                }
            }
        }
    }
}

fn tool_activity_icon(activity: ToolActivity) -> ActivityIcon {
    match activity {
        ToolActivity::Guardian => ActivityIcon::Guardian,
        ToolActivity::ReadFile => ActivityIcon::ReadFile,
        ToolActivity::WriteFile => ActivityIcon::WriteFile,
        ToolActivity::Layout => ActivityIcon::Layout,
        ToolActivity::Worktree => ActivityIcon::Worktree,
        ToolActivity::Image => ActivityIcon::Image,
        ToolActivity::Screenshot => ActivityIcon::Screenshot,
        ToolActivity::OpenPage => ActivityIcon::OpenPage,
        ToolActivity::Browser => ActivityIcon::Browser,
        ToolActivity::Search => ActivityIcon::Search,
        ToolActivity::Command => ActivityIcon::Command,
        ToolActivity::Other => ActivityIcon::Tool,
    }
}

fn language_activity_icon(value: &str) -> Option<ActivityIcon> {
    let lower = value.to_ascii_lowercase();
    (lower.contains(".py") || lower == "py" || lower.contains("python"))
        .then_some(ActivityIcon::Python)
}

fn file_path_from_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            for key in ["path", "file_path", "filename", "file"] {
                if let Some(path) = map.get(key).and_then(serde_json::Value::as_str)
                    && !path.trim().is_empty()
                {
                    return Some(path.to_string());
                }
            }
            map.values().find_map(file_path_from_value)
        }
        serde_json::Value::Array(values) => values.iter().find_map(file_path_from_value),
        serde_json::Value::String(text) => file_path_from_text(text),
        _ => None,
    }
}

fn file_path_from_text(text: &str) -> Option<String> {
    for marker in ["*** Update File: ", "*** Add File: ", "*** Delete File: "] {
        if let Some(path) = text.lines().find_map(|line| line.strip_prefix(marker)) {
            return Some(path.trim().to_string());
        }
    }
    text.split_whitespace()
        .map(|token| token.trim_matches(['"', '\'', ',', ':', ';', '(', ')']))
        .find(|token| {
            if token.contains("://") {
                return false;
            }
            let name = token.rsplit('/').next().unwrap_or(token);
            name.rsplit_once('.')
                .is_some_and(|(_, ext)| !ext.is_empty() && ext.len() <= 12)
        })
        .map(ToOwned::to_owned)
}

fn tool_file_path(args: &str) -> Option<String> {
    serde_json::from_str(args)
        .ok()
        .and_then(|value| file_path_from_value(&value))
        .or_else(|| file_path_from_text(args))
}

fn render_file_activity_icon(path: &str, write: bool) -> Element {
    let tone = if write {
        "bg-green-500/10 text-green-600 ring-green-500/20 dark:text-green-300"
    } else {
        "bg-sky-500/10 text-sky-600 ring-sky-500/20 dark:text-sky-300"
    };
    rsx! {
        span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ring-1 ring-inset {tone}", aria_hidden: "true",
            {type_icon(path, false, "h-4 w-4")}
        }
    }
}

fn render_tool_activity_icon(name: &str, args: &str, fallback: ActivityIcon) -> Element {
    let activity = tool_activity(name);
    if matches!(
        activity,
        ToolActivity::ReadFile | ToolActivity::WriteFile | ToolActivity::Other
    ) && let Some(path) = tool_file_path(args)
    {
        return render_file_activity_icon(&path, activity == ToolActivity::WriteFile);
    }
    if matches!(file_icon_kind(name, false), FileIcon::Logo(_)) {
        return render_file_activity_icon(name, false);
    }
    render_activity_icon(fallback)
}

fn tool_activity_icon_for(name: &str, args: &str) -> ActivityIcon {
    language_activity_icon(args)
        .or_else(|| language_activity_icon(name))
        .unwrap_or_else(|| tool_activity_icon(tool_activity(name)))
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

fn tool_presentation(name: &str, args: &str) -> (ActivityIcon, String) {
    let activity = tool_activity(name);
    let icon = tool_activity_icon_for(name, args);
    match activity {
        ToolActivity::Guardian => (icon, translate("agent-tool-guardian-review")),
        ToolActivity::ReadFile => (icon, translate("agent-tool-read-files")),
        ToolActivity::WriteFile => (icon, translate("agent-edited")),
        ToolActivity::Layout => (icon, translate("schema-layout")),
        ToolActivity::Worktree => (icon, translate("layout-worktree")),
        ToolActivity::Image => (icon, translate("agent-tool-viewed-image")),
        ToolActivity::Screenshot => (icon, translate("agent-tool-viewed-image")),
        ToolActivity::OpenPage => (icon, translate("agent-tool-used-browser")),
        ToolActivity::Browser => (icon, translate("agent-tool-used-browser")),
        ToolActivity::Search => (icon, translate("agent-tool-searched-files")),
        ToolActivity::Command => (icon, translate("agent-tool-ran-commands")),
        ToolActivity::Other => (
            icon,
            name.rsplit(['.', ':'])
                .next()
                .unwrap_or(name)
                .replace('_', " "),
        ),
    }
}

fn normalized_tool_args(args: &str) -> Option<serde_json::Value> {
    let mut value = serde_json::from_str::<serde_json::Value>(args).ok()?;
    while let serde_json::Value::Object(map) = &value {
        let Some(arguments) = map.get("arguments") else {
            break;
        };
        if map.contains_key("server") || map.contains_key("tool") || map.contains_key("name") {
            value = arguments.clone();
        } else {
            break;
        }
    }
    Some(value)
}

fn tool_arg_label(key: &str) -> String {
    let mut label = key.replace('_', " ");
    if let Some(first) = label.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    label
}

fn tool_arg_is_path(key: &str, value: &str) -> bool {
    matches!(
        key,
        "path" | "file" | "file_path" | "cwd" | "dir" | "directory" | "workdir"
    ) || value.starts_with('/')
}

fn render_tool_arg(key: String, value: serde_json::Value) -> Element {
    let label = tool_arg_label(&key);
    let row_class = "relative flex min-w-0 items-center gap-3 py-1.5 pl-1 before:absolute before:-left-3 before:top-1/2 before:h-px before:w-2 before:bg-foreground/20";
    let label_class =
        "shrink-0 text-[10px] font-medium uppercase tracking-[0.1em] text-muted-foreground/80";
    match value {
        serde_json::Value::String(text) if tool_arg_is_path(&key, &text) => rsx! {
            div { class: "{row_class}",
                {type_icon(&text, false, "h-4 w-4 shrink-0 opacity-85")}
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "min-w-0 flex-1 truncate text-right font-mono text-[11px] text-foreground/80", title: "{text}", "{text}" }
            }
        },
        serde_json::Value::String(text)
            if matches!(
                key.as_str(),
                "cmd" | "command" | "script" | "patch" | "text" | "content"
            ) || text.contains('\n') =>
        {
            rsx! {
                div { class: "relative py-1.5 pl-1 before:absolute before:-left-3 before:top-3 before:h-px before:w-2 before:bg-foreground/20",
                    if !key.is_empty() {
                        div { class: "mb-1.5 flex items-center gap-1.5 {label_class}",
                            span { class: "h-1.5 w-1.5 rounded-full bg-emerald-400/70" }
                            "{label}"
                        }
                    }
                    pre { class: "max-h-56 overflow-auto whitespace-pre-wrap break-words border-l border-foreground/20 py-1 pl-3 font-mono text-[11px] leading-relaxed text-foreground/80", "{text}" }
                }
            }
        }
        serde_json::Value::String(text) => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "min-w-0 flex-1 truncate text-right font-mono text-[11px] text-foreground/80", title: "{text}", "{text}" }
            }
        },
        serde_json::Value::Bool(value) => {
            let tone = if value {
                "bg-emerald-500/10 text-emerald-600 ring-emerald-500/20 dark:text-emerald-300"
            } else {
                "bg-foreground/[0.04] text-muted-foreground ring-foreground/10"
            };
            rsx! {
                div { class: "{row_class}",
                    if !key.is_empty() {
                        span { class: "{label_class}", "{label}" }
                    }
                    span { class: "rounded-full px-2 py-0.5 text-[10px] font-semibold ring-1 ring-inset {tone}", "{value}" }
                }
            }
        }
        serde_json::Value::Number(value) => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                code { class: "ml-auto font-mono text-[11px] tabular-nums text-cyan-600 dark:text-cyan-300", "{value}" }
            }
        },
        serde_json::Value::Array(values) => rsx! {
            div { class: "relative py-1 pl-1 before:absolute before:-left-3 before:top-3 before:h-px before:w-2 before:bg-foreground/20",
                if !key.is_empty() {
                    div { class: "mb-1 {label_class}", "{label}" }
                }
                div { class: "ml-1 flex flex-col border-l border-foreground/20 pl-3",
                    for (index , value) in values.into_iter().enumerate() {
                        {render_tool_arg(format!("{}", index + 1), value)}
                    }
                }
            }
        },
        serde_json::Value::Object(values) => rsx! {
            div { class: "relative py-1 pl-1 before:absolute before:-left-3 before:top-3 before:h-px before:w-2 before:bg-foreground/20",
                if !key.is_empty() {
                    div { class: "mb-1 {label_class}", "{label}" }
                }
                div { class: "ml-1 flex flex-col border-l border-foreground/20 pl-3",
                    for (child_key , child_value) in values {
                        {render_tool_arg(child_key, child_value)}
                    }
                }
            }
        },
        serde_json::Value::Null => rsx! {
            div { class: "{row_class}",
                if !key.is_empty() {
                    span { class: "{label_class}", "{label}" }
                }
                span { class: "ml-auto text-[10px] italic text-muted-foreground/70", "None" }
            }
        },
    }
}

fn render_tool_args(args: &str) -> Element {
    let Some(value) = normalized_tool_args(args) else {
        return rsx! {
            pre { class: "agent-code-panel mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap rounded-lg p-2.5 font-mono text-[11px] leading-relaxed text-muted-foreground", "{args}" }
        };
    };
    match value {
        serde_json::Value::Object(map) if map.is_empty() => rsx! {},
        serde_json::Value::Object(map) => rsx! {
            div { class: "ml-1 mt-2 flex flex-col border-l border-foreground/20 pl-3", aria_label: "Tool arguments",
                for (key , value) in map {
                    {render_tool_arg(key, value)}
                }
            }
        },
        value => rsx! {
            div { class: "ml-1 mt-2 border-l border-foreground/20 pl-3", {render_tool_arg(String::new(), value)} }
        },
    }
}

fn render_block(
    key: usize,
    block: &ChatBlock,
    children: &[(usize, &ChatBlock)],
    latest_thinking: bool,
    latest_tool: bool,
) -> Element {
    match block {
        ChatBlock::Text(text) => rsx! {
            div {
                key: "{key}",
                class: "chat-md px-0.5 text-sm leading-relaxed text-foreground/95",
                dangerous_inner_html: md_to_html(text),
            }
        },
        ChatBlock::Thinking(text) => rsx! {
            div { key: "{key}", class: "agent-row-hover grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors",
                {render_activity_icon(ActivityIcon::Thinking)}
                details { open: latest_thinking, class: "disclosure min-w-0 text-sm text-muted-foreground",
                    summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                        span { class: "font-medium", {translate("agent-thinking")} }
                        {render_disclosure_icon()}
                    }
                    div { class: "mt-2 whitespace-pre-wrap border-l border-foreground/15 pl-3 text-xs leading-relaxed", "{text}" }
                }
            }
        },
        ChatBlock::ToolUse { name, args, .. } => {
            let (icon, label) = tool_presentation(name, args);
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-foreground/[0.025]",
                    {render_tool_activity_icon(name, args, icon)}
                    div { class: "min-w-0",
                        details { open: latest_tool, class: "disclosure text-sm text-muted-foreground",
                            summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                                span { class: "font-medium", "{label}" }
                                {render_disclosure_icon()}
                            }
                            div { class: "mt-1 text-[11px] font-medium text-foreground/45", "{name}" }
                            if !args.is_empty() && args != "{}" {
                                {render_tool_args(args)}
                            }
                        }
                        if !children.is_empty() {
                            div { class: "agent-context-tree ml-0.5 mt-1.5 flex flex-col gap-1 border-l pl-3",
                                for (child_key , child) in children {
                                    {render_tool_child(*child_key, child)}
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatBlock::Subagent(subagent) => {
            let status_label = subagent_status_label(&subagent.status);
            let status_class = subagent_status_class(&subagent.status);
            let title = if subagent.title.is_empty() {
                translate("agent-subagent")
            } else {
                subagent.title.replace('_', " ")
            };
            let action = subagent.action.replace('_', " ");
            let child_threads = subagent.child_thread_ids.join(", ");
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl bg-violet-500/[0.025] px-2 py-1.5 ring-1 ring-inset ring-violet-500/10 transition-colors hover:bg-violet-500/[0.05]",
                    {render_activity_icon(ActivityIcon::Subagent)}
                    div { class: "min-w-0",
                        details { open: subagent.status == "in_progress", class: "disclosure text-sm text-muted-foreground",
                            summary { class: "flex cursor-pointer select-none flex-wrap items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                                span { class: "font-medium text-foreground/85", "{title}" }
                                span { class: "rounded-full px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide {status_class}", "{status_label}" }
                                {render_disclosure_icon()}
                            }
                            div { class: "mt-2 flex flex-wrap gap-1.5 text-[10px]",
                                span { class: "rounded-full bg-violet-500/10 px-2 py-0.5 font-semibold text-violet-700 dark:text-violet-300", "{subagent.provider}" }
                                if !subagent.action.is_empty() {
                                    span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-foreground/60", "{action}" }
                                }
                                if let Some(agent_name) = &subagent.agent_name {
                                    span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-foreground/60", "{agent_name}" }
                                }
                                if let Some(model) = &subagent.model {
                                    span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 font-mono text-foreground/60", "{model}" }
                                }
                                if let Some(effort) = &subagent.reasoning_effort {
                                    span { class: "rounded-full bg-foreground/[0.055] px-2 py-0.5 text-foreground/60", "{effort}" }
                                }
                            }
                            if let Some(prompt) = &subagent.prompt {
                                div { class: "mt-2 rounded-lg bg-foreground/[0.025] p-2 text-xs leading-relaxed text-foreground/75 ring-1 ring-inset ring-foreground/10",
                                    div { class: "mb-1 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground/70", {translate("agent-prompt")} }
                                    div { class: "whitespace-pre-wrap", "{prompt}" }
                                }
                            }
                            div { class: "mt-2 grid gap-1 text-[10px] text-muted-foreground/75",
                                if let Some(thread_id) = &subagent.thread_id {
                                    div { span { class: "font-semibold", {format!("{} ", translate("agent-thread"))} } code { class: "font-mono", "{thread_id}" } }
                                }
                                if let Some(parent_thread_id) = &subagent.parent_thread_id {
                                    div { span { class: "font-semibold", {format!("{} ", translate("agent-parent"))} } code { class: "font-mono", "{parent_thread_id}" } }
                                }
                                if !child_threads.is_empty() {
                                    div { span { class: "font-semibold", {format!("{} ", translate("agent-children"))} } code { class: "break-all font-mono", "{child_threads}" } }
                                }
                                div { span { class: "font-semibold", {format!("{} ", translate("agent-call"))} } code { class: "font-mono", "{subagent.call_id}" } }
                            }
                            if !subagent.raw_input.is_empty() && subagent.raw_input != "{}" {
                                details { class: "disclosure mt-2 text-[11px] text-muted-foreground",
                                    summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                                        span { class: "font-medium", {translate("agent-raw-event")} }
                                        {render_disclosure_icon()}
                                    }
                                    pre { class: "agent-code-panel mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap rounded-lg p-2 font-mono text-[11px] text-muted-foreground", "{subagent.raw_input}" }
                                }
                            }
                        }
                        if !children.is_empty() {
                            div { class: "agent-context-tree ml-0.5 mt-2 flex flex-col gap-1 border-l border-violet-500/25 pl-3",
                                for (child_key , child) in children {
                                    {render_tool_child(*child_key, child)}
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatBlock::Plan { steps } => {
            let n = steps.len();
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-indigo-500/[0.035]",
                    {render_activity_icon(ActivityIcon::Plan)}
                    details { open: true, class: "disclosure min-w-0 text-sm",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                            span { class: "font-medium text-foreground/80", {translate("agent-plan")} }
                            span {
                                class: "text-xs text-muted-foreground",
                                {translate_with(
                                    "agent-tasks",
                                    &[("count", TranslationValue::Number(n as i64))],
                                )}
                            }
                            {render_disclosure_icon()}
                        }
                        ul { class: "mt-2 flex flex-col gap-1.5 border-l border-indigo-500/20 pl-3",
                            for (i , step) in steps.iter().enumerate() {
                                li { key: "{i}", class: "flex items-start gap-2 text-xs",
                                    span { class: "mt-px {plan_glyph_class(&step.status)}", "{plan_glyph(&step.status)}" }
                                    span { class: plan_text_class(&step.status), "{step.content}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatBlock::Diff {
            path,
            old_text,
            new_text,
            ..
        } => {
            let old = old_text.as_deref().unwrap_or("");
            let lines: Vec<(String, &'static str)> =
                similar::TextDiff::from_lines(old, new_text.as_str())
                    .iter_all_changes()
                    .filter_map(|c| match c.tag() {
                        similar::ChangeTag::Delete => Some((
                            format!("- {}", c.value().trim_end_matches('\n')),
                            "px-3 bg-red-500/10 text-red-300",
                        )),
                        similar::ChangeTag::Insert => Some((
                            format!("+ {}", c.value().trim_end_matches('\n')),
                            "px-3 bg-emerald-500/10 text-emerald-300",
                        )),
                        similar::ChangeTag::Equal => None,
                    })
                    .collect();
            let fname = path.rsplit('/').next().unwrap_or(path.as_str()).to_string();
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors hover:bg-green-500/[0.035]",
                    {render_file_activity_icon(path, true)}
                    details { class: "disclosure min-w-0 text-sm text-muted-foreground",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                            span { class: "font-medium", {format!("{} ", translate("agent-edited"))} }
                            code { class: "truncate font-mono text-xs text-foreground/70", "{fname}" }
                            {render_disclosure_icon()}
                        }
                        div { class: "mt-2 overflow-hidden rounded-lg ring-1 ring-inset ring-foreground/10",
                            div { class: "overflow-x-auto bg-foreground/[0.02] py-1 font-mono text-[11px] leading-relaxed",
                                for (i , (line , cls)) in lines.iter().enumerate() {
                                    div { key: "{i}", class: "{cls}", "{line}" }
                                }
                            }
                        }
                    }
                }
            }
        }
        ChatBlock::ToolResult {
            content, is_error, ..
        } => render_standalone_tool_result(key, content, *is_error),
        ChatBlock::Reconnect { attempt, total } => rsx! {
            div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-center gap-2.5 rounded-xl px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-amber-500/[0.035]",
                {render_activity_icon(ActivityIcon::Reconnect)}
                span {
                    class: "font-medium tabular-nums",
                    {translate_with(
                        "agent-reconnecting",
                        &[
                            ("attempt", TranslationValue::Number(*attempt as i64)),
                            ("total", TranslationValue::Number(*total as i64)),
                        ],
                    )}
                }
            }
        },
    }
}

fn render_tool_child(key: usize, block: &ChatBlock) -> Element {
    match block {
        ChatBlock::ToolUse { name, args, .. } => {
            let (_, label) = tool_presentation(name, args);
            rsx! {
                details { key: "{key}", class: "disclosure text-xs text-muted-foreground",
                    summary { class: "flex cursor-pointer select-none items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                        span { class: "font-medium", "{label}" }
                        {render_disclosure_icon()}
                    }
                    div { class: "mt-1 text-[11px] font-medium text-foreground/45", "{name}" }
                    if !args.is_empty() && args != "{}" {
                        {render_tool_args(args)}
                    }
                }
            }
        }
        ChatBlock::Subagent(subagent) => {
            let status_label = subagent_status_label(&subagent.status);
            let status_class = subagent_status_class(&subagent.status);
            rsx! {
                details { key: "{key}", class: "disclosure text-xs text-muted-foreground",
                    summary { class: "flex cursor-pointer select-none flex-wrap items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                        span { class: "font-medium", "{subagent.title}" }
                        span { class: "rounded-full px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide {status_class}", "{status_label}" }
                        {render_disclosure_icon()}
                    }
                    div { class: "mt-1 flex flex-wrap gap-1 text-[10px]",
                        span { class: "rounded-full bg-violet-500/10 px-1.5 py-0.5 text-violet-700 dark:text-violet-300", "{subagent.provider}" }
                        if let Some(agent_name) = &subagent.agent_name {
                            span { class: "rounded-full bg-foreground/[0.055] px-1.5 py-0.5", "{agent_name}" }
                        }
                    }
                    if let Some(prompt) = &subagent.prompt {
                        div { class: "mt-1.5 whitespace-pre-wrap rounded-lg bg-foreground/[0.025] p-2 text-[11px] leading-relaxed ring-1 ring-inset ring-foreground/10", "{prompt}" }
                    }
                }
            }
        }
        ChatBlock::ToolResult {
            content, is_error, ..
        } => render_nested_tool_result(key, content, *is_error),
        _ => rsx! {},
    }
}

fn subagent_status_label(status: &str) -> String {
    match status {
        "in_progress" => translate("agent-status-running"),
        "completed" => translate("agent-status-done"),
        "failed" => translate("agent-status-failed"),
        _ => translate("agent-status-pending"),
    }
}

fn subagent_status_class(status: &str) -> &'static str {
    match status {
        "in_progress" => "bg-violet-500/10 text-violet-700 dark:text-violet-300",
        "completed" => "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
        "failed" => "bg-red-500/10 text-red-700 dark:text-red-300",
        _ => "bg-amber-500/10 text-amber-700 dark:text-amber-300",
    }
}

fn render_nested_tool_result(key: usize, content: &str, is_error: bool) -> Element {
    let tone = if is_error {
        "text-red-600 dark:text-red-300"
    } else {
        "text-teal-700/80 dark:text-teal-300/80"
    };
    let panel = if is_error {
        "bg-red-500/[0.045] ring-red-500/15"
    } else {
        "bg-teal-500/[0.035] ring-teal-500/10"
    };
    let label = if is_error {
        translate("common-error")
    } else {
        translate("common-output")
    };
    rsx! {
        details { key: "{key}", class: "disclosure text-xs {tone}",
            summary { class: "flex cursor-pointer select-none items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                span { class: "font-medium", "{label}" }
                {render_disclosure_icon()}
            }
            pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap rounded-lg p-2 font-mono text-[11px] text-muted-foreground ring-1 ring-inset {panel}", "{content}" }
        }
    }
}

fn render_standalone_tool_result(key: usize, content: &str, is_error: bool) -> Element {
    let tone = if is_error {
        "text-red-600 dark:text-red-300"
    } else {
        "text-teal-700/80 dark:text-teal-300/80"
    };
    let panel = if is_error {
        "bg-red-500/[0.045] ring-red-500/15"
    } else {
        "bg-teal-500/[0.035] ring-teal-500/10"
    };
    let row = if is_error {
        "hover:bg-red-500/[0.035]"
    } else {
        "hover:bg-teal-500/[0.035]"
    };
    let label = if is_error {
        translate("common-error")
    } else {
        translate("common-output")
    };
    let icon = if is_error {
        ActivityIcon::Error
    } else {
        ActivityIcon::Output
    };
    rsx! {
        div { key: "{key}", class: "grid grid-cols-[1.5rem_minmax(0,1fr)] items-start gap-2.5 rounded-xl px-2 py-1.5 transition-colors {row}",
            {render_activity_icon(icon)}
            details { class: "disclosure min-w-0 text-sm {tone}",
                summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                    span { class: "font-medium", "{label}" }
                    {render_disclosure_icon()}
                }
                pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap rounded-lg p-2 font-mono text-[11px] text-muted-foreground ring-1 ring-inset {panel}", "{content}" }
            }
        }
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
fn avatar_node(icon: &str, accent: &str, agent: &str, name: &str, size_class: &str) -> Element {
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

fn fmt_elapsed(secs: u32) -> String {
    if secs >= 60 {
        format!("{}:{:02}", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

fn plan_glyph(status: &str) -> &'static str {
    match status {
        "completed" => "✓",
        "in_progress" => "◐",
        _ => "○",
    }
}

fn plan_glyph_class(status: &str) -> &'static str {
    match status {
        "completed" => "text-emerald-500",
        "in_progress" => "text-amber-500",
        _ => "text-muted-foreground",
    }
}

fn plan_text_class(status: &str) -> &'static str {
    match status {
        "completed" => "text-muted-foreground line-through",
        "in_progress" => "text-foreground",
        _ => "text-muted-foreground",
    }
}

/// Render assistant markdown to HTML, dropping any raw HTML the agent emits (markdown only —
/// never inject arbitrary markup into the page).
fn md_to_html(src: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, html};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(src, opts)
        .filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)));
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

/// Scoped styling for markdown rendered via `dangerous_inner_html` (Tailwind can't see generated
/// HTML, and its preflight strips heading/list defaults). Theme-neutral rgba so it works in both
/// light and dark.
const MD_CSS: &str = r#"
.agent-chat-prompt-shell::before{content:"";position:absolute;inset:-28px -42px;z-index:-1;border-radius:2.5rem;background:radial-gradient(60% 90% at 50% 75%,rgba(255,255,255,0.1),transparent 72%);pointer-events:none}
.agent-chat-page{background-image:radial-gradient(80% 55% at 15% 0%,color-mix(in srgb,var(--agent-accent) 9%,transparent),transparent 65%),radial-gradient(75% 55% at 90% 10%,color-mix(in srgb,var(--agent-accent) 7%,transparent),transparent 62%),radial-gradient(65% 45% at 55% 100%,color-mix(in srgb,var(--agent-accent) 5%,transparent),transparent 70%)}
.agent-chat-header{border-color:color-mix(in srgb,var(--agent-accent) 12%,transparent)}
.chat-user-bubble,.chat-assistant-turn{content-visibility:auto;contain-intrinsic-size:auto 160px;contain:layout paint style;transition:border-color 180ms ease,box-shadow 180ms ease,transform 180ms ease}
.chat-user-bubble{border-color:color-mix(in srgb,var(--agent-accent) 18%,transparent);background:linear-gradient(135deg,color-mix(in srgb,var(--agent-accent) 19%,transparent),color-mix(in srgb,var(--agent-accent) 9%,transparent) 58%,color-mix(in srgb,var(--agent-accent) 4%,transparent));box-shadow:0 10px 32px color-mix(in srgb,var(--agent-accent) 9%,transparent)}
.chat-user-bubble:hover{border-color:color-mix(in srgb,var(--agent-accent) 30%,transparent);box-shadow:0 14px 38px color-mix(in srgb,var(--agent-accent) 14%,transparent);transform:translateY(-1px)}
.chat-assistant-turn{border-color:color-mix(in srgb,var(--agent-accent) 9%,rgba(127,127,127,0.08));background:linear-gradient(135deg,color-mix(in srgb,var(--agent-accent) 5%,transparent),rgba(127,127,127,0.025) 55%,transparent);box-shadow:0 10px 35px rgba(0,0,0,0.035)}
.chat-assistant-turn::before{content:"";position:absolute;inset:0 auto 0 0;width:2px;background:linear-gradient(180deg,color-mix(in srgb,var(--agent-accent) 82%,transparent),color-mix(in srgb,var(--agent-accent) 52%,transparent),color-mix(in srgb,var(--agent-accent) 28%,transparent));opacity:0.75}
.chat-assistant-turn:hover{border-color:color-mix(in srgb,var(--agent-accent) 17%,transparent);box-shadow:0 14px 40px color-mix(in srgb,var(--agent-accent) 5%,rgba(0,0,0,0.055))}
.chat-assistant-turn .disclosure>summary{transition:color 160ms ease}
.chat-assistant-turn .disclosure>summary:hover{color:color-mix(in srgb,currentColor 68%,var(--agent-accent))}
.agent-themed-activity{color:var(--agent-accent);background:color-mix(in srgb,var(--agent-accent) 11%,transparent);box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--agent-accent) 18%,transparent)}
.python-activity-icon{background:linear-gradient(145deg,rgba(55,118,171,0.15),rgba(255,212,59,0.11));color:#3776ab;box-shadow:inset 0 0 0 1px rgba(55,118,171,0.3)}
.agent-working-label{color:color-mix(in srgb,var(--agent-accent) 82%,currentColor)}
.agent-row-hover:hover{background:color-mix(in srgb,var(--agent-accent) 4%,transparent)}
.agent-code-panel,.user-context-content{background:color-mix(in srgb,var(--agent-accent) 4%,transparent);box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--agent-accent) 11%,transparent)}
.agent-context-tree{border-color:color-mix(in srgb,var(--agent-accent) 22%,transparent)}
.agent-turn-meta{color:color-mix(in srgb,var(--agent-accent) 72%,currentColor);border-color:color-mix(in srgb,var(--agent-accent) 13%,transparent);background:color-mix(in srgb,var(--agent-accent) 7%,transparent)}
.agent-turn-meta-dot{background:var(--agent-accent)}
.user-context-panel{border-color:color-mix(in srgb,var(--agent-accent) 14%,transparent);background:color-mix(in srgb,var(--agent-accent) 5%,rgba(127,127,127,0.025))}
.user-context-panel>summary:hover{color:color-mix(in srgb,currentColor 65%,var(--agent-accent))}
.chat-md{line-height:1.6;word-break:break-word}
.chat-md>*:first-child{margin-top:0}
.chat-md>*:last-child{margin-bottom:0}
.chat-md h1,.chat-md h2,.chat-md h3,.chat-md h4{font-weight:600;line-height:1.3;margin:0.9em 0 0.35em}
.chat-md h1{font-size:1.35em}
.chat-md h2{font-size:1.2em}
.chat-md h3{font-size:1.05em}
.chat-md h4{font-size:1em}
.chat-md p{margin:0.5em 0}
.chat-md ul,.chat-md ol{margin:0.4em 0;padding-left:1.4em}
.chat-md ul{list-style:disc}
.chat-md ol{list-style:decimal}
.chat-md li{margin:0.15em 0}
.chat-md li>ul,.chat-md li>ol{margin:0.15em 0}
.chat-md strong{font-weight:600}
.chat-md em{font-style:italic}
.chat-md a{color:color-mix(in srgb,var(--agent-accent) 82%,currentColor);text-decoration-color:color-mix(in srgb,var(--agent-accent) 45%,transparent);text-underline-offset:0.16em}
.chat-md code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:0.88em;background:color-mix(in srgb,var(--agent-accent) 10%,transparent);border:1px solid color-mix(in srgb,var(--agent-accent) 11%,transparent);padding:0.1em 0.35em;border-radius:0.4em}
.chat-md pre{background:linear-gradient(135deg,color-mix(in srgb,var(--agent-accent) 7%,transparent),color-mix(in srgb,var(--agent-accent) 3%,transparent));border:1px solid color-mix(in srgb,var(--agent-accent) 11%,transparent);padding:0.7em 0.9em;border-radius:0.7em;overflow-x:auto;margin:0.6em 0}
.chat-md pre code{background:none;border:0;padding:0;font-size:0.85em}
.chat-md blockquote{border-left:2px solid color-mix(in srgb,var(--agent-accent) 48%,transparent);padding-left:0.8em;margin:0.5em 0;opacity:0.85}
.chat-md hr{border:0;border-top:1px solid rgba(127,127,127,0.25);margin:0.9em 0}
.chat-md table{border-collapse:collapse;margin:0.5em 0;font-size:0.95em}
.chat-md th,.chat-md td{border:1px solid rgba(127,127,127,0.3);padding:0.3em 0.6em;text-align:left}
@media (prefers-reduced-motion:reduce){.agent-chat-caret{animation:none}.chat-user-bubble,.chat-assistant-turn{transition:none}.chat-user-bubble:hover{transform:none}}
"#;
