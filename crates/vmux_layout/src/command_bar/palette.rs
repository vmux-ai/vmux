#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::command_bar::keyboard::{
    CtrlEditAction, CtrlKeyCapture, caret_scroll_left, ctrl_key_capture_for_code,
    ignore_physical_rerouted_ctrl_keydown, utf16_offset_to_byte,
};
use crate::command_bar::results::{
    CommandBarResultItem as ResultItem, active_space_index, agent_page_matches_query,
    agent_page_results, agent_page_url, filter_results, prepend_prompt_agent, space_switch_results,
    start_page_results,
};
use crate::command_bar::style::{
    command_bar_input_class, command_bar_input_row_class, command_bar_input_wrap_class,
    result_content_row_class, result_favicon_class, result_history_url_class, result_item_class,
    result_leading_icon_class, result_list_class, result_primary_text_class,
    result_secondary_text_class, result_shortcut_badge_class, result_terminal_path_class,
    result_trailing_slot_class,
};
use crate::start::event::StartSelectWorkspace;
use dioxus::prelude::*;
use vmux_command::event::{
    CommandBarActionEvent, CommandBarOpenEvent, HISTORY_SUGGESTIONS_RESPONSE_EVENT, HistoryEntry,
    HistorySuggestionsRequest, HistorySuggestionsResponse, PATH_COMPLETE_RESPONSE,
    PathCompleteRequest, PathCompleteResponse, PathEntry, command_bar_should_refocus, is_data_uri,
    is_start_prompt_query, looks_like_url, should_open_typed_query_on_enter,
};
use vmux_command::open_target::OpenTarget;
use vmux_command::prompt_media::{
    CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT, ChatAttachPaths, ChatAttachment,
    ChatAttachments, ChatMediaEntries, ChatMediaEntry, ChatMediaListRequest, ChatPasteMedia,
    ChatPickFiles, inline_media_query, media_display_path, media_reference,
    replace_inline_media_query,
};
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::components::icon::Icon;
use vmux_ui::components::prompt_box::{PromptBox, PromptPopup, PromptPopupPlacement};
use vmux_ui::components::prompt_composer::{
    PROMPT_INPUT_ID, PromptComposer, PromptComposerAttachment, focus_prompt_end,
};
use vmux_ui::components::prompt_media_options::{PromptMediaOption, PromptMediaOptions};
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener};
use vmux_ui::i18n::{TranslationValue, translate, translate_with};
use vmux_ui::icon::PageIconView;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const HOST_SEARCH_DEBOUNCE_MS: i32 = 300;

type HostSearchTimer = Rc<RefCell<Option<(i32, js_sys::Function, Rc<Cell<bool>>)>>>;

fn cancel_host_search(timer: &HostSearchTimer) {
    let Some((id, callback, cancelled)) = timer.borrow_mut().take() else {
        return;
    };
    cancelled.set(true);
    if let Some(window) = web_sys::window() {
        window.clear_timeout_with_handle(id);
    }
    let _ = callback.call0(&JsValue::NULL);
}

fn schedule_host_search(timer: HostSearchTimer, callback: impl FnOnce() + 'static) {
    cancel_host_search(&timer);
    let Some(window) = web_sys::window() else {
        return;
    };
    let cancelled = Rc::new(Cell::new(false));
    let callback_timer = timer.clone();
    let callback_cancelled = cancelled.clone();
    let callback = Closure::once_into_js(move || {
        callback_timer.borrow_mut().take();
        if !callback_cancelled.get() {
            callback();
        }
    })
    .unchecked_into::<js_sys::Function>();
    match window
        .set_timeout_with_callback_and_timeout_and_arguments_0(&callback, HOST_SEARCH_DEBOUNCE_MS)
    {
        Ok(id) => *timer.borrow_mut() = Some((id, callback, cancelled)),
        Err(_) => {
            let _ = callback.call0(&JsValue::NULL);
        }
    }
}

/// Where a [`CommandPalette`] is rendered: the Cmd+K modal or the `vmux://start/` page.
#[derive(Clone, Copy, PartialEq)]
pub enum PaletteVariant {
    /// The Cmd+K command-bar modal overlay.
    Modal,
    /// The `vmux://start/` launcher page.
    Start,
}

#[derive(Clone, PartialEq)]
pub struct StartAgentTransition {
    pub agent_url: String,
    pub prompt: String,
    pub attachments: Vec<vmux_command::prompt_media::ChatAttachment>,
}

/// Props for [`CommandPalette`].
#[derive(Props, Clone, PartialEq)]
pub struct PaletteProps {
    /// Launcher payload (entries + open target); the input resets when its `open_id` changes.
    pub state: ReadSignal<CommandBarOpenEvent>,
    /// Presentation context (placeholder text and host expectations).
    pub variant: PaletteVariant,
    /// Called after an entry executes (the modal hides itself; home is a no-op).
    pub on_close: EventHandler<()>,
    /// Called when the user cancels (Esc / Ctrl-C).
    pub on_dismiss: EventHandler<()>,
    /// Called on query/selection change (the modal re-emits its size).
    pub on_activity: EventHandler<()>,
    #[props(default)]
    pub on_start_agent_transition: Option<EventHandler<StartAgentTransition>>,
}

