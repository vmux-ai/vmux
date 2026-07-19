#![allow(non_snake_case)]

use crate::chat_page::composer::{
    PromptEdit, PromptHistoryDirection, ResumeMenuState, SelectorMode, ToolActivity,
    chat_page_title, edit_prompt, filter_models, filter_sessions, inline_media_query,
    is_handoff_boundary, menu_direction, move_prompt_history, move_selection,
    prompt_history_direction, prompt_prefix_at_utf16, replace_inline_media_query,
    resume_menu_state, selector_mode, should_clear_draft_on_escape, should_expand_thinking,
    should_fetch_resume, tool_activity,
};
use crate::chat_page::event::{
    CHAT_ATTACHMENT_PREVIEWS_EVENT, CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT,
    CHAT_SNAPSHOT_EVENT, ChatApproval, ChatAttachPaths, ChatAttachment,
    ChatAttachmentPreviewRequest, ChatAttachments, ChatBlock, ChatCancel, ChatCancelQueuedPrompt,
    ChatClearQueue, ChatEscape, ChatItem, ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest,
    ChatPasteMedia, ChatPickFiles, ChatResume, ChatSnapshot, ChatSubmit, ChatSubmitAttachment,
    ChatTurn, ConfigureWorkspace, MODEL_STATE_EVENT, ModelOptionEntry, ModelState,
    QueuedPromptSnapshot, RESUMABLE_SESSIONS_EVENT, ResumableSessionEntry, ResumableSessions,
    ResumeListRequest, ResumeSession, RuntimeSwitchRequest, SLASH_COMMANDS_EVENT, SelectModel,
    SelectWorkspace, SlashCommandEntry, SlashCommands, WORKING_VERBS,
};
use dioxus::prelude::*;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use vmux_terminal::matrix_rain::MatrixRain;
use vmux_terminal::page::PromptGhost;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::favicon::favicon_src_for_url;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::{JsCast, closure::Closure};

const PROMPT_ID: &str = "agent-chat-prompt";

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

fn prompt_textarea() -> Option<web_sys::HtmlTextAreaElement> {
    web_sys::window()?
        .document()?
        .get_element_by_id(PROMPT_ID)?
        .dyn_into()
        .ok()
}

fn resize_prompt_textarea() {
    let Some(textarea) = prompt_textarea() else {
        return;
    };
    let _ = textarea.set_attribute("style", "height:auto;overflow-y:hidden");
    let height = textarea.scroll_height().clamp(40, 160);
    let overflow = if height == 160 { "auto" } else { "hidden" };
    let _ = textarea.set_attribute("style", &format!("height:{height}px;overflow-y:{overflow}"));
}

fn sync_prompt_caret(mut caret: Signal<Option<u32>>, mut scroll_top: Signal<i32>) {
    let Some(textarea) = prompt_textarea() else {
        return;
    };
    let start = textarea.selection_start().ok().flatten().unwrap_or(0);
    let end = textarea.selection_end().ok().flatten().unwrap_or(start);
    caret.set((start == end).then_some(start));
    scroll_top.set(textarea.scroll_top());
}

fn focus_prompt_end() {
    let closure = Closure::once(move || {
        let Some(textarea) = prompt_textarea() else {
            return;
        };
        let end = textarea.value().encode_utf16().count() as u32;
        let _ = textarea.focus();
        let _ = textarea.set_selection_range(end, end);
    });
    if let Some(window) = web_sys::window() {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            0,
        );
    }
    closure.forget();
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