/// The shared command-bar body: input, live-filtered results, file-path completion,
/// history suggestions, keyboard navigation, and action dispatch. Rendered by both
/// the Cmd+K modal ([`PaletteVariant::Modal`]) and the start launcher ([`PaletteVariant::Start`]).
#[component]
pub fn CommandPalette(props: PaletteProps) -> Element {
    let state = props.state;
    let variant = props.variant;
    let is_start = matches!(variant, PaletteVariant::Start);
    let on_close = props.on_close;
    let on_dismiss = props.on_dismiss;
    let on_activity = props.on_activity;
    let on_start_agent_transition = props.on_start_agent_transition;

    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    let mut nav_mode = use_signal(|| false);
    let mut path_completions = use_signal(Vec::<PathEntry>::new);
    let mut path_request_id = use_signal(|| 0u64);
    let path_search_timer: HostSearchTimer = use_hook(|| Rc::new(RefCell::new(None)));
    let mut history_suggestions = use_signal(Vec::<HistoryEntry>::new);
    let mut suggestions_request_id = use_signal(|| 0u64);
    let suggestions_search_timer: HostSearchTimer = use_hook(|| Rc::new(RefCell::new(None)));
    let mut last_open_id = use_signal(|| u64::MAX);
    let mut last_focus_open_id = use_signal(|| u64::MAX);
    let mut attachments = use_signal(Vec::<ChatAttachment>::new);
    let mut media_entries = use_signal(Vec::<ChatMediaEntry>::new);
    let mut media_request_id = use_signal(|| 0u64);
    let mut media_requested_query = use_signal(|| None::<String>);
    let media_search_timer: HostSearchTimer = use_hook(|| Rc::new(RefCell::new(None)));
    let mut media_loading = use_signal(|| false);
    let mut media_selected = use_signal(|| 0usize);
    let mut start_agent_url = use_signal(String::new);
    let mut start_agent_menu_open = use_signal(|| false);

    let path_search_effect_timer = path_search_timer.clone();
    use_effect(move || {
        let s = state();
        if last_open_id() != s.open_id {
            last_open_id.set(s.open_id);
            query.set(s.url.clone());
            selected.set(if s.space_switch {
                active_space_index(&s.spaces)
            } else {
                0
            });
            nav_mode.set(false);
            path_completions.set(Vec::new());
            history_suggestions.set(Vec::new());
            if is_start {
                attachments.set(Vec::new());
                media_entries.set(Vec::new());
                media_requested_query.set(None);
                media_loading.set(false);
                media_selected.set(0);
            }
        }
    });

    let _path_listener =
        use_bin_event_listener::<PathCompleteResponse, _>(PATH_COMPLETE_RESPONSE, move |data| {
            path_completions.set(data.completions);
        });

    use_effect(move || {
        let q = query();
        let request_id = (*path_request_id.peek()).wrapping_add(1).max(1);
        path_request_id.set(request_id);
        let Some(path_query) = completion_query(&q) else {
            cancel_host_search(&path_search_effect_timer);
            path_completions.set(Vec::new());
            return;
        };
        schedule_host_search(path_search_effect_timer.clone(), move || {
            if *path_request_id.peek() != request_id {
                return;
            }
            let _ = try_cef_bin_emit_rkyv(&PathCompleteRequest { query: path_query });
        });
    });

    let _history_listener = use_bin_event_listener::<HistorySuggestionsResponse, _>(
        HISTORY_SUGGESTIONS_RESPONSE_EVENT,
        move |resp| {
            if resp.request_id != *suggestions_request_id.read() {
                return;
            }
            history_suggestions.set(resp.entries);
        },
    );

    let _attachments_listener =
        use_bin_event_listener::<ChatAttachments, _>(CHAT_ATTACHMENTS_EVENT, move |selected| {
            if !is_start {
                return;
            }
            let mut next = attachments.peek().clone();
            for attachment in &selected.attachments {
                if !next.iter().any(|existing| existing.path == attachment.path) {
                    next.push(attachment.clone());
                }
            }
            attachments.set(next);
            focus_prompt_end(PROMPT_INPUT_ID);
        });

    let _media_entries_listener =
        use_bin_event_listener::<ChatMediaEntries, _>(CHAT_MEDIA_ENTRIES_EVENT, move |response| {
            if !is_start || response.request_id != media_request_id() {
                return;
            }
            media_entries.set(response.entries.clone());
            media_loading.set(false);
            media_selected.set(0);
        });

    let suggestions_search_effect_timer = suggestions_search_timer.clone();
    use_effect(move || {
        if is_start {
            cancel_host_search(&suggestions_search_effect_timer);
            history_suggestions.set(Vec::new());
            return;
        }
        let q = query();
        let trimmed = q.trim();
        let id = (*suggestions_request_id.peek()).wrapping_add(1).max(1);
        suggestions_request_id.set(id);
        if trimmed.is_empty()
            || trimmed.starts_with('>')
            || trimmed.starts_with('/')
            || trimmed.starts_with('~')
            || trimmed.starts_with("vmux://")
            || trimmed.starts_with("file:")
        {
            cancel_host_search(&suggestions_search_effect_timer);
            history_suggestions.set(Vec::new());
            return;
        }
        let query = trimmed.to_string();
        schedule_host_search(suggestions_search_effect_timer.clone(), move || {
            if *suggestions_request_id.peek() != id {
                return;
            }
            let _ = try_cef_bin_emit_rkyv(&HistorySuggestionsRequest {
                query,
                limit: 5,
                request_id: id,
            });
        });
    });

    let media_search_effect_timer = media_search_timer.clone();
    use_effect(move || {
        if !is_start {
            return;
        }
        let value = query();
        let Some(media_query) = inline_media_query(&value).map(|query| query.query.to_string())
        else {
            let request_id = (*media_request_id.peek()).wrapping_add(1).max(1);
            media_request_id.set(request_id);
            cancel_host_search(&media_search_effect_timer);
            media_entries.set(Vec::new());
            media_requested_query.set(None);
            media_loading.set(false);
            media_selected.set(0);
            return;
        };
        if media_requested_query.peek().as_deref() == Some(media_query.as_str()) {
            return;
        }
        let request_id = (*media_request_id.peek()).wrapping_add(1).max(1);
        media_request_id.set(request_id);
        media_requested_query.set(Some(media_query.clone()));
        media_entries.set(Vec::new());
        media_loading.set(true);
        media_selected.set(0);
        schedule_host_search(media_search_effect_timer.clone(), move || {
            if *media_request_id.peek() != request_id
                || media_requested_query.peek().as_deref() != Some(media_query.as_str())
            {
                return;
            }
            if try_cef_bin_emit_rkyv(&ChatMediaListRequest {
                request_id,
                query: media_query,
            })
            .is_err()
            {
                media_loading.set(false);
            }
        });
    });

    use_drop(move || {
        cancel_host_search(&path_search_timer);
        cancel_host_search(&suggestions_search_timer);
        cancel_host_search(&media_search_timer);
    });

    use_effect(move || {
        let open_id = state().open_id;
        if command_bar_should_refocus(last_focus_open_id(), open_id) {
            last_focus_open_id.set(open_id);
            if is_start {
                focus_prompt_end(PROMPT_INPUT_ID);
            } else {
                focus_and_install_ctrl_bindings();
            }
        }
    });

    use_effect(move || {
        let _ = query();
        let _ = selected();
        let _ = nav_mode();
        let _ = path_completions();
        let _ = history_suggestions();
        on_activity.call(());
    });

    let state_val = state();
    let space_name = state_val.space_name.clone();
    let spaces = state_val.spaces.clone();
    let tabs = state_val.tabs.clone();
    let commands = state_val.commands.clone();
    let pages = state_val.pages.clone();
    let work_dirs = state_val.work_dirs.clone();
    let recent_files = state_val.recent_files.clone();
    let search_engines = state_val.search_engines.clone();
    let prompt_context = state_val.prompt_context.clone();
    let open_target = state_val.target;
    let space_switch = state_val.space_switch;
    let is_new_tab = matches!(open_target, Some(OpenTarget::InNewStack));

    let q = query();
    let media_query = is_start.then(|| inline_media_query(&q)).flatten();
    let media_menu_open = media_query.is_some();
    let media_sel = media_selected().min(media_entries.read().len().saturating_sub(1));
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
    let start_prompt_mode = is_start && is_start_prompt_query(&q);
    let start_agent_items = if is_start {
        agent_page_results(&pages, "")
    } else {
        Vec::new()
    };
    let default_agent_item = start_agent_items
        .iter()
        .find(|item| agent_page_url(item) == Some(start_agent_url().as_str()))
        .cloned()
        .or_else(|| start_agent_items.first().cloned());
    let mut results: Vec<ResultItem> = if space_switch {
        space_switch_results(&spaces, &pages, &q)
    } else if is_start && q.trim().is_empty() {
        Vec::new()
    } else if start_prompt_mode {
        start_page_results(&pages, &work_dirs, &recent_files, &search_engines, &q)
    } else {
        let history = history_suggestions();
        let r = filter_results(
            &q,
            &tabs,
            &commands,
            &spaces,
            &pages,
            is_new_tab,
            &history,
            &work_dirs,
            &recent_files,
        );
        let completions = if completion_query(&q).is_some() {
            path_completions()
        } else {
            Vec::new()
        };
        let r = if completions.is_empty() {
            r
        } else {
            let mut combined: Vec<ResultItem> = completions
                .iter()
                .take(8)
                .map(|e| ResultItem::File {
                    path: e.full_path.clone(),
                    is_dir: e.is_dir,
                })
                .collect();
            combined.extend(r);
            combined
        };
        if is_start {
            r.into_iter()
                .filter(|item| {
                    !matches!(
                        item,
                        ResultItem::Stack { url, .. } | ResultItem::Page { url, .. }
                            if url.trim_end_matches('/') == "vmux://start"
                    )
                })
                .collect()
        } else {
            r
        }
    };
    if start_prompt_mode {
        prepend_prompt_agent(&mut results, default_agent_item.as_ref(), &q);
    }
    let sel = selected().min(results.len().saturating_sub(1));
    let active_item = results.get(sel).cloned();
    let nav = nav_mode();
    let selected_agent_accent = default_agent_item
        .as_ref()
        .and_then(agent_page_url)
        .and_then(|url| url.strip_prefix("vmux://agent/"))
        .and_then(|path| path.split('/').next())
        .filter(|agent| !agent.is_empty())
        .map(agent_accent);
    let active_agent_accent = if nav {
        active_item.as_ref()
    } else {
        default_agent_item.as_ref()
    }
    .and_then(agent_page_url)
    .and_then(|url| url.strip_prefix("vmux://agent/"))
    .and_then(|path| path.split('/').next())
    .filter(|agent| !agent.is_empty())
    .map(agent_accent)
    .or(selected_agent_accent);
    let display_text = if nav && !start_prompt_mode {
        match &active_item {
            Some(ResultItem::Command { name, .. }) => format!("> {name}"),
            Some(ResultItem::Navigate { url }) => url.clone(),
            Some(ResultItem::Search { query, .. }) => query.clone(),
            Some(ResultItem::Stack { url, .. }) => url.clone(),
            Some(ResultItem::Space { name, .. }) => name.clone(),
            Some(ResultItem::Page { title, .. }) => title.clone(),
            Some(ResultItem::Terminal { path }) if path.is_empty() => "Terminal".to_string(),
            Some(ResultItem::Terminal { path }) => path.clone(),
            Some(ResultItem::History { title, url, .. }) => {
                if title.is_empty() {
                    url.clone()
                } else {
                    title.clone()
                }
            }
            Some(ResultItem::File { path, .. }) => path.clone(),
            Some(ResultItem::WorkDir { path, .. }) => path.clone(),
            Some(ResultItem::RecentFile { title, url }) => {
                if title.is_empty() {
                    url.clone()
                } else {
                    title.clone()
                }
            }
            None => q.clone(),
        }
    } else {
        q.clone()
    };

    let ghost_text = {
        let q_trimmed = q.trim();
        let completions = if completion_query(&q).is_some() {
            path_completions()
        } else {
            Vec::new()
        };
        if let Some(first) = completions.first() {
            let full = &first.full_path;
            if full.to_lowercase().starts_with(&q_trimmed.to_lowercase())
                && full.is_char_boundary(q_trimmed.len())
            {
                full[q_trimmed.len()..].to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };

    use_effect(move || {
        let s = selected();
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(&format!("command-bar-item-{s}")))
        {
            let opts = web_sys::ScrollIntoViewOptions::new();
            opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
            el.scroll_into_view_with_scroll_into_view_options(&opts);
        }
    });

    use_effect(move || {
        let selected = media_selected();
        let _ = media_entries.read().len();
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| {
                document.get_element_by_id(&format!("prompt-media-item-{selected}"))
            })
        {
            let options = web_sys::ScrollIntoViewOptions::new();
            options.set_block(web_sys::ScrollLogicalPosition::Nearest);
            element.scroll_into_view_with_scroll_into_view_options(&options);
        }
    });

    let execute = move |item: &ResultItem| {
        let prompt = query();
        let transition = if is_start
            && let Some(agent_url) = agent_page_url(item)
            && crate::start::supports_inline_agent_transition(agent_url)
            && let Some(handler) = on_start_agent_transition
        {
            Some((
                handler,
                StartAgentTransition {
                    agent_url: agent_url.to_string(),
                    prompt: prompt.trim().to_string(),
                    attachments: attachments.peek().clone(),
                },
            ))
        } else {
            None
        };
        if matches!(variant, PaletteVariant::Start)
            && (is_start_prompt_query(&prompt) || !attachments.peek().is_empty())
            && let Some(agent_url) = agent_page_url(item)
        {
            on_close.call(());
            let selected_attachments = attachments.peek().clone();
            if agent_page_matches_query(item, &prompt) && selected_attachments.is_empty() {
                emit_action_with_target("open", agent_url, open_target);
            } else {
                emit_prompt_action(prompt.trim(), open_target, agent_url, &selected_attachments);
            }
            if let Some((handler, next)) = transition {
                handler.call(next);
            }
            return;
        }
        on_close.call(());
        match item {
            ResultItem::Terminal { path } => {
                emit_action("terminal", path);
            }
            ResultItem::Stack {
                pane_id, tab_index, ..
            } => {
                emit_action("switch_tab", &format!("{pane_id}:{tab_index}"));
            }
            ResultItem::Command { id, .. } => {
                emit_action("command", id);
            }
            ResultItem::Space { id, .. } => {
                emit_action("space", id);
            }
            ResultItem::Page { url, .. } => {
                if !url.is_empty() {
                    emit_action_with_target("open", url, open_target);
                }
            }
            ResultItem::Navigate { url } => {
                if !url.is_empty() {
                    emit_action_with_target("open", url, open_target);
                }
            }
            ResultItem::Search { engine, query } => {
                emit_action_with_target("open", &engine.search_url(query), open_target);
            }
            ResultItem::History { url, .. } => {
                if !url.is_empty() {
                    emit_action_with_target("open", url, open_target);
                }
            }
            ResultItem::File { path, .. } => {
                emit_action_with_target("open", &format!("file://{path}"), open_target);
            }
            ResultItem::WorkDir { path, .. } => {
                emit_action_with_target("open", &format!("file://{path}"), open_target);
            }
            ResultItem::RecentFile { url, .. } => {
                emit_action_with_target("open", url, open_target);
            }
        }
        if let Some((handler, next)) = transition {
            handler.call(next);
        }
    };

    let placeholder = if space_switch {
        translate("command-switch-space")
    } else {
        match variant {
            PaletteVariant::Start => translate("command-search-ask"),
            PaletteVariant::Modal => {
                if is_new_tab {
                    translate("command-new-tab-placeholder")
                } else {
                    translate("command-placeholder")
                }
            }
        }
    };
    let start_accent = active_agent_accent.unwrap_or_else(|| agent_accent("vibe"));
    let start_prompt_attachments = attachments
        .read()
        .iter()
        .enumerate()
        .map(|(index, attachment)| PromptComposerAttachment {
            key: format!("start-attachment-{}", attachment.path),
            name: attachment.name.clone(),
            label: file_extension_label(&attachment.name),
            preview_data_url: attachment.preview_data_url.clone(),
            remove_index: Some(index),
        })
        .collect::<Vec<_>>();
    let start_action_enabled = !q.trim().is_empty() || !attachments.read().is_empty();
    let selected_agent_title = default_agent_item
        .as_ref()
        .and_then(|item| match item {
            ResultItem::Page { title, .. } => Some(title.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "Agent".to_string());
    let selected_agent_url_value = default_agent_item
        .as_ref()
        .and_then(agent_page_url)
        .unwrap_or_default()
        .to_string();
    let workspace_label = if prompt_context.workspace_name.is_empty() {
        "Select workspace".to_string()
    } else {
        prompt_context.workspace_name.clone()
    };
    let branch_title = if prompt_context.branch.is_empty() {
        "Git repository".to_string()
    } else {
        format!("Branch {}", prompt_context.branch)
    };
    let worktree_title = if prompt_context.base_ref.is_empty() {
        "Linked worktree".to_string()
    } else {
        format!("Worktree from {}", prompt_context.base_ref)
    };
    let start_composer_footer = rsx! {
        div { class: "flex min-w-0 items-center justify-between gap-1",
            div { class: "flex min-w-0 flex-1 items-center gap-1 overflow-x-auto",
                button {
                    class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] font-medium text-foreground/70 transition hover:bg-foreground/[0.08] hover:text-foreground",
                    title: "Choose agent",
                    onmousedown: move |event| event.prevent_default(),
                    onclick: move |_| {
                        start_agent_menu_open.set(!start_agent_menu_open());
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
                    span { class: "truncate", "{selected_agent_title}" }
                    svg {
                        class: "h-3 w-3 shrink-0 opacity-50",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path { d: "m8 10 4 4 4-4" }
                    }
                }
                span {
                    class: "flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground",
                    title: "Tools ask before protected actions",
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
                    "Ask"
                }
                button {
                        class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] text-muted-foreground transition hover:bg-foreground/[0.08] hover:text-foreground",
                        title: if prompt_context.cwd.is_empty() { "Create or select workspace" } else { "{prompt_context.cwd}" },
                        onmousedown: move |event| event.prevent_default(),
                        onclick: move |_| {
                            let _ = try_cef_bin_emit_rkyv(&StartSelectWorkspace {
                                current_dir: prompt_context.cwd.clone(),
                            });
                            focus_prompt_end(PROMPT_INPUT_ID);
                        },
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
                if prompt_context.is_git_repo {
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
                        span { class: "truncate", if prompt_context.branch.is_empty() { "Git" } else { "{prompt_context.branch}" } }
                    }
                    if prompt_context.is_worktree {
                        span {
                            class: "flex h-7 shrink-0 items-center gap-1 rounded-lg bg-violet-500/[0.08] px-2 text-[10px] font-medium text-violet-600 ring-1 ring-inset ring-violet-500/15 dark:text-violet-300",
                            title: "{worktree_title}",
                            "Worktree"
                        }
                    }
                    if prompt_context.uncommitted > 0 {
                        span { class: "shrink-0 font-mono text-[10px] text-amber-500", title: "Uncommitted changes", "● {prompt_context.uncommitted}" }
                    }
                    if prompt_context.ahead > 0 {
                        span { class: "shrink-0 font-mono text-[10px] text-sky-500", title: "Commits ahead of upstream", "↑{prompt_context.ahead}" }
                    }
                } else if !prompt_context.cwd.is_empty() {
                    span { class: "h-7 shrink-0 content-center rounded-lg px-2 text-[10px] text-muted-foreground/70", "No Git" }
                }
            }
            span { class: "flex h-7 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[10px] text-muted-foreground",
                span { class: "h-1.5 w-1.5 rounded-full bg-emerald-500" }
                "Ready"
            }
        }
    };
    let start_keydown_q = q.clone();
    let start_keydown_results = results.clone();
    let start_keydown_default_agent = default_agent_item.clone();
    let start_keydown_nav = nav;
    let start_keydown_ghost = ghost_text.clone();
    let start_keydown = move |e: KeyboardEvent| {
        if e.key() == Key::Tab {
            e.prevent_default();
            if !start_keydown_ghost.is_empty() {
                query.set(format!("{}{}", start_keydown_q, start_keydown_ghost));
                selected.set(0);
                focus_prompt_end(PROMPT_INPUT_ID);
            }
            return;
        }

        let ctrl = e.modifiers().contains(Modifiers::CONTROL);
        if space_switch
            && !ctrl
            && start_keydown_q.trim().is_empty()
            && let Key::Character(s) = e.key()
            && let Some(idx) = s
                .chars()
                .next()
                .filter(|c| c.is_ascii_digit())
                .and_then(|c| c.to_digit(10))
        {
            let space_count = start_keydown_results
                .iter()
                .filter(|result| matches!(result, ResultItem::Space { .. }))
                .count();
            if (idx as usize) < space_count {
                e.prevent_default();
                selected.set(idx as usize);
                nav_mode.set(true);
                return;
            }
        }
        let go_down = (e.key() == Key::ArrowDown && !ctrl)
            || (ctrl && matches!(e.code(), Code::KeyN | Code::KeyJ));
        let go_up = (e.key() == Key::ArrowUp && !ctrl)
            || (ctrl && matches!(e.code(), Code::KeyP | Code::KeyK));

        if media_menu_open {
            if go_down {
                e.prevent_default();
                let max = media_entries.read().len().saturating_sub(1);
                media_selected.set((media_sel + 1).min(max));
                return;
            }
            if go_up {
                e.prevent_default();
                media_selected.set(media_sel.saturating_sub(1));
                return;
            }
            if e.key() == Key::Enter && !e.modifiers().shift() {
                e.prevent_default();
                if let Some(entry) = media_entries.read().get(media_sel).cloned() {
                    select_start_media_entry(&entry, query, media_selected);
                }
                return;
            }
            if e.key() == Key::Escape {
                e.prevent_default();
                if let Some(media_query) = inline_media_query(&start_keydown_q) {
                    query.set(replace_inline_media_query(
                        &start_keydown_q,
                        media_query,
                        "",
                    ));
                }
                media_selected.set(0);
                return;
            }
        }

        if go_down {
            e.prevent_default();
            let max = start_keydown_results.len().saturating_sub(1);
            selected.set((sel + 1).min(max));
            nav_mode.set(true);
        } else if go_up {
            e.prevent_default();
            selected.set(sel.saturating_sub(1));
            nav_mode.set(true);
        } else if e.key() == Key::Escape || (ctrl && e.code() == Code::KeyC) {
            on_dismiss.call(());
        } else if e.key() == Key::Enter && !e.modifiers().shift() {
            e.prevent_default();
            if start_keydown_q.trim().is_empty() && !attachments.peek().is_empty() {
                if let Some(item) = start_keydown_default_agent.as_ref() {
                    execute(item);
                } else {
                    let selected_attachments = attachments.peek().clone();
                    emit_prompt_action("", open_target, "", &selected_attachments);
                }
                return;
            }
            if space_switch {
                if let Some(item) = start_keydown_results.get(sel) {
                    execute(item);
                }
            } else if start_prompt_mode {
                if let Some(item) = start_keydown_results.get(sel).filter(|item| {
                    start_keydown_nav
                        || agent_page_matches_query(item, &start_keydown_q)
                        || (matches!(item, ResultItem::Terminal { .. })
                            && start_keydown_q.trim().eq_ignore_ascii_case("terminal"))
                }) {
                    execute(item);
                } else if let Some(item) = start_keydown_default_agent.as_ref() {
                    execute(item);
                } else {
                    on_close.call(());
                    let selected_attachments = attachments.peek().clone();
                    emit_prompt_action(
                        start_keydown_q.trim(),
                        open_target,
                        "",
                        &selected_attachments,
                    );
                }
            } else {
                let prefer_page = matches!(
                    start_keydown_results.get(sel),
                    Some(ResultItem::Page { url, .. })
                        if start_keydown_q.trim().starts_with("vmux://")
                            && url.starts_with(start_keydown_q.trim())
                );
                if !prefer_page
                    && should_open_typed_query_on_enter(open_target, nav_mode(), &start_keydown_q)
                {
                    on_close.call(());
                    emit_action_with_target("open", &start_keydown_q, open_target);
                } else if let Some(item) = start_keydown_results.get(sel) {
                    execute(item);
                } else if !start_keydown_q.is_empty() {
                    emit_action_with_target("open", &start_keydown_q, open_target);
                }
            }
        }
    };
    let modal_keydown_q = q.clone();
    let modal_keydown_results = results.clone();
    let modal_keydown_ghost = ghost_text.clone();
    let modal_keydown = move |e: KeyboardEvent| {
        if e.key() == Key::Tab {
            e.prevent_default();
            if !modal_keydown_ghost.is_empty() {
                let new_value = format!("{}{}", modal_keydown_q, modal_keydown_ghost);
                query.set(new_value.clone());
                selected.set(0);
                if let Some(element) = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.get_element_by_id("command-bar-input"))
                {
                    let input: web_sys::HtmlInputElement = element.unchecked_into();
                    input.set_value(&new_value);
                    let len = new_value.len() as u32;
                    let _ = input.set_selection_range(len, len);
                    ensure_caret_visible(&input, new_value.len());
                }
            }
            return;
        }

        let ctrl = e.modifiers().contains(Modifiers::CONTROL);
        let vmux_synthetic = is_vmux_synthetic_dioxus_keydown(&e);
        if ctrl && ignore_physical_rerouted_ctrl_keydown(&e.code().to_string(), vmux_synthetic) {
            e.prevent_default();
            return;
        }
        if space_switch
            && !ctrl
            && modal_keydown_q.trim().is_empty()
            && let Key::Character(s) = e.key()
            && let Some(index) = s
                .chars()
                .next()
                .filter(|character| character.is_ascii_digit())
                .and_then(|character| character.to_digit(10))
        {
            let space_count = modal_keydown_results
                .iter()
                .filter(|result| matches!(result, ResultItem::Space { .. }))
                .count();
            if (index as usize) < space_count {
                e.prevent_default();
                selected.set(index as usize);
                nav_mode.set(true);
                return;
            }
        }
        let go_down = (e.key() == Key::ArrowDown && !ctrl)
            || (ctrl && matches!(e.code(), Code::KeyN | Code::KeyJ));
        let go_up = (e.key() == Key::ArrowUp && !ctrl)
            || (ctrl && matches!(e.code(), Code::KeyP | Code::KeyK));
        if go_down {
            e.prevent_default();
            let max = modal_keydown_results.len().saturating_sub(1);
            selected.set((sel + 1).min(max));
            nav_mode.set(true);
        } else if go_up {
            e.prevent_default();
            selected.set(sel.saturating_sub(1));
            nav_mode.set(true);
        } else if e.key() == Key::Escape || (ctrl && e.code() == Code::KeyC) {
            on_dismiss.call(());
        } else if e.key() == Key::Enter {
            if space_switch {
                if let Some(item) = modal_keydown_results.get(sel) {
                    execute(item);
                }
            } else {
                let prefer_page = matches!(
                    modal_keydown_results.get(sel),
                    Some(ResultItem::Page { url, .. })
                        if modal_keydown_q.trim().starts_with("vmux://")
                            && url.starts_with(modal_keydown_q.trim())
                );
                if !prefer_page
                    && should_open_typed_query_on_enter(open_target, nav_mode(), &modal_keydown_q)
                {
                    on_close.call(());
                    emit_action_with_target("open", &modal_keydown_q, open_target);
                } else if let Some(item) = modal_keydown_results.get(sel) {
                    execute(item);
                } else if !modal_keydown_q.is_empty() {
                    emit_action_with_target("open", &modal_keydown_q, open_target);
                }
            }
        }
    };

    rsx! {
        div { class: "relative",
            if is_start {
                if let Some(accent) = active_agent_accent {
                    div { class: "{accent.glow_top} transition-all duration-500 ease-out" }
                    div { class: "{accent.glow_bottom} transition-all duration-500 ease-out" }
                }
            }
            if is_start {
                if start_agent_menu_open() {
                    PromptPopup {
                        placement: PromptPopupPlacement::Upward,
                        id: "start-agent-selector",
                        div { class: "p-1.5",
                            div { class: "px-2 pb-1 pt-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60", "Agent" }
                            for item in start_agent_items.iter() {
                                if let ResultItem::Page { url, title, .. } = item {
                                    {
                                        let option_url = url.clone();
                                        let option_selected = url == &selected_agent_url_value;
                                        rsx! {
                                            button {
                                                key: "{url}",
                                                class: if option_selected { "flex w-full items-center gap-2 rounded-xl bg-foreground/[0.08] px-2.5 py-2 text-left text-sm text-foreground" } else { "flex w-full items-center gap-2 rounded-xl px-2.5 py-2 text-left text-sm text-foreground/75 transition hover:bg-foreground/[0.06] hover:text-foreground" },
                                                onmousedown: move |event| event.prevent_default(),
                                                onclick: move |_| {
                                                    start_agent_url.set(option_url.clone());
                                                    start_agent_menu_open.set(false);
                                                    selected.set(0);
                                                    nav_mode.set(false);
                                                    focus_prompt_end(PROMPT_INPUT_ID);
                                                },
                                                span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.07] text-[10px] font-semibold uppercase", "{title.chars().next().unwrap_or('A')}" }
                                                span { class: "min-w-0 flex-1 truncate", "{title}" }
                                                if option_selected {
                                                    svg {
                                                        class: "h-3.5 w-3.5 shrink-0 text-emerald-500",
                                                        view_box: "0 0 24 24",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        stroke_width: "2.2",
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        path { d: "m5 12 4 4L19 6" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                PromptComposer {
                    value: display_text.clone(),
                    completion: ghost_text.clone(),
                    attachments: start_prompt_attachments,
                    show_examples: q.is_empty() && ghost_text.is_empty(),
                    placeholder: translate("command-composer-placeholder"),
                    accent_bg: start_accent.accent_bg.to_string(),
                    accent_color: format!("rgb({})", start_accent.rain_rgb),
                    accent_gradient: start_accent.grad.to_string(),
                    footer: Some(start_composer_footer),
                    action_title: translate("command-send"),
                    action_enabled: start_action_enabled,
                    on_input: move |value| {
                        start_agent_menu_open.set(false);
                        query.set(value);
                        selected.set(0);
                        nav_mode.set(false);
                    },
                    on_keydown: start_keydown,
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
                    on_action: {
                        let action_results = results.clone();
                        let action_query = q.clone();
                        let action_default_agent = default_agent_item.clone();
                        let action_nav = nav;
                        move |_| {
                            if let Some(item) = action_results.get(sel).filter(|item| {
                                !start_prompt_mode
                                    || action_nav
                                    || agent_page_matches_query(item, &action_query)
                                    || (matches!(item, ResultItem::Terminal { .. })
                                        && action_query.trim().eq_ignore_ascii_case("terminal"))
                            }) {
                                execute(item);
                            } else if !action_query.trim().is_empty()
                                || !attachments.peek().is_empty()
                            {
                                if let Some(item) = action_default_agent.as_ref() {
                                    execute(item);
                                } else {
                                    on_close.call(());
                                    let selected_attachments = attachments.peek().clone();
                                    emit_prompt_action(
                                        action_query.trim(),
                                        open_target,
                                        "",
                                        &selected_attachments,
                                    );
                                }
                            }
                        }
                    },
                }
            } else {
                PromptBox {
                    glass: false,
                    class: "p-2",
                    div { class: command_bar_input_row_class(),
                        if !space_name.is_empty() {
                            span {
                                title: "{space_name}",
                                class: "max-w-36 shrink-0 truncate rounded-md bg-glass-hover px-2 py-1 text-ui-xs font-medium text-muted-foreground",
                                "{space_name}"
                            }
                        }
                        {
                            let icon_class = "h-4 w-4 shrink-0 text-muted-foreground";
                            let (is_command, is_path, is_url) = if nav {
                                match &active_item {
                                    Some(ResultItem::Command { .. }) => (true, false, false),
                                    Some(ResultItem::Terminal { path }) if path.is_empty() => (true, false, false),
                                    Some(ResultItem::Terminal { .. }) => (false, true, false),
                                    Some(ResultItem::Stack { .. }) => (false, false, true),
                                    Some(ResultItem::Space { .. }) => (false, false, false),
                                    Some(ResultItem::Page { .. }) => (false, false, false),
                                    Some(ResultItem::Navigate { url }) => {
                                        let is_url = url.contains("://")
                                            || (url.contains('.') && !url.contains(' '));
                                        (false, false, is_url)
                                    }
                                    Some(ResultItem::Search { .. }) => (false, false, false),
                                    Some(ResultItem::History { .. }) => (false, false, true),
                                    Some(ResultItem::File { .. }) => (false, true, false),
                                    Some(ResultItem::WorkDir { .. }) => (false, true, false),
                                    Some(ResultItem::RecentFile { .. }) => (false, true, false),
                                    None => (false, false, false),
                                }
                            } else {
                                let trimmed = q.trim();
                                let command = trimmed.starts_with('>');
                                let path = !command
                                    && (trimmed.starts_with('/') || trimmed.starts_with('~'));
                                let url = !command
                                    && !path
                                    && (trimmed.contains("://")
                                        || (trimmed.contains('.') && !trimmed.contains(' ')));
                                (command, path, url)
                            };
                            if is_command {
                                rsx! { span { class: "select-none font-mono text-base text-muted-foreground", ">_" } }
                            } else if is_path {
                                rsx! { Icon { class: icon_class,
                                    path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                    path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
                                } }
                            } else if is_url {
                                rsx! { Icon { class: icon_class,
                                    path { d: "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z" }
                                    path { d: "M2 12h20" }
                                    path { d: "M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z" }
                                } }
                            } else {
                                rsx! { Icon { class: icon_class,
                                    circle { cx: "11", cy: "11", r: "8" }
                                    path { d: "m21 21-4.3-4.3" }
                                } }
                            }
                        }
                        div { class: command_bar_input_wrap_class(),
                            if !ghost_text.is_empty() {
                                div {
                                    class: "pointer-events-none absolute inset-0 flex items-center",
                                    span { class: "invisible text-base", "{q}" }
                                    span { class: "text-base text-muted-foreground/40", "{ghost_text}" }
                                }
                            }
                            input {
                                id: "command-bar-input",
                                r#type: "text",
                                "data-ghost": "{ghost_text}",
                                class: command_bar_input_class(),
                                placeholder,
                                value: "{display_text}",
                                autofocus: true,
                                oninput: move |event| {
                                    query.set(event.value());
                                    selected.set(0);
                                    nav_mode.set(false);
                                },
                                onkeydown: modal_keydown,
                            }
                        }
                        button {
                            r#type: "button",
                            aria_label: "Bookmark this page",
                            title: "Bookmark this page (⌘D)",
                            class: "flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
                            onmousedown: move |event| {
                                event.prevent_default();
                                event.stop_propagation();
                            },
                            onclick: move |event| {
                                event.prevent_default();
                                event.stop_propagation();
                                let _ = try_cef_bin_emit_rkyv(&crate::event::BookmarksCommandEvent {
                                    command: "toggle_active".into(),
                                    uuid: None,
                                    name: None,
                                    url: None,
                                    metadata: None,
                                    folder: None,
                                });
                            },
                            Icon { class: "h-4 w-4",
                                path { d: "M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" }
                            }
                        }
                    }
                }
            }
            if !start_agent_menu_open() && media_menu_open {
                PromptPopup {
                    placement: PromptPopupPlacement::Downward,
                    id: "command-bar-results",
                    PromptMediaOptions {
                        items: prompt_media_options,
                        selected: media_sel,
                        loading: media_loading(),
                        on_hover: move |index| media_selected.set(index),
                        on_select: move |index| {
                            if let Some(entry) = media_entries.peek().get(index).cloned() {
                                select_start_media_entry(&entry, query, media_selected);
                            }
                        },
                    }
                }
            }
            if !start_agent_menu_open() && !media_menu_open && !results.is_empty() {
                PromptPopup {
                    placement: if is_start { PromptPopupPlacement::Downward } else { PromptPopupPlacement::Inline },
                    id: "command-bar-results",
                    class: if is_start { "" } else { result_list_class() },
                for (i, item) in results.iter().enumerate() {
                    div {
                        key: "{i}",
                        id: "command-bar-item-{i}",
                        class: result_item_class(i == sel),
                        onclick: {
                            let item = item.clone();
                            move |_| { execute(&item); }
                        },
                        onmouseenter: move |_| {
                            if is_start {
                                selected.set(i);
                            }
                        },
                        match item {
                            ResultItem::Terminal { path } => rsx! {
                                div { class: result_content_row_class(),
                                    span { class: "shrink-0 text-sm text-muted-foreground", ">_" }
                                    if path.is_empty() {
                                        span { class: "text-sm text-foreground", {translate("command-terminal")} }
                                    } else {
                                        span { class: "shrink-0 text-sm text-foreground", {translate("command-open-terminal")} }
                                        span { class: result_terminal_path_class(), "{path}" }
                                    }
                                }
                                span { class: result_trailing_slot_class() }
                            },
                            ResultItem::Stack { title, url, icon, location, .. } => rsx! {
                                div { class: result_content_row_class(),
                                    PageIconView {
                                        icon: icon.clone(),
                                        url: url.clone(),
                                        img_class: result_favicon_class().to_string(),
                                        icon_class: result_leading_icon_class().to_string(),
                                    }
                                    div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                        span { class: result_primary_text_class(), "{title}" }
                                        span { class: result_secondary_text_class(), "{url}" }
                                    }
                                }
                                span { class: result_trailing_slot_class(),
                                    if location.is_empty() { {translate("command-stack")} } else { "{location}" }
                                }
                            },
                            ResultItem::Space { name, profile, is_active, tab_count, .. } => rsx! {
                                if space_switch {
                                    span { class: "w-5 shrink-0 text-center font-mono text-xs text-muted-foreground", "{i}" }
                                }
                                div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                    div { class: "flex min-w-0 items-center gap-2",
                                        span { class: result_primary_text_class(), "{name}" }
                                        if *is_active {
                                            span { class: "rounded-full bg-blue-500/15 px-2 py-0.5 text-xs text-blue-300", {translate("common-active")} }
                                        }
                                    }
                                    span { class: result_secondary_text_class(), "{profile}" }
                                }
                                span { class: result_trailing_slot_class(), {translate_with("command-tabs", &[("count", TranslationValue::Number(*tab_count as i64))])} }
                            },
                            ResultItem::Command { name, shortcut, .. } => rsx! {
                                div { class: result_content_row_class(),
                                    span { class: "shrink-0 text-sm text-muted-foreground", ">_" }
                                    span { class: result_primary_text_class(), "{name}" }
                                }
                                span { class: result_trailing_slot_class(),
                                    if !shortcut.is_empty() {
                                        span { class: result_shortcut_badge_class(), "{shortcut}" }
                                    }
                                }
                            },
                            ResultItem::History { url, title, favicon_url, .. } => rsx! {
                                div { class: result_content_row_class(),
                                    Favicon {
                                        favicon_url: favicon_url.clone(),
                                        url: url.clone(),
                                        class: result_favicon_class().to_string(),
                                        globe_class: result_leading_icon_class().to_string(),
                                    }
                                    span { class: "min-w-0 flex-1 truncate text-sm text-foreground",
                                        if title.is_empty() { "{url}" } else { "{title}" }
                                    }
                                    span { class: result_history_url_class(), "{url}" }
                                }
                                span { class: result_trailing_slot_class() }
                            },
                            ResultItem::Page { url, title, icon, shortcut } => rsx! {
                                div { class: result_content_row_class(),
                                    PageIconView {
                                        icon: icon.clone(),
                                        url: url.clone(),
                                        img_class: result_favicon_class().to_string(),
                                        icon_class: result_leading_icon_class().to_string(),
                                    }
                                    div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                        if start_prompt_mode
                                            && agent_page_url(item).is_some()
                                            && !agent_page_matches_query(item, &q)
                                        {
                                            span { class: result_primary_text_class(), "Ask {title}" }
                                        } else {
                                            span { class: result_primary_text_class(), "{title}" }
                                            span { class: result_secondary_text_class(), "{url}" }
                                        }
                                    }
                                }
                                span { class: result_trailing_slot_class(),
                                    if start_prompt_mode
                                        && agent_page_url(item).is_some()
                                        && !agent_page_matches_query(item, &q)
                                    {
                                        {translate("command-prompt")}
                                    } else if shortcut.is_empty() {
                                        {translate("command-new-tab")}
                                    } else {
                                        span { class: result_shortcut_badge_class(), "{shortcut}" }
                                    }
                                }
                            },
                            ResultItem::Navigate { url } => rsx! {
                                div { class: result_content_row_class(),
                                    Icon { class: result_leading_icon_class(),
                                        circle { cx: "11", cy: "11", r: "8" }
                                        path { d: "m21 21-4.3-4.3" }
                                    }
                                    if url.is_empty() {
                                        span { class: "text-sm text-foreground", {translate("command-search")} }
                                    } else if looks_like_url(url) {
                                        span { class: result_primary_text_class(), {translate_with("command-open-value", &[("value", TranslationValue::String(url))])} }
                                    } else {
                                        span { class: result_primary_text_class(), {translate_with("command-search-value", &[("value", TranslationValue::String(url))])} }
                                    }
                                }
                                if !url.is_empty() {
                                    span { class: result_trailing_slot_class(), "\u{21b5}" }
                                } else {
                                    span { class: result_trailing_slot_class() }
                                }
                            },
                            ResultItem::Search { engine, query } => rsx! {
                                div { class: result_content_row_class(),
                                    Favicon {
                                        favicon_url: String::new(),
                                        url: engine.search_url(query),
                                        class: result_favicon_class().to_string(),
                                        globe_class: result_leading_icon_class().to_string(),
                                    }
                                    div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                        span { class: result_primary_text_class(), "Search with {engine.name()}" }
                                        span { class: result_secondary_text_class(), "{query}" }
                                    }
                                }
                                span { class: result_trailing_slot_class(), "\u{21b5}" }
                            },
                            ResultItem::File { path, is_dir } => {
                                let name = path
                                    .trim_end_matches('/')
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or(path.as_str())
                                    .to_string();
                                rsx! {
                                    div { class: result_content_row_class(),
                                        if *is_dir {
                                            Icon { class: result_leading_icon_class(),
                                                path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                            }
                                        } else {
                                            Icon { class: result_leading_icon_class(),
                                                path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                                path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
                                            }
                                        }
                                        div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                            span { class: result_primary_text_class(), "{name}" }
                                            span { class: result_secondary_text_class(), "{path}" }
                                        }
                                    }
                                    if *is_dir {
                                        span { class: result_trailing_slot_class() }
                                    } else {
                                        span { class: result_trailing_slot_class(), "\u{21b5}" }
                                    }
                                }
                            },
                            ResultItem::WorkDir { path, is_dir } => {
                                let name = path
                                    .trim_end_matches('/')
                                    .rsplit('/')
                                    .next()
                                    .unwrap_or(path.as_str())
                                    .to_string();
                                rsx! {
                                    div { class: result_content_row_class(),
                                        if *is_dir {
                                            Icon { class: result_leading_icon_class(),
                                                path { d: "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13c0 1.1.9 2 2 2Z" }
                                            }
                                        } else {
                                            Icon { class: result_leading_icon_class(),
                                                path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                                path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
                                            }
                                        }
                                        div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                            span { class: result_primary_text_class(), "{name}" }
                                            span { class: result_secondary_text_class(), "{path}" }
                                        }
                                    }
                                    if *is_dir {
                                        span { class: result_trailing_slot_class() }
                                    } else {
                                        span { class: result_trailing_slot_class(), "\u{21b5}" }
                                    }
                                }
                            },
                            ResultItem::RecentFile { url, title } => {
                                let display = url.strip_prefix("file://").unwrap_or(url.as_str()).to_string();
                                let name = if title.is_empty() {
                                    display
                                        .trim_end_matches('/')
                                        .rsplit('/')
                                        .next()
                                        .unwrap_or(display.as_str())
                                        .to_string()
                                } else {
                                    title.clone()
                                };
                                rsx! {
                                    div { class: result_content_row_class(),
                                        Icon { class: result_leading_icon_class(),
                                            path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
                                            path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
                                        }
                                        div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                            span { class: result_primary_text_class(), "{name}" }
                                            span { class: result_secondary_text_class(), "{display}" }
                                        }
                                    }
                                    span { class: result_trailing_slot_class(), "\u{21b5}" }
                                }
                            },
                        }
                    }
                }
                }
            }
        }
    }
}

fn looks_like_path(s: &str) -> bool {
    if is_data_uri(s) {
        return false;
    }
    s.starts_with('/')
        || s.starts_with("~/")
        || s.starts_with("./")
        || s.starts_with("../")
        || s.contains('/') && !s.contains(' ') && !s.contains("://")
}

/// The filesystem query to complete from the command-bar input, if any.
/// `file://…` completes the path after the scheme (empty → local dir); bare paths
/// (`/…`, `~/…`, `./…`) complete as typed.
fn completion_query(input: &str) -> Option<String> {
    let t = input.trim();
    if let Some(rest) = t.strip_prefix("file://") {
        Some(rest.to_string())
    } else if looks_like_path(t) {
        Some(t.to_string())
    } else {
        None
    }
}

fn file_extension_label(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_uppercase())
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "FILE".to_string())
}

fn select_start_media_entry(
    entry: &ChatMediaEntry,
    mut query: Signal<String>,
    mut selected: Signal<usize>,
) {
    let value = query.peek().clone();
    let Some(media_query) = inline_media_query(&value) else {
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
    query.set(replace_inline_media_query(
        &value,
        media_query,
        &replacement,
    ));
    selected.set(0);
    focus_prompt_end(PROMPT_INPUT_ID);
}

/// Emit a command-bar action to the host with no explicit open target.
pub(crate) fn emit_action(action: &str, value: &str) {
    emit_action_with_target(action, value, None);
}

/// Emit a [`CommandBarActionEvent`] to the host (open / command / space / terminal / switch_tab).
pub(crate) fn emit_action_with_target(action: &str, value: &str, target: Option<OpenTarget>) {
    let _ = try_cef_bin_emit_rkyv(&CommandBarActionEvent {
        action: action.to_string(),
        value: value.to_string(),
        target,
        agent_url: None,
        attachments: Vec::new(),
    });
}

fn emit_prompt_action(
    value: &str,
    target: Option<OpenTarget>,
    agent_url: &str,
    attachments: &[ChatAttachment],
) {
    let _ = try_cef_bin_emit_rkyv(&CommandBarActionEvent {
        action: "prompt".to_string(),
        value: value.to_string(),
        target,
        agent_url: (!agent_url.is_empty()).then(|| agent_url.to_string()),
        attachments: attachments
            .iter()
            .map(
                |attachment| vmux_command::prompt_media::ChatSubmitAttachment {
                    path: attachment.path.clone(),
                    name: attachment.name.clone(),
                    mime_type: attachment.mime_type.clone(),
                    size: attachment.size,
                },
            )
            .collect(),
    });
}

fn focus_and_install_ctrl_bindings() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id("command-bar-input") else {
        return;
    };
    let input: web_sys::HtmlInputElement = el.unchecked_into();
    input.focus().ok();
    select_all_on_open(&input);

    if js_sys::Reflect::get(&input, &JsValue::from_str("_ctrlBound"))
        .map(|v| v.is_truthy())
        .unwrap_or(false)
    {
        return;
    }
    let _ = js_sys::Reflect::set(&input, &JsValue::from_str("_ctrlBound"), &JsValue::TRUE);

    let input2 = input.clone();
    let closure = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
        if handle_plain_meta_a(&e, &input2) {
            return;
        }
        if !e.ctrl_key() {
            return;
        }
        if is_vmux_synthetic_keydown(&e) {
            return;
        }
        let code = e.code();
        let action = match ctrl_key_capture_for_code(&code) {
            CtrlKeyCapture::Ignore => return,
            CtrlKeyCapture::PassToDioxus => {
                e.prevent_default();
                return;
            }
            CtrlKeyCapture::RerouteToDioxus => {
                e.prevent_default();
                e.stop_immediate_propagation();
                dispatch_ctrl_keydown(&input2, &code);
                return;
            }
            CtrlKeyCapture::Edit(action) => action,
        };
        e.prevent_default();
        e.stop_immediate_propagation();

        match action {
            CtrlEditAction::Home => {
                let _ = input2.set_selection_range(0, 0);
                ensure_caret_visible(&input2, 0);
            }
            CtrlEditAction::End => {
                let ghost = input2.get_attribute("data-ghost").unwrap_or_default();
                if !ghost.is_empty() {
                    let new_val = format!("{}{}", input2.value(), ghost);
                    input2.set_value(&new_val);
                    let len = new_val.len() as u32;
                    let _ = input2.set_selection_range(len, len);
                    dispatch_input_event(&input2);
                    ensure_caret_visible(&input2, new_val.len());
                } else {
                    let len = input2.value().len();
                    let _ = input2.set_selection_range(len as u32, len as u32);
                    ensure_caret_visible(&input2, len);
                }
            }
            CtrlEditAction::Forward => {
                let value = input2.value();
                let max = value.encode_utf16().count() as u32;
                let p = (input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) + 1).min(max);
                let _ = input2.set_selection_range(p, p);
                ensure_caret_visible(&input2, utf16_offset_to_byte(&value, p));
            }
            CtrlEditAction::Back => {
                let value = input2.value();
                let p = input2
                    .selection_start()
                    .unwrap_or(Some(0))
                    .unwrap_or(0)
                    .saturating_sub(1);
                let _ = input2.set_selection_range(p, p);
                ensure_caret_visible(&input2, utf16_offset_to_byte(&value, p));
            }
            CtrlEditAction::Delete => {
                let v = input2.value();
                let s = floor_char_boundary(&v, raw_selection_start(&input2));
                let end = v[s..].chars().next().map(|c| s + c.len_utf8()).unwrap_or(s);
                let new_val = format!("{}{}", &v[..s], &v[end..]);
                input2.set_value(&new_val);
                let _ = input2.set_selection_range(s as u32, s as u32);
                dispatch_input_event(&input2);
                ensure_caret_visible(&input2, s);
            }
            CtrlEditAction::Backspace => {
                let v = input2.value();
                let s = floor_char_boundary(&v, raw_selection_start(&input2));
                if s > 0 {
                    let prev = v[..s]
                        .chars()
                        .next_back()
                        .map(|c| s - c.len_utf8())
                        .unwrap_or(0);
                    let new_val = format!("{}{}", &v[..prev], &v[s..]);
                    input2.set_value(&new_val);
                    let _ = input2.set_selection_range(prev as u32, prev as u32);
                    dispatch_input_event(&input2);
                    ensure_caret_visible(&input2, prev);
                }
            }
            CtrlEditAction::DeleteWord => {
                let v = input2.value();
                let s = floor_char_boundary(&v, raw_selection_start(&input2));
                let bytes = v.as_bytes();
                let mut i = s.saturating_sub(1);
                while i > 0 && bytes[i - 1] == b' ' {
                    i -= 1;
                }
                while i > 0 && bytes[i - 1] != b' ' {
                    i -= 1;
                }
                let i = floor_char_boundary(&v, i);
                let new_val = format!("{}{}", &v[..i], &v[s..]);
                input2.set_value(&new_val);
                let _ = input2.set_selection_range(i as u32, i as u32);
                dispatch_input_event(&input2);
                ensure_caret_visible(&input2, i);
            }
            CtrlEditAction::DeleteToBeginning => {
                let v = input2.value();
                let s = floor_char_boundary(&v, raw_selection_start(&input2));
                input2.set_value(&v[s..]);
                let _ = input2.set_selection_range(0, 0);
                dispatch_input_event(&input2);
                ensure_caret_visible(&input2, 0);
            }
        }
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);

    let target: &web_sys::EventTarget = input.as_ref();
    let opts = web_sys::AddEventListenerOptions::new();
    opts.set_capture(true);
    let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
        "keydown",
        closure.as_ref().unchecked_ref(),
        &opts,
    );
    closure.forget();
}

/// Select the whole query one animation frame after open so Cmd+L reveals the current URL
/// ready to overtype. The query signal is populated by a sibling effect that re-renders the
/// input `value`; selecting synchronously here would catch the still-empty value and leave
/// nothing highlighted.
fn select_all_on_open(input: &web_sys::HtmlInputElement) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let input = input.clone();
    let cb = Closure::once_into_js(move || {
        input.focus().ok();
        let len = input.value().len() as u32;
        let _ = input.set_selection_range(0, len);
        input.set_scroll_left(0);
    });
    let _ = window.request_animation_frame(cb.unchecked_ref());
}

/// Scroll the command-bar input so the caret at byte offset `caret` is visible. Programmatic
/// caret moves (Ctrl+E/A/F/B, deletes, Tab-complete) bypass Chromium's native caret-follow,
/// so on a long URL the caret would otherwise sit off-screen.
fn ensure_caret_visible(input: &web_sys::HtmlInputElement, caret: usize) {
    let value = input.value();
    let caret = floor_char_boundary(&value, caret);
    let Some((viewport, caret_px)) = caret_metrics(input, &value[..caret]) else {
        return;
    };
    if let Some(scroll_left) =
        caret_scroll_left(caret_px, viewport, input.scroll_left() as f64, 8.0)
    {
        input.set_scroll_left(scroll_left as i32);
    }
}