fn media_reference(entry: &ChatMediaEntry) -> String {
    let encode = |value: &str| value.replace('%', "%25").replace(' ', "%20");
    if entry.parent == "~" {
        format!("~/{name}", name = encode(&entry.name))
    } else {
        format!(
            "{parent}/{name}",
            parent = encode(&entry.parent),
            name = encode(&entry.name)
        )
    }
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
    focus_prompt_end();
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
    workspace_stage: Signal<String>,
) {
    let closure = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
        if !workspace_stage.peek().is_empty() {
            return;
        }
        let Some(textarea) = prompt_textarea() else {
            return;
        };
        let prompt_focused = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element())
            .is_some_and(|element| element.id() == PROMPT_ID);
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
        if direction.is_some() || (selector_open && selector_key) {
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
pub fn Page() -> Element {
    use_theme();
    let agent = current_agent();
    let mut items = use_signal(Vec::<ChatItem>::new);
    let mut status = use_signal(|| "installing".to_string());
    let mut error = use_signal(String::new);
    let mut approval = use_signal(|| Option::<(String, String, String)>::None);
    let mut agent_name = use_signal(String::new);
    let mut agent_icon = use_signal(String::new);
    let mut accent = use_signal(String::new);
    let mut handoff_source = use_signal(String::new);
    let mut handoff_truncated = use_signal(|| false);
    let mut handoff_message_count = use_signal(|| 0u32);
    let mut workspace_stage = use_signal(String::new);
    let mut workspace_path = use_signal(String::new);
    let mut workspace_branch = use_signal(String::new);
    let mut workspace_error = use_signal(String::new);
    let mut draft = use_signal(String::new);
    let mut attachments = use_signal(Vec::<ChatAttachment>::new);
    let mut attachment_previews = use_signal(HashMap::<String, ChatAttachment>::new);
    let mut attachment_preview_requests = use_signal(HashSet::<String>::new);
    let mut history_cursor = use_signal(|| None::<usize>);
    let mut history_scratch = use_signal(String::new);
    let mut prompt_caret = use_signal(|| None::<u32>);
    let prompt_scroll_top = use_signal(|| 0i32);
    let mut elapsed = use_signal(|| 0u32);
    let mut at_bottom = use_signal(|| true);
    let mut last_top = use_signal(|| 0i32);
    let mut queued = use_signal(Vec::<QueuedPromptSnapshot>::new);
    let mut paused = use_signal(|| false);
    let mut slash_cmds = use_signal(Vec::<SlashCommandEntry>::new);
    let mut sessions = use_signal(Vec::<ResumableSessionEntry>::new);
    let mut models = use_signal(Vec::<ModelOptionEntry>::new);
    let mut media_entries = use_signal(Vec::<ChatMediaEntry>::new);
    let mut media_request_id = use_signal(|| 0u64);
    let mut media_requested_query = use_signal(|| None::<String>);
    let mut media_loading = use_signal(|| false);
    let mut current_model_id = use_signal(String::new);
    let mut current_model = use_signal(String::new);
    let mut menu_sel = use_signal(|| 0usize);
    let mut resume_requested = use_signal(|| false);
    let mut resume_loading = use_signal(|| false);
    let mut verb = use_signal(|| "Working".to_string());

    use_effect(move || install_global_prompt_input(draft, slash_cmds, workspace_stage));
    use_effect(move || {
        let _ = draft.read();
        resize_prompt_textarea();
    });

    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1000).await;
            if matches!(status().as_str(), "streaming" | "installing") {
                elapsed.set(elapsed() + 1);
            } else if elapsed() != 0 {
                elapsed.set(0);
            }
        }
    });

    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(2500).await;
            if status() == "streaming" {
                let n = WORKING_VERBS.len();
                let idx = ((js_sys::Math::random() * n as f64) as usize).min(n - 1);
                verb.set(WORKING_VERBS[idx].to_string());
            }
        }
    });

    use_effect(move || {
        // Subscribe to any transcript/status change (each snapshot is a fresh `set`). Only pin to
        // the bottom when the user is already there — if they scrolled up to read, leave them.
        let _ = items.read().len();
        let _ = status.read();
        if !*at_bottom.peek() {
            return;
        }
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("chat-scroll"))
        {
            el.set_scroll_top(el.scroll_height());
        }
    });

    let _listener = use_bin_event_listener::<ChatSnapshot, _>(CHAT_SNAPSHOT_EVENT, move |snap| {
        if let Ok(parsed) = serde_json::from_str::<Vec<ChatItem>>(&snap.messages_json) {
            let known = attachment_previews
                .peek()
                .keys()
                .cloned()
                .collect::<HashSet<_>>();
            let mut requested = attachment_preview_requests.peek().clone();
            let paths = parsed
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
            if !paths.is_empty()
                && try_cef_bin_emit_rkyv(&ChatAttachmentPreviewRequest { paths }).is_ok()
            {
                attachment_preview_requests.set(requested);
            }
            items.set(parsed);
        }
        status.set(snap.status.clone());
        error.set(snap.error.clone());
        queued.set(snap.queued.clone());
        paused.set(snap.paused);
        agent_name.set(snap.agent_name.clone());
        agent_icon.set(snap.agent_icon.clone());
        accent.set(snap.accent_color.clone());
        handoff_source.set(snap.handoff_source.clone());
        handoff_truncated.set(snap.handoff_truncated);
        handoff_message_count.set(snap.handoff_message_count);
        workspace_stage.set(snap.workspace_stage.clone());
        workspace_path.set(snap.workspace_path.clone());
        workspace_branch.set(snap.workspace_branch.clone());
        workspace_error.set(snap.workspace_error.clone());
        if snap.status == "awaiting" {
            approval.set(Some((
                snap.approval_call_id.clone(),
                snap.approval_name.clone(),
                snap.approval_args_json.clone(),
            )));
        } else {
            approval.set(None);
        }
    });
    let _attachments =
        use_bin_event_listener::<ChatAttachments, _>(CHAT_ATTACHMENTS_EVENT, move |selected| {
            let mut next = attachments.peek().clone();
            let mut previews = attachment_previews.peek().clone();
            for attachment in &selected.attachments {
                previews.insert(attachment.path.clone(), attachment.clone());
                if !next.iter().any(|existing| existing.path == attachment.path) {
                    next.push(attachment.clone());
                }
            }
            attachment_previews.set(previews);
            attachments.set(next);
            focus_prompt_end();
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

    use_effect(move || {
        let name = {
            let n = agent_name();
            if n.is_empty() { current_agent() } else { n }
        };
        let title = chat_page_title(&items.read(), &status(), &name);
        if let Some(doc) = web_sys::window().and_then(|w| w.document())
            && doc.title() != title
        {
            doc.set_title(&title);
        }
    });

    let header_name = {
        let n = agent_name();
        if n.is_empty() { agent.clone() } else { n }
    };
    let agent_accent = agent_accent(&agent);
    let installing = status() == "installing";
    let installing_splash = installing && items.read().is_empty();
    let workspace_setup = !workspace_stage().is_empty();
    let show_capability_examples =
        items.read().is_empty() && queued.read().is_empty() && attachments.read().is_empty();
    let install_detail = {
        let detail = error();
        if detail.is_empty() {
            "Preparing agent…".to_string()
        } else {
            detail
        }
    };
    let draft_val = draft();
    let prompt_caret_prefix = prompt_caret()
        .filter(|_| !draft_val.is_empty())
        .map(|offset| prompt_prefix_at_utf16(&draft_val, offset).to_string());
    let prompt_scroll_offset = prompt_scroll_top();
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
    let resume_state = resume_query.map(|_| {
        resume_menu_state(
            resume_requested(),
            resume_loading(),
            sessions.read().len(),
            filtered_sessions.len(),
        )
    });

    use_effect(move || {
        let selected = menu_sel();
        let _ = draft.read();
        let _ = sessions.read().len();
        let _ = models.read().len();
        let _ = media_entries.read().len();
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| {
                document.get_element_by_id(&format!("agent-selector-item-{selected}"))
            })
        {
            let options = web_sys::ScrollIntoViewOptions::new();
            options.set_block(web_sys::ScrollLogicalPosition::Nearest);
            element.scroll_into_view_with_scroll_into_view_options(&options);
        }
    });

    rsx! {
        main {
            class: "relative isolate flex h-screen flex-col overflow-hidden bg-background text-foreground",
            style: "background-image:radial-gradient(120% 80% at 50% -10%, rgba(129,140,248,0.05), transparent 55%);",
            style { dangerous_inner_html: MD_CSS }
            if installing_splash {
                div { class: "pointer-events-none absolute inset-0 z-0 overflow-hidden bg-background opacity-75",
                    MatrixRain {
                        accent_rgb: agent_accent.rain_rgb.to_string(),
                        words: vec![header_name.to_uppercase()],
                    }
                }
            } else {
                div { class: "pointer-events-none absolute inset-0 -z-10 overflow-hidden",
                    div { class: "absolute left-1/2 top-[-10%] h-[30rem] w-[30rem] -translate-x-1/2 rounded-full blur-[150px] dark:bg-indigo-500/10" }
                }
            }
            header { class: "relative z-10 flex items-center gap-2.5 border-b border-foreground/10 bg-background/50 px-5 py-3 backdrop-blur-xl",
                {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-6 w-6 text-[11px]")}
                span { class: "h-2.5 w-2.5 rounded-full {status_dot_class(&status())}" }
                span { class: "bg-gradient-to-b from-foreground to-foreground/60 bg-clip-text text-sm font-semibold capitalize text-transparent",
                    "{header_name}"
                }
                if !current_model().is_empty() {
                    span { class: "min-w-0 truncate rounded-md bg-foreground/[0.06] px-2 py-0.5 font-mono text-[10px] text-muted-foreground ring-1 ring-inset ring-foreground/10",
                        "{current_model}"
                    }
                }
            }
            div {
                id: "chat-scroll",
                class: "relative z-10 flex-1 overflow-y-auto px-4 py-6",
                onscroll: move |_| {
                    if let Some(el) = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id("chat-scroll"))
                    {
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
                    }
                },
                div { class: "mx-auto flex min-h-full max-w-3xl flex-col gap-4",
                    if installing_splash {
                        div { class: "my-auto flex flex-col items-center gap-3 py-16 text-center",
                            {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-14 w-14 text-xl")}
                            h2 { class: "bg-gradient-to-b from-foreground to-foreground/50 bg-clip-text text-3xl font-semibold capitalize tracking-tight text-transparent",
                                "{header_name}"
                            }
                            div { class: "flex max-w-sm items-center gap-2 rounded-full bg-background/70 px-3 py-1.5 text-xs text-muted-foreground ring-1 ring-inset ring-foreground/10 backdrop-blur-xl",
                                span { class: "h-1.5 w-1.5 shrink-0 animate-pulse rounded-full {agent_accent.accent_bg}" }
                                span { class: "truncate", "{install_detail}" }
                            }
                        }
                    } else if items.read().is_empty() && status() == "idle" {
                        div { class: "flex flex-col items-center gap-3 py-24 text-center",
                            {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-14 w-14 text-xl")}
                            h2 { class: "bg-gradient-to-b from-foreground to-foreground/50 bg-clip-text text-3xl font-semibold capitalize tracking-tight text-transparent",
                                "{header_name}"
                            }
                            p { class: "text-sm text-muted-foreground", "Ready when you are." }
                        }
                    }
                    for (i , item) in items.read().iter().enumerate() {
                        {render_item(i, item, &verb(), elapsed(), attachment_previews)}
                        if !handoff_source().is_empty()
                            && is_handoff_boundary(i, handoff_message_count())
                        {
                            div { class: "flex items-center gap-2 py-1 text-xs text-muted-foreground",
                                span { class: "h-px flex-1 bg-foreground/10" }
                                span { "Continued from {handoff_source}" }
                                if handoff_truncated() {
                                    span { class: "text-amber-500/80", "· older context omitted" }
                                }
                                span { class: "h-px flex-1 bg-foreground/10" }
                            }
                        }
                    }
                    if workspace_stage() == "workspace" {
                        div { class: "flex max-w-[85%] items-start gap-2.5 self-start",
                            {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-7 w-7 shrink-0 text-[10px]")}
                            div { class: "flex min-w-0 flex-col gap-3 rounded-2xl bg-foreground/[0.055] px-4 py-3 text-sm ring-1 ring-inset ring-foreground/10",
                                div { class: "font-medium text-foreground", "Where should I work?" }
                                p { class: "leading-relaxed text-muted-foreground",
                                    "Choose the project folder for this conversation."
                                }
                                button {
                                    class: "w-fit rounded-lg bg-foreground px-3 py-2 text-xs font-medium text-background transition hover:opacity-90 active:scale-[0.98]",
                                    onclick: move |_| {
                                        let _ = try_cef_bin_emit_rkyv(&SelectWorkspace);
                                    },
                                    "Choose folder"
                                }
                                if !workspace_error().is_empty() {
                                    div { class: "rounded-lg bg-red-500/10 px-3 py-2 text-xs text-red-600 ring-1 ring-inset ring-red-500/20 dark:text-red-300",
                                        "{workspace_error}"
                                    }
                                }
                            }
                        }
                    } else if workspace_stage() == "branch" {
                        div { class: "flex max-w-[85%] items-start gap-2.5 self-start",
                            {avatar_node(&agent_icon(), &accent(), &agent, &header_name, "h-7 w-7 shrink-0 text-[10px]")}
                            div { class: "flex min-w-0 flex-1 flex-col gap-3 rounded-2xl bg-foreground/[0.055] px-4 py-3 text-sm ring-1 ring-inset ring-foreground/10",
                                div { class: "font-medium text-foreground", "How should I work here?" }
                                div { class: "truncate font-mono text-[11px] text-muted-foreground", "{workspace_path}" }
                                label { class: "flex flex-col gap-1.5",
                                    span { class: "text-xs text-muted-foreground", "New branch" }
                                    input {
                                        class: "w-full rounded-lg bg-background/70 px-3 py-2 font-mono text-xs text-foreground outline-none ring-1 ring-inset ring-foreground/15 focus:ring-foreground/30",
                                        value: "{workspace_branch}",
                                        oninput: move |event| workspace_branch.set(event.value()),
                                    }
                                }
                                if !workspace_error().is_empty() {
                                    div { class: "rounded-lg bg-red-500/10 px-3 py-2 text-xs text-red-600 ring-1 ring-inset ring-red-500/20 dark:text-red-300",
                                        "{workspace_error}"
                                    }
                                }
                                div { class: "flex flex-wrap justify-end gap-2",
                                    button {
                                        class: "rounded-lg px-3 py-2 text-xs font-medium text-muted-foreground transition hover:bg-foreground/10 hover:text-foreground",
                                        onclick: move |_| {
                                            let _ = try_cef_bin_emit_rkyv(&ConfigureWorkspace {
                                                branch: workspace_branch.peek().clone(),
                                                create_worktree: false,
                                            });
                                        },
                                        "Work here"
                                    }
                                    button {
                                        class: "rounded-lg bg-foreground px-3 py-2 text-xs font-medium text-background transition hover:opacity-90 active:scale-[0.98]",
                                        disabled: workspace_branch.read().trim().is_empty(),
                                        onclick: move |_| {
                                            let _ = try_cef_bin_emit_rkyv(&ConfigureWorkspace {
                                                branch: workspace_branch.peek().clone(),
                                                create_worktree: true,
                                            });
                                        },
                                        "Create worktree"
                                    }
                                }
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
                            span { class: "shrink-0", "interrupted" }
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
                                        "Allow "
                                        code { class: "font-mono text-amber-500", "{name}" }
                                        "?"
                                    }
                                    if !details.is_empty() {
                                        div { class: "mt-2 max-h-40 overflow-auto rounded-lg bg-foreground/[0.05] ring-1 ring-inset ring-foreground/10",
                                            for (i , detail) in details.iter().enumerate() {
                                                div {
                                                    key: "approval-detail-{i}",
                                                    class: "grid grid-cols-[7rem_minmax(0,1fr)] items-start gap-3 border-b border-foreground/10 px-3 py-2 last:border-b-0",
                                                    span { class: "pt-0.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground/70", "{detail.label}" }
                                                    pre { class: "overflow-x-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-muted-foreground", "{detail.value}" }
                                                }
                                            }
                                        }
                                    }
                                }
                                div { class: "flex justify-end gap-2",
                                    button {
                                        class: "rounded-lg px-3 py-1.5 text-sm text-muted-foreground hover:bg-foreground/10",
                                        onclick: {
                                            let call_id = call_id.clone();
                                            move |_| send_approval(call_id.clone(), 0)
                                        },
                                        "Deny"
                                    }
                                    button {
                                        class: "rounded-lg bg-foreground/10 px-3 py-1.5 text-sm hover:bg-foreground/20",
                                        onclick: {
                                            let call_id = call_id.clone();
                                            move |_| send_approval(call_id.clone(), 2)
                                        },
                                        "Allow always"
                                    }
                                    button {
                                        class: "rounded-lg bg-foreground px-3 py-1.5 text-sm font-medium text-background",
                                        onclick: {
                                            let call_id = call_id.clone();
                                            move |_| send_approval(call_id.clone(), 1)
                                        },
                                        "Allow"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div {
                    class: "relative z-10 bg-gradient-to-t from-background via-background/95 to-transparent px-4 pb-4 pt-8",
                    div {
                    class: "relative mx-auto flex max-w-3xl flex-col gap-2",
                    if media_menu_open {
                        div { class: "absolute bottom-full left-0 z-20 mb-2 max-h-80 w-full overflow-y-auto rounded-xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl",
                            if media_loading() {
                                div { class: "px-3.5 py-2 text-sm text-muted-foreground", "Loading media…" }
                            } else if media_entries.read().is_empty() {
                                div { class: "px-3.5 py-2 text-sm text-muted-foreground", "No matching media" }
                            } else {
                                for (i , entry) in media_entries.read().iter().cloned().enumerate() {
                                    {
                                        let entry = entry.clone();
                                        rsx! {
                                            div {
                                                key: "media-{entry.path}",
                                                id: "agent-selector-item-{i}",
                                                class: if i == menu_sel() { "flex cursor-pointer items-center gap-3 bg-foreground/10 px-3.5 py-2" } else { "flex cursor-pointer items-center gap-3 px-3.5 py-2" },
                                                onclick: move |_| select_media_entry(&entry, draft, menu_sel),
                                                div { class: "flex h-12 w-16 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-foreground/[0.06] text-muted-foreground ring-1 ring-inset ring-foreground/10",
                                                    if entry.is_dir {
                                                        svg {
                                                            class: "h-4 w-4",
                                                            view_box: "0 0 24 24",
                                                            fill: "none",
                                                            stroke: "currentColor",
                                                            stroke_width: "2",
                                                            stroke_linecap: "round",
                                                            stroke_linejoin: "round",
                                                            path { d: "M3 6h6l2 2h10v10H3z" }
                                                        }
                                                    } else if !entry.preview_data_url.is_empty() {
                                                        img {
                                                            src: "{entry.preview_data_url}",
                                                            alt: "{entry.name}",
                                                            class: "h-full w-full object-contain",
                                                        }
                                                    } else {
                                                        span { class: "font-mono text-[9px] font-semibold", "{file_extension_label(&entry.name)}" }
                                                    }
                                                }
                                                div { class: "min-w-0 flex-1",
                                                    div { class: "truncate text-sm text-foreground", "{entry.name}" }
                                                    div { class: "truncate text-xs text-muted-foreground", "{entry.parent}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if cmd_menu_open {
                        div { class: "absolute bottom-full left-0 z-20 mb-2 w-full overflow-hidden rounded-xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl",
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
                                            span { class: "text-xs text-muted-foreground", "{command.description}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if session_menu_open {
                        div { class: "absolute bottom-full left-0 z-20 mb-2 max-h-80 w-full overflow-y-auto rounded-xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl",
                            if resume_state == Some(ResumeMenuState::Loading) {
                                div { class: "px-3.5 py-2 text-sm text-muted-foreground", "Loading sessions…" }
                            } else if resume_state == Some(ResumeMenuState::Empty) {
                                div { class: "px-3.5 py-2 text-sm text-muted-foreground", "No resumable sessions found" }
                            } else if resume_state == Some(ResumeMenuState::NoMatch) {
                                div { class: "px-3.5 py-2 text-sm text-muted-foreground", "No matching sessions" }
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
                                                span { class: "truncate text-xs text-muted-foreground", "{session.subtitle}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if model_menu_open {
                        div { class: "absolute bottom-full left-0 z-20 mb-2 max-h-80 w-full overflow-y-auto rounded-xl border border-foreground/10 bg-background/95 shadow-xl backdrop-blur-xl",
                            if filtered_models.is_empty() {
                                div { class: "px-3.5 py-2 text-sm text-muted-foreground", "No matching models" }
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
                                                        span { class: "shrink-0 text-[10px] uppercase tracking-wide text-emerald-500", "current" }
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
                    if !queued.read().is_empty() {
                        div { class: "flex flex-col items-end gap-1.5",
                            for queued_prompt in queued.read().iter().cloned() {
                                div {
                                    key: "q{queued_prompt.id}",
                                    class: "group flex max-w-[80%] items-center gap-2 rounded-2xl border border-dashed border-foreground/20 bg-foreground/[0.03] py-2 pl-3.5 pr-2 text-sm text-muted-foreground",
                                    span { class: "shrink-0 text-[10px] uppercase tracking-wide text-foreground/40", "queued" }
                                    span { class: "min-w-0 flex-1 whitespace-pre-wrap break-words",
                                        if !queued_prompt.text.is_empty() {
                                            "{queued_prompt.text}"
                                        }
                                        if !queued_prompt.attachment_names.is_empty() {
                                            span { class: "block text-xs text-foreground/45",
                                                "Attached: "
                                                for (i , name) in queued_prompt.attachment_names.iter().enumerate() {
                                                    if i > 0 { ", " }
                                                    "{name}"
                                                }
                                            }
                                        }
                                    }
                                    button {
                                        class: "flex shrink-0 items-center rounded-lg p-1 text-foreground/35 opacity-70 transition hover:bg-foreground/10 hover:text-foreground hover:opacity-100 focus:opacity-100",
                                        title: "Cancel queued prompt",
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
                                        title: "Resume queued prompts",
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
                                        title: "Clear queue",
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
                                span { "send all now" }
                            }
                        }
                    }
                    div { class: "relative flex items-center overflow-hidden rounded-2xl bg-white/45 p-1 shadow-[0_18px_55px_-24px_rgba(0,0,0,0.65),inset_0_1px_0_rgba(255,255,255,0.18),inset_0_-1px_0_rgba(255,255,255,0.04)] ring-1 ring-inset ring-black/10 backdrop-blur-3xl backdrop-saturate-150 transition-all duration-200 focus-within:bg-white/55 focus-within:ring-black/20 focus-within:shadow-[0_22px_65px_-24px_rgba(0,0,0,0.72),inset_0_1px_0_rgba(255,255,255,0.22)] dark:bg-white/[0.045] dark:ring-white/[0.16] dark:focus-within:bg-white/[0.065] dark:focus-within:ring-white/25",
                        div { class: "pointer-events-none absolute inset-px rounded-[0.9rem] bg-gradient-to-b from-white/[0.12] via-white/[0.025] to-transparent dark:from-white/[0.10]" }
                        div { class: "pointer-events-none absolute -left-12 -top-12 h-24 w-72 rotate-[-5deg] rounded-full bg-white/[0.09] blur-2xl" }
                        button {
                            class: if workspace_setup { "relative z-10 ml-0.5 flex h-8 w-8 shrink-0 cursor-default self-center items-center justify-center rounded-lg text-foreground/25" } else { "relative z-10 ml-0.5 flex h-8 w-8 shrink-0 self-center items-center justify-center rounded-lg text-foreground/45 transition hover:bg-foreground/10 hover:text-foreground" },
                            disabled: workspace_setup,
                            title: "Attach files (/upload)",
                            onclick: move |_| {
                                let _ = try_cef_bin_emit_rkyv(&ChatPickFiles);
                            },
                            svg {
                                class: "h-4 w-4",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48" }
                            }
                        }
                        div { class: "relative z-10 flex min-w-0 flex-1 flex-wrap items-center gap-1 px-2",
                            for (i , attachment) in attachments.read().iter().cloned().enumerate() {
                                div {
                                    key: "attachment-pill-{attachment.path}",
                                    class: "group flex h-7 max-w-56 shrink-0 items-center gap-1.5 rounded-full bg-foreground/[0.08] pl-1 pr-1.5 text-xs text-foreground/80 ring-1 ring-inset ring-foreground/10",
                                    if attachment.preview_data_url.is_empty() {
                                        span { class: "flex h-5 min-w-5 items-center justify-center rounded-full bg-foreground/[0.08] px-1 font-mono text-[8px] font-semibold text-muted-foreground",
                                            "{attachment_label(&attachment)}"
                                        }
                                    } else {
                                        img {
                                            src: "{attachment.preview_data_url}",
                                            alt: "{attachment.name}",
                                            class: "h-5 w-5 rounded-full object-cover",
                                        }
                                    }
                                    span { class: "min-w-0 max-w-40 truncate", "{attachment.name}" }
                                    button {
                                        class: "flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-foreground/45 transition hover:bg-foreground/10 hover:text-foreground",
                                        title: "Remove attachment",
                                        onclick: move |_| {
                                            let mut next = attachments.peek().clone();
                                            if i < next.len() {
                                                next.remove(i);
                                                attachments.set(next);
                                            }
                                        },
                                        svg {
                                            class: "h-3 w-3",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2.5",
                                            stroke_linecap: "round",
                                            path { d: "M6 6l12 12M18 6L6 18" }
                                        }
                                    }
                                }
                            }
                            div { class: "relative min-w-32 flex-1 overflow-hidden",
                                if draft.read().is_empty() {
                                    div { class: "pointer-events-none absolute inset-0 flex -translate-y-px items-center overflow-hidden px-1.5",
                                        if workspace_setup {
                                            div { class: "max-w-full truncate whitespace-nowrap text-[15px] leading-6 text-muted-foreground/50", "Finish workspace setup above" }
                                        } else if show_capability_examples {
                                            PromptGhost {
                                                accent_bg: agent_accent.accent_bg.to_string(),
                                                terminal: false,
                                            }
                                        } else {
                                            div { class: "max-w-full truncate whitespace-nowrap text-[15px] leading-6 text-muted-foreground/50", "Type / for commands or @ for media" }
                                        }
                                    }
                                }
                                if let Some(prefix) = prompt_caret_prefix.as_ref() {
                                    div { class: "pointer-events-none absolute inset-0 z-20 overflow-hidden",
                                        div {
                                            class: "min-h-10 w-full whitespace-pre-wrap break-words px-1.5 py-2 text-[15px] leading-6 text-transparent",
                                            style: "transform:translateY(-{prompt_scroll_offset}px);",
                                            span { "{prefix}" }
                                            span { class: "agent-chat-caret relative top-px ml-px inline-block h-4 w-1.5 align-middle {agent_accent.accent_bg}" }
                                        }
                                    }
                                }
                                textarea {
                                id: PROMPT_ID,
                                class: "agent-chat-prompt relative z-10 max-h-40 min-h-10 w-full resize-none bg-transparent px-1.5 py-2 text-[15px] leading-6 placeholder:text-transparent focus:outline-none",
                                rows: "1",
                                placeholder: if workspace_setup { "Finish workspace setup…" } else { "Message the agent…" },
                                disabled: workspace_setup,
                                value: "{draft}",
                                oninput: move |e| {
                                    draft.set(e.value());
                                    history_cursor.set(None);
                                    history_scratch.set(String::new());
                                    menu_sel.set(0);
                                    sync_prompt_caret(prompt_caret, prompt_scroll_top);
                                },
                                onpaste: move |_| {
                                    let _ = try_cef_bin_emit_rkyv(&ChatPasteMedia);
                                },
                                onfocus: move |_| sync_prompt_caret(prompt_caret, prompt_scroll_top),
                                onblur: move |_| prompt_caret.set(None),
                                onkeyup: move |_| sync_prompt_caret(prompt_caret, prompt_scroll_top),
                                onmouseup: move |_| sync_prompt_caret(prompt_caret, prompt_scroll_top),
                                onscroll: move |_| sync_prompt_caret(prompt_caret, prompt_scroll_top),
                                onkeydown: move |e| {
                                    let streaming = matches!(status().as_str(), "streaming" | "awaiting");
                                    let draft_now = draft.peek().clone();
                                    let (cmd_items, sess_items, model_items, session_selector_open, model_selector_open) = match selector_mode(&draft_now) {
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
                                    let command_modifier = e.modifiers().meta()
                                        || e.modifiers().ctrl()
                                        || e.modifiers().alt();
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
                                    if selector_open
                                        && e.key() == Key::Enter
                                        && !e.modifiers().shift()
                                        && !command_modifier
                                    {
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
                                            focus_prompt_end();
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
                                        && let Some(textarea) = prompt_textarea()
                                    {
                                        let start = textarea
                                            .selection_start()
                                            .ok()
                                            .flatten()
                                            .unwrap_or_default();
                                        let end = textarea
                                            .selection_end()
                                            .ok()
                                            .flatten()
                                            .unwrap_or(start);
                                        if let Some(direction) = prompt_history_direction(
                                            &key,
                                            e.modifiers().ctrl(),
                                            &draft_now,
                                            start,
                                            end,
                                        ) {
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
                                                focus_prompt_end();
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
                                            workspace_setup,
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
                                },
                                }
                            }
                        }
                        if matches!(status().as_str(), "streaming" | "awaiting") {
                            if queued.read().is_empty() {
                                button {
                                    class: "relative z-10 mr-0.5 flex h-8 w-8 shrink-0 self-center items-center justify-center rounded-lg bg-white/40 text-foreground/70 shadow-sm ring-1 ring-inset ring-black/10 transition hover:bg-white/60 hover:text-foreground active:scale-95 dark:bg-white/[0.08] dark:ring-white/10 dark:hover:bg-white/[0.14]",
                                    title: "Stop",
                                    onclick: move |_| {
                                        let _ = try_cef_bin_emit_rkyv(&ChatCancel);
                                    },
                                    svg {
                                        class: "h-4 w-4",
                                        view_box: "0 0 24 24",
                                        fill: "currentColor",
                                        rect { x: "6", y: "6", width: "12", height: "12", rx: "2.5" }
                                    }
                                }
                            } else {
                                button {
                                    class: "relative z-10 mr-0.5 flex h-8 w-8 shrink-0 self-center items-center justify-center rounded-lg bg-gradient-to-br text-white shadow-lg transition hover:brightness-110 active:scale-95 {agent_accent.grad}",
                                    title: "Send all queued prompts now (Esc)",
                                    onclick: move |_| {
                                        let _ = try_cef_bin_emit_rkyv(&ChatEscape);
                                    },
                                    svg {
                                        class: "h-4 w-4",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        path { d: "M12 19V5" }
                                        path { d: "M5 12l7-7 7 7" }
                                    }
                                }
                            }
                        } else {
                            button {
                                class: if workspace_setup || draft.read().trim().is_empty() && attachments.read().is_empty() { "relative z-10 mr-0.5 flex h-8 w-8 shrink-0 cursor-default self-center items-center justify-center rounded-lg bg-white/25 text-muted-foreground/35 shadow-sm ring-1 ring-inset ring-black/[0.06] dark:bg-white/[0.055] dark:ring-white/[0.08]" } else { "relative z-10 mr-0.5 flex h-8 w-8 shrink-0 self-center items-center justify-center rounded-lg bg-gradient-to-br text-white shadow-lg transition hover:brightness-110 active:scale-95 {agent_accent.grad}" },
                                disabled: workspace_setup || draft.read().trim().is_empty() && attachments.read().is_empty(),
                                title: "Send (Enter)",
                                onclick: move |_| {
                                    do_submit(
                                        draft,
                                        attachments,
                                        history_cursor,
                                        history_scratch,
                                        at_bottom,
                                        workspace_setup,
                                    )
                                },
                                svg {
                                    class: "h-4 w-4",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M12 19V5" }
                                    path { d: "M5 12l7-7 7 7" }
                                }
                            }
                        }
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
    blocked: bool,
) {
    if blocked {
        return;
    }
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

fn send_approval(call_id: String, decision: u8) {
    let _ = try_cef_bin_emit_rkyv(&ChatApproval { call_id, decision });
}

fn render_item(
    key: usize,
    item: &ChatItem,
    verb: &str,
    elapsed: u32,
    attachment_previews: Signal<HashMap<String, ChatAttachment>>,
) -> Element {
    match item {
        ChatItem::User { text, attachments } => rsx! {
            div {
                key: "{key}",
                class: "flex max-w-[80%] self-end flex-col gap-2 rounded-2xl bg-foreground/[0.08] p-2.5 text-sm",
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
        ChatItem::Turn(turn) => render_turn(key, turn, verb, elapsed),
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

fn render_turn(key: usize, turn: &ChatTurn, verb: &str, elapsed: u32) -> Element {
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
    rsx! {
        div { key: "{key}", class: "flex max-w-[90%] flex-col gap-2.5 self-start",
            for (j , block , children) in blocks {
                {render_block(j, block, &children, should_expand_thinking(j, block_count))}
            }
            if turn.running && !reconnecting {
                {render_working(verb, elapsed)}
            }
            if !turn.running && let Some(duration) = turn.duration_secs {
                div { class: "grid grid-cols-[1.25rem_minmax(0,1fr)] gap-2.5 text-[11px] text-muted-foreground/70",
                    span {}
                    if turn.step_count == 0 {
                        span { class: "tabular-nums", "Worked for {fmt_elapsed(duration)}" }
                    } else if turn.step_count == 1 {
                        span { class: "tabular-nums", "Worked for {fmt_elapsed(duration)} · 1 step" }
                    } else {
                        span { class: "tabular-nums", "Worked for {fmt_elapsed(duration)} · {turn.step_count} steps" }
                    }
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

fn render_working(verb: &str, elapsed: u32) -> Element {
    rsx! {
        div { class: "grid grid-cols-[1.25rem_minmax(0,1fr)] items-center gap-2.5 py-1 text-sm text-muted-foreground",
            span { class: "flex h-5 w-5 items-center justify-center",
                span { class: "flex items-end gap-0.5",
                    span { class: "h-1 w-1 animate-bounce rounded-full bg-current [animation-delay:-0.32s]" }
                    span { class: "h-1 w-1 animate-bounce rounded-full bg-current [animation-delay:-0.16s]" }
                    span { class: "h-1 w-1 animate-bounce rounded-full bg-current" }
                }
            }
            div { class: "flex items-baseline gap-2",
                span { class: "animate-pulse font-medium text-foreground/75", "{verb}" }
                span { class: "tabular-nums text-xs", "{fmt_elapsed(elapsed)}" }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivityIcon {
    Thinking,
    ReadFile,
    Search,
    Image,
    Command,
    Browser,
    Guardian,
    Tool,
    Output,
    Error,
    Plan,
    Diff,
    Reconnect,
}

fn render_activity_icon(kind: ActivityIcon) -> Element {
    let paths: &[&str] = match kind {
        ActivityIcon::Thinking => &[
            "M9.5 4A2.5 2.5 0 0 1 12 6.5v11a2.5 2.5 0 0 1-4.96.44A2.5 2.5 0 0 1 4.5 15.5a3 3 0 0 1 .34-5.14A2.5 2.5 0 0 1 6.5 6a3 3 0 0 1 3-2Z",
            "M14.5 4A2.5 2.5 0 0 0 12 6.5v11a2.5 2.5 0 0 0 4.96.44A2.5 2.5 0 0 0 19.5 15.5a3 3 0 0 0-.34-5.14A2.5 2.5 0 0 0 17.5 6a3 3 0 0 0-3-2Z",
        ],
        ActivityIcon::ReadFile => &[
            "M12 7v14",
            "M3 18a1 1 0 0 1-1-1V5a2 2 0 0 1 2-2h5a3 3 0 0 1 3 3v15a3 3 0 0 0-3-3Z",
            "M21 18a1 1 0 0 0 1-1V5a2 2 0 0 0-2-2h-5a3 3 0 0 0-3 3v15a3 3 0 0 1 3-3Z",
        ],
        ActivityIcon::Search => &["M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z", "m21 21-4.35-4.35"],
        ActivityIcon::Image => &[
            "M19 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2Z",
            "M10.5 8.5a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Z",
            "m21 15-5-5L5 21",
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
    };
    let tone = if kind == ActivityIcon::Error {
        "text-red-500"
    } else {
        "text-muted-foreground"
    };
    rsx! {
        span { class: "flex h-5 w-5 shrink-0 items-center justify-center {tone}", aria_hidden: "true",
            svg {
                class: "h-[17px] w-[17px]",
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

fn tool_presentation(name: &str) -> (ActivityIcon, Cow<'static, str>) {
    match tool_activity(name) {
        ToolActivity::Guardian => (ActivityIcon::Guardian, Cow::Borrowed("Guardian Review")),
        ToolActivity::ReadFile => (ActivityIcon::ReadFile, Cow::Borrowed("Read files")),
        ToolActivity::Image => (ActivityIcon::Image, Cow::Borrowed("Viewed image")),
        ToolActivity::Browser => (ActivityIcon::Browser, Cow::Borrowed("Used browser")),
        ToolActivity::Search => (ActivityIcon::Search, Cow::Borrowed("Searched files")),
        ToolActivity::Command => (ActivityIcon::Command, Cow::Borrowed("Ran commands")),
        ToolActivity::Other => (
            ActivityIcon::Tool,
            Cow::Owned(
                name.rsplit(['.', ':'])
                    .next()
                    .unwrap_or(name)
                    .replace('_', " "),
            ),
        ),
    }
}

fn render_block(
    key: usize,
    block: &ChatBlock,
    children: &[(usize, &ChatBlock)],
    latest: bool,
) -> Element {
    match block {
        ChatBlock::Text(text) => rsx! {
            div {
                key: "{key}",
                class: "chat-md text-sm leading-relaxed",
                dangerous_inner_html: md_to_html(text),
            }
        },
        ChatBlock::Thinking(text) => rsx! {
            div { key: "{key}", class: "grid grid-cols-[1.25rem_minmax(0,1fr)] items-start gap-2.5 py-1",
                {render_activity_icon(ActivityIcon::Thinking)}
                details { open: latest, class: "disclosure min-w-0 text-sm text-muted-foreground",
                    summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                        span { class: "font-medium", "Thinking" }
                        {render_disclosure_icon()}
                    }
                    div { class: "mt-2 whitespace-pre-wrap border-l border-foreground/15 pl-3 text-xs leading-relaxed", "{text}" }
                }
            }
        },
        ChatBlock::ToolUse { name, args, .. } => {
            let (icon, label) = tool_presentation(name);
            rsx! {
                div { key: "{key}", class: "grid grid-cols-[1.25rem_minmax(0,1fr)] items-start gap-2.5 py-1",
                    {render_activity_icon(icon)}
                    div { class: "min-w-0",
                        details { class: "disclosure text-sm text-muted-foreground",
                            summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                                span { class: "font-medium", "{label}" }
                                {render_disclosure_icon()}
                            }
                            div { class: "mt-1 text-[11px] font-medium text-foreground/45", "{name}" }
                            if !args.is_empty() && args != "{}" {
                                pre { class: "mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap rounded-lg bg-foreground/[0.04] p-2 font-mono text-[11px] text-muted-foreground ring-1 ring-inset ring-foreground/10", "{args}" }
                            }
                        }
                        if !children.is_empty() {
                            div { class: "ml-0.5 mt-1.5 flex flex-col gap-1 border-l border-foreground/15 pl-3",
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
                div { key: "{key}", class: "grid grid-cols-[1.25rem_minmax(0,1fr)] items-start gap-2.5 py-1",
                    {render_activity_icon(ActivityIcon::Plan)}
                    details { open: true, class: "disclosure min-w-0 text-sm",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                            span { class: "font-medium text-foreground/80", "Plan" }
                            span { class: "text-xs text-muted-foreground", "{n} tasks" }
                            {render_disclosure_icon()}
                        }
                        ul { class: "mt-2 flex flex-col gap-1.5 border-l border-foreground/15 pl-3",
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
                div { key: "{key}", class: "grid grid-cols-[1.25rem_minmax(0,1fr)] items-start gap-2.5 py-1",
                    {render_activity_icon(ActivityIcon::Diff)}
                    details { class: "disclosure min-w-0 text-sm text-muted-foreground",
                        summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                            span { class: "font-medium", "Edited " }
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
            div { key: "{key}", class: "grid grid-cols-[1.25rem_minmax(0,1fr)] items-center gap-2.5 py-1 text-sm text-muted-foreground",
                {render_activity_icon(ActivityIcon::Reconnect)}
                span { class: "font-medium tabular-nums", "Reconnecting {attempt}/{total}" }
            }
        },
    }
}

fn render_tool_child(key: usize, block: &ChatBlock) -> Element {
    match block {
        ChatBlock::ToolUse { name, args, .. } => {
            let (_, label) = tool_presentation(name);
            rsx! {
                details { key: "{key}", class: "disclosure text-xs text-muted-foreground",
                    summary { class: "flex cursor-pointer select-none items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                        span { class: "font-medium", "{label}" }
                        {render_disclosure_icon()}
                    }
                    div { class: "mt-1 text-[11px] font-medium text-foreground/45", "{name}" }
                    if !args.is_empty() && args != "{}" {
                        pre { class: "mt-1.5 max-h-56 overflow-auto whitespace-pre-wrap rounded-lg bg-foreground/[0.04] p-2 font-mono text-[11px] text-muted-foreground ring-1 ring-inset ring-foreground/10", "{args}" }
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

fn render_nested_tool_result(key: usize, content: &str, is_error: bool) -> Element {
    let tone = if is_error {
        "text-red-500"
    } else {
        "text-muted-foreground"
    };
    let label = if is_error { "Error" } else { "Output" };
    rsx! {
        details { key: "{key}", class: "disclosure text-xs {tone}",
            summary { class: "flex cursor-pointer select-none items-center gap-2 py-0.5 list-none [&::-webkit-details-marker]:hidden",
                span { class: "font-medium", "{label}" }
                {render_disclosure_icon()}
            }
            pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap rounded-lg bg-foreground/[0.04] p-2 font-mono text-[11px] text-muted-foreground ring-1 ring-inset ring-foreground/10", "{content}" }
        }
    }
}

fn render_standalone_tool_result(key: usize, content: &str, is_error: bool) -> Element {
    let tone = if is_error {
        "text-red-500"
    } else {
        "text-muted-foreground"
    };
    let label = if is_error { "Error" } else { "Output" };
    let icon = if is_error {
        ActivityIcon::Error
    } else {
        ActivityIcon::Output
    };
    rsx! {
        div { key: "{key}", class: "grid grid-cols-[1.25rem_minmax(0,1fr)] items-start gap-2.5 py-1",
            {render_activity_icon(icon)}
            details { class: "disclosure min-w-0 text-sm {tone}",
                summary { class: "flex cursor-pointer select-none items-center gap-2 list-none [&::-webkit-details-marker]:hidden",
                    span { class: "font-medium", "{label}" }
                    {render_disclosure_icon()}
                }
                pre { class: "mt-1.5 max-h-72 overflow-auto whitespace-pre-wrap rounded-lg bg-foreground/[0.04] p-2 font-mono text-[11px] text-muted-foreground ring-1 ring-inset ring-foreground/10", "{content}" }
            }
        }
    }
}

fn status_dot_class(status: &str) -> &'static str {
    match status {
        "streaming" => "bg-amber-400 animate-pulse shadow-[0_0_8px_rgba(251,191,36,0.65)]",
        "installing" => "bg-sky-400 animate-pulse shadow-[0_0_8px_rgba(56,189,248,0.65)]",
        "awaiting" => "bg-violet-400 animate-pulse shadow-[0_0_8px_rgba(167,139,250,0.65)]",
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
.agent-chat-prompt{caret-color:transparent}
.agent-chat-caret{animation:agent-chat-caret-blink 1s step-end infinite}
@keyframes agent-chat-caret-blink{0%,49%{opacity:1}50%,100%{opacity:0}}
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
.chat-md a{color:#6ea8fe;text-decoration:underline}
.chat-md code{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:0.88em;background:rgba(127,127,127,0.18);padding:0.1em 0.35em;border-radius:0.35em}
.chat-md pre{background:rgba(127,127,127,0.14);padding:0.7em 0.9em;border-radius:0.6em;overflow-x:auto;margin:0.6em 0}
.chat-md pre code{background:none;padding:0;font-size:0.85em}
.chat-md blockquote{border-left:2px solid rgba(127,127,127,0.4);padding-left:0.8em;margin:0.5em 0;opacity:0.85}
.chat-md hr{border:0;border-top:1px solid rgba(127,127,127,0.25);margin:0.9em 0}
.chat-md table{border-collapse:collapse;margin:0.5em 0;font-size:0.95em}
.chat-md th,.chat-md td{border:1px solid rgba(127,127,127,0.3);padding:0.3em 0.6em;text-align:left}
"#;