/// The input's usable text viewport width and the pixel offset of `prefix` in the input's
/// current font (measured on an offscreen canvas). `None` if the canvas/context or computed
/// font is unavailable.
fn caret_metrics(input: &web_sys::HtmlInputElement, prefix: &str) -> Option<(f64, f64)> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let style = window.get_computed_style(input).ok()??;
    let font_size = style.get_property_value("font-size").unwrap_or_default();
    let font_family = style.get_property_value("font-family").unwrap_or_default();
    if font_size.is_empty() || font_family.is_empty() {
        return None;
    }
    let font_weight = style.get_property_value("font-weight").unwrap_or_default();
    let font_style = style.get_property_value("font-style").unwrap_or_default();
    let canvas: web_sys::HtmlCanvasElement =
        document.create_element("canvas").ok()?.unchecked_into();
    let ctx: web_sys::CanvasRenderingContext2d = canvas.get_context("2d").ok()??.unchecked_into();
    ctx.set_font(format!("{font_style} {font_weight} {font_size} {font_family}").trim());
    let caret_px = ctx.measure_text(prefix).ok()?.width();
    let pad_left = css_px(&style.get_property_value("padding-left").unwrap_or_default());
    let pad_right = css_px(
        &style
            .get_property_value("padding-right")
            .unwrap_or_default(),
    );
    let viewport = (input.client_width() as f64 - pad_left - pad_right).max(1.0);
    caret_px.is_finite().then_some((viewport, caret_px))
}

/// Parse a computed `<n>px` length to `f64`, defaulting to `0.0`.
fn css_px(value: &str) -> f64 {
    value
        .trim()
        .strip_suffix("px")
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

fn handle_plain_meta_a(e: &web_sys::KeyboardEvent, input: &web_sys::HtmlInputElement) -> bool {
    if !e.meta_key() || e.ctrl_key() || e.alt_key() || e.shift_key() || e.code() != "KeyA" {
        return false;
    }
    e.prevent_default();
    e.stop_immediate_propagation();
    let len = input.value().len() as u32;
    let _ = input.set_selection_range(0, len);
    true
}

fn is_vmux_synthetic_dioxus_keydown(e: &KeyboardEvent) -> bool {
    e.data()
        .downcast::<web_sys::KeyboardEvent>()
        .map(is_vmux_synthetic_keydown)
        .unwrap_or(false)
}

fn is_vmux_synthetic_keydown(e: &web_sys::KeyboardEvent) -> bool {
    js_sys::Reflect::get(e.as_ref(), &JsValue::from_str("_vmuxSyntheticKeydown"))
        .map(|v| v.is_truthy())
        .unwrap_or(false)
}

fn key_for_code(code: &str) -> &str {
    match code {
        "KeyA" => "a",
        "KeyB" => "b",
        "KeyC" => "c",
        "KeyD" => "d",
        "KeyE" => "e",
        "KeyF" => "f",
        "KeyH" => "h",
        "KeyJ" => "j",
        "KeyK" => "k",
        "KeyN" => "n",
        "KeyP" => "p",
        "KeyU" => "u",
        "KeyW" => "w",
        _ => "",
    }
}

fn dispatch_ctrl_keydown(el: &web_sys::HtmlInputElement, code: &str) {
    let init = web_sys::KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_ctrl_key(true);
    init.set_code(code);
    init.set_key(key_for_code(code));
    if let Ok(evt) = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init) {
        let _ = js_sys::Reflect::set(
            evt.as_ref(),
            &JsValue::from_str("_vmuxSyntheticKeydown"),
            &JsValue::TRUE,
        );
        let _ = el.dispatch_event(&evt);
    }
}

/// Largest char boundary of `s` at or before `i`, so DOM text offsets never slice a
/// UTF-8 string mid-character (which would panic the WASM UI on non-ASCII input).
fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn raw_selection_start(input: &web_sys::HtmlInputElement) -> usize {
    input.selection_start().unwrap_or(Some(0)).unwrap_or(0) as usize
}

/// Dispatch a synthetic "input" event so Dioxus picks up value changes.
fn dispatch_input_event(el: &web_sys::HtmlInputElement) {
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    if let Ok(evt) = web_sys::Event::new_with_event_init_dict("input", &init) {
        let _ = el.dispatch_event(&evt);
    }
}
