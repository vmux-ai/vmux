//! The `vmux://command-bar/` page: the Cmd+K modal shell and the launcher it holds.
//!
//! [`Page`] is the modal; [`CommandPalette`] is what it renders, and what `vmux://start/` and the
//! layout page render too. The palette lists rows and reports which was chosen — it never learns
//! what any of them are for.

#![allow(non_snake_case)]

use crate::event::{
    COMMAND_BAR_KEY_EVENT, CommandBarActionEvent, CommandBarKey, CommandBarOpenEvent,
    CommandBarQuery, HISTORY_SUGGESTIONS_RESPONSE_EVENT, HistoryEntry, HistorySuggestionsRequest,
    HistorySuggestionsResponse, OpenId, PATH_COMPLETE_RESPONSE, PathCompleteRequest,
    PathCompleteResponse, PathEntry, StartSelectWorkspace, is_data_uri,
};
use crate::open_target::OpenTarget;
use crate::prompt_media::{
    CHAT_ATTACHMENT_PREVIEWS_EVENT, CHAT_ATTACHMENTS_EVENT, CHAT_MEDIA_ENTRIES_EVENT,
    ChatAttachPaths, ChatAttachment, ChatAttachments, ChatMediaEntries, ChatMediaEntry,
    ChatMediaListRequest, ChatPasteMedia, ChatPickFiles, inline_media_query,
    merge_chat_attachments, replace_inline_media_query,
};
use dioxus::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use vmux_core::input::{PageKeyContext, Unclaimed};
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::caret::TextCaret;
use vmux_ui::components::icon::Icon;
use vmux_ui::components::prompt_box::{PromptBox, PromptPopup, PromptPopupPlacement};
use vmux_ui::components::prompt_composer::{
    PROMPT_INPUT_ID, PromptComposer, PromptComposerAttachment, focus_prompt_end,
};
use vmux_ui::components::prompt_media_options::{PromptMediaOption, PromptMediaOptions};
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{MenuDirection, send, use_key_claim, use_listener};
use vmux_ui::i18n::translate;
use vmux_ui::launcher::keyboard::{CtrlEditAction, CtrlKeyCapture, ctrl_key_capture_for_code};
use vmux_ui::launcher::results::{
    CommandBarResultItem as ResultItem, active_space_index, filter_results, open_session_results,
    prepend_prompt_targets, prompt_target_matches_query, prompt_target_results, prompt_target_url,
    space_switch_results, start_page_results,
};
use vmux_ui::launcher::row::ResultRow;
use vmux_ui::launcher::style::{
    command_bar_input_class, command_bar_input_row_class, command_bar_input_wrap_class,
    result_list_class,
};
use vmux_ui::platform::sleep_ms;
use vmux_ui::scroll::ScrollIntoView;

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
    let on_start_inline_transition = props.on_start_inline_transition;

    let PaletteInput {
        mut query,
        mut selected,
        mut nav_mode,
        mut last_open_id,
        mut last_focus_open_id,
    } = use_palette_input();
    let PathCompletions {
        entries: mut path_completions,
        request_id: mut path_request_id,
        timer: path_search_timer,
    } = use_path_completions();
    let HistorySuggestions {
        entries: mut history_suggestions,
        request_id: mut suggestions_request_id,
        timer: suggestions_search_timer,
    } = use_history_suggestions();
    let PromptMedia {
        mut attachments,
        previews: mut attachment_previews,
        entries: mut media_entries,
        request_id: mut media_request_id,
        requested_query: mut media_requested_query,
        timer: media_search_timer,
        loading: mut media_loading,
        selected: mut media_selected,
    } = use_prompt_media();
    let PromptTarget {
        url: mut start_target_url,
        menu_open: mut target_menu_open,
    } = use_prompt_target();

    let keys = use_key_claim(Unclaimed::Types, move || match variant {
        PaletteVariant::Modal => vec!["command-bar".to_string()],
        PaletteVariant::Start => Vec::new(),
    });
    use_drop(move || {
        let _ = send(&PageKeyContext { keys: Vec::new() });
    });

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
        use_listener::<PathCompleteResponse, _>(PATH_COMPLETE_RESPONSE, move |data| {
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
            let _ = send(&PathCompleteRequest { query: path_query });
        });
    });

    let _history_listener = use_listener::<HistorySuggestionsResponse, _>(
        HISTORY_SUGGESTIONS_RESPONSE_EVENT,
        move |resp| {
            if resp.request_id != *suggestions_request_id.read() {
                return;
            }
            history_suggestions.set(resp.entries);
        },
    );

    let _attachments_listener =
        use_listener::<ChatAttachments, _>(CHAT_ATTACHMENTS_EVENT, move |selected| {
            if !is_start {
                return;
            }
            let current = attachments.peek().clone();
            attachments.set(merge_chat_attachments(&current, &selected.attachments));
            focus_prompt_end(PROMPT_INPUT_ID);
        });

    let _attachment_previews_listener =
        use_listener::<ChatAttachments, _>(CHAT_ATTACHMENT_PREVIEWS_EVENT, move |loaded| {
            if !is_start {
                return;
            }
            let mut previews = attachment_previews.peek().clone();
            for attachment in &loaded.attachments {
                previews.insert(attachment.path.clone(), attachment.clone());
            }
            attachment_previews.set(previews);
            let mut current = attachments.peek().clone();
            for preview in &loaded.attachments {
                if let Some(attachment) = current
                    .iter_mut()
                    .find(|attachment| attachment.path == preview.path)
                {
                    attachment.preview_data_url = preview.preview_data_url.clone();
                }
            }
            attachments.set(current);
        });

    let _media_entries_listener =
        use_listener::<ChatMediaEntries, _>(CHAT_MEDIA_ENTRIES_EVENT, move |response| {
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
            let _ = send(&HistorySuggestionsRequest {
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
            if send(&ChatMediaListRequest {
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
        if open_id.should_refocus(last_focus_open_id()) {
            last_focus_open_id.set(open_id);
            if is_start {
                focus_prompt_end(PROMPT_INPUT_ID);
            } else {
                focus_command_bar_input();
            }
        }
    });

    // `Rc` because `use_hook` clones its value out on every render and a listener must have one
    // owner — two would each try to remove it, and the second removal is the one that silently
    // does nothing.
    #[cfg(web)]
    use_hook(|| Rc::new(is_start.then(|| start_menu_click_outside(target_menu_open))));

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
            display_path: entry.display_path(),
            preview_data_url: entry.preview_data_url.clone(),
            label: file_extension_label(&entry.name),
            is_dir: entry.is_dir,
        })
        .collect::<Vec<_>>();
    let start_prompt_mode = is_start && CommandBarQuery(&q).is_start_prompt();
    let rows = use_memo(move || {
        PaletteRows::of(
            &state(),
            &query(),
            &path_completions(),
            &history_suggestions(),
            variant,
            &start_target_url(),
        )
    });
    let mut palette_keys = PaletteKeys {
        rows,
        query,
        selected,
        nav_mode,
        on_dismiss,
    };
    let _key_listener =
        use_listener::<CommandBarKey, _>(COMMAND_BAR_KEY_EVENT, move |key| palette_keys.apply(key));

    let current_rows = rows();
    let prompt_targets = current_rows.prompt_targets.clone();
    let default_target = current_rows.default_target.clone();
    let results = current_rows.items.clone();
    let sel = current_rows.selected(selected());
    let active_item = results.get(sel).cloned();
    let nav = nav_mode();
    let selected_agent_accent = default_target
        .as_ref()
        .and_then(prompt_target_url)
        .and_then(|url| url.strip_prefix("vmux://agent/"))
        .and_then(|path| path.split('/').next())
        .filter(|agent| !agent.is_empty())
        .map(agent_accent);
    let active_agent_accent = if nav {
        active_item.as_ref()
    } else {
        default_target.as_ref()
    }
    .and_then(prompt_target_url)
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
            Some(ResultItem::Terminal { path }) if path.is_empty() => translate("command-terminal"),
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

    let ghost_text = current_rows.ghost.clone();

    use_effect(move || {
        ScrollIntoView::nearest(&format!("command-bar-item-{}", selected()));
    });

    use_effect(move || {
        let _ = media_entries.read().len();
        ScrollIntoView::nearest(&format!("prompt-media-item-{}", media_selected()));
    });

    let execute = move |item: &ResultItem| {
        let prompt = query();
        let transition = if is_start
            && let Some(target_url) = prompt_target_url(item)
            && vmux_wire::agent::supports_inline_agent_transition(target_url)
            && let Some(handler) = on_start_inline_transition
        {
            Some((
                handler,
                StartInlineTransition {
                    target_url: target_url.to_string(),
                    prompt: prompt.trim().to_string(),
                    attachments: attachments.peek().clone(),
                },
            ))
        } else {
            None
        };
        if matches!(variant, PaletteVariant::Start)
            && (CommandBarQuery(&prompt).is_start_prompt() || !attachments.peek().is_empty())
            && let Some(target_url) = prompt_target_url(item)
        {
            on_close.call(());
            let selected_attachments = attachments.peek().clone();
            if prompt_target_matches_query(item, &prompt) && selected_attachments.is_empty() {
                let _ = send(&CommandBarActionEvent::open(target_url, open_target));
            } else {
                let _ = send(&CommandBarActionEvent::prompt(
                    prompt.trim(),
                    target_url,
                    &selected_attachments,
                ));
            }
            if let Some((handler, next)) = transition {
                handler.call(next);
            }
            return;
        }
        on_close.call(());
        match item {
            ResultItem::Terminal { path } => {
                let _ = send(&CommandBarActionEvent::Terminal {
                    value: path.clone(),
                });
            }
            ResultItem::Stack {
                pane_id, tab_index, ..
            } => {
                let _ = send(&CommandBarActionEvent::SwitchTab {
                    pane: *pane_id,
                    index: *tab_index,
                });
            }
            ResultItem::Command { id, .. } => {
                let _ = send(&CommandBarActionEvent::Command {
                    id: id.clone(),
                    open: open_target,
                });
            }
            ResultItem::Space { id, .. } => {
                let _ = send(&CommandBarActionEvent::Space { id: id.clone() });
            }
            ResultItem::Page { url, .. } => {
                if !url.is_empty() {
                    let _ = send(&CommandBarActionEvent::open(url, open_target));
                }
            }
            ResultItem::Navigate { url } => {
                if !url.is_empty() {
                    let _ = send(&CommandBarActionEvent::open(url, open_target));
                }
            }
            ResultItem::Search { engine, query } => {
                let _ = send(&CommandBarActionEvent::open(
                    &engine.search_url(query),
                    open_target,
                ));
            }
            ResultItem::History { url, .. } => {
                if !url.is_empty() {
                    let _ = send(&CommandBarActionEvent::open(url, open_target));
                }
            }
            ResultItem::File { path, .. } => {
                let _ = send(&CommandBarActionEvent::open(
                    &format!("file://{path}"),
                    open_target,
                ));
            }
            ResultItem::WorkDir { path, .. } => {
                let _ = send(&CommandBarActionEvent::open(
                    &format!("file://{path}"),
                    open_target,
                ));
            }
            ResultItem::RecentFile { url, .. } => {
                let _ = send(&CommandBarActionEvent::open(url, open_target));
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
    let start_attachment_previews = attachment_previews.read();
    let start_prompt_attachments = attachments
        .read()
        .iter()
        .enumerate()
        .map(|(index, attachment)| PromptComposerAttachment {
            key: format!("start-attachment-{}", attachment.path),
            name: attachment.name.clone(),
            label: file_extension_label(&attachment.name),
            preview_data_url: start_attachment_previews
                .get(&attachment.path)
                .map(|preview| preview.preview_data_url.clone())
                .unwrap_or_else(|| attachment.preview_data_url.clone()),
            remove_index: Some(index),
        })
        .collect::<Vec<_>>();
    let start_action_enabled = !q.trim().is_empty() || !attachments.read().is_empty();
    let selected_target_title = default_target
        .as_ref()
        .and_then(|item| match item {
            ResultItem::Page { title, .. } => Some(title.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "Agent".to_string());
    let selected_target_url = default_target
        .as_ref()
        .and_then(prompt_target_url)
        .unwrap_or_default()
        .to_string();
    let workspace_label = if prompt_context.workspace_name.is_empty() {
        "Select project".to_string()
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
                    id: "start-agent-selector-trigger",
                    class: "flex h-7 max-w-44 shrink-0 items-center gap-1.5 rounded-lg px-2 text-[11px] font-medium text-foreground/70 transition hover:bg-foreground/[0.08] hover:text-foreground",
                    title: "Choose agent",
                    onmousedown: move |event| event.prevent_default(),
                    onclick: move |_| {
                        target_menu_open.set(!target_menu_open());
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
                    span { class: "truncate", "{selected_target_title}" }
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
                        title: if prompt_context.cwd.is_empty() { "Choose project" } else { "{prompt_context.cwd}" },
                        onmousedown: move |event| event.prevent_default(),
                        onclick: move |_| {
                            let _ = send(&StartSelectWorkspace {
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
                span { class: "h-1.5 w-1.5 rounded-full bg-success" }
                "Ready"
            }
        }
    };
    let start_keydown_q = q.clone();
    let start_keydown_results = results.clone();
    let start_keydown_default_agent = default_target.clone();
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
        let direction = MenuDirection::of(&e);
        let go_down = direction == Some(MenuDirection::Next);
        let go_up = direction == Some(MenuDirection::Previous);

        if target_menu_open() && (e.key() == Key::Escape || (ctrl && e.code() == Code::KeyC)) {
            e.prevent_default();
            target_menu_open.set(false);
            return;
        }

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
                    let _ = send(&CommandBarActionEvent::prompt(
                        "",
                        "",
                        &selected_attachments,
                    ));
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
                        || prompt_target_matches_query(item, &start_keydown_q)
                        || (matches!(item, ResultItem::Terminal { .. })
                            && vmux_ui::launcher::results::terminal_matches_query(&start_keydown_q))
                }) {
                    execute(item);
                } else if let Some(item) = start_keydown_default_agent.as_ref() {
                    execute(item);
                } else {
                    on_close.call(());
                    let selected_attachments = attachments.peek().clone();
                    let _ = send(&CommandBarActionEvent::prompt(
                        start_keydown_q.trim(),
                        "",
                        &selected_attachments,
                    ));
                }
            } else {
                let prefer_page = matches!(
                    start_keydown_results.get(sel),
                    Some(ResultItem::Page { url, .. })
                        if start_keydown_q.trim().starts_with("vmux://")
                            && url.starts_with(start_keydown_q.trim())
                );
                if !prefer_page
                    && CommandBarQuery(&start_keydown_q)
                        .opens_typed_url_on_enter(open_target, nav_mode())
                {
                    on_close.call(());
                    let _ = send(&CommandBarActionEvent::open(&start_keydown_q, open_target));
                } else if let Some(item) = start_keydown_results.get(sel) {
                    execute(item);
                } else if !start_keydown_q.is_empty() {
                    let _ = send(&CommandBarActionEvent::open(&start_keydown_q, open_target));
                }
            }
        }
    };
    let modal_keydown_q = q.clone();
    let modal_keydown_results = results.clone();
    let modal_keydown_ghost = ghost_text.clone();
    let modal_keydown = move |e: KeyboardEvent| {
        if handle_readline_chord(&e, query, &modal_keydown_ghost) {
            return;
        }
        let ctrl = e.modifiers().contains(Modifiers::CONTROL);
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
        if e.key() == Key::Enter {
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
                    && CommandBarQuery(&modal_keydown_q)
                        .opens_typed_url_on_enter(open_target, nav_mode())
                {
                    on_close.call(());
                    let _ = send(&CommandBarActionEvent::open(&modal_keydown_q, open_target));
                } else if let Some(item) = modal_keydown_results.get(sel) {
                    execute(item);
                } else if !modal_keydown_q.is_empty() {
                    let _ = send(&CommandBarActionEvent::open(&modal_keydown_q, open_target));
                }
            }
            return;
        }
        keys.on_keydown(&e, |_| false);
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
                if target_menu_open() {
                    PromptPopup {
                        placement: PromptPopupPlacement::Downward,
                        id: "start-agent-selector",
                        div { class: "p-1.5",
                            div { class: "px-2 pb-1 pt-0.5 text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60", "Agent" }
                            for item in prompt_targets.iter() {
                                if let ResultItem::Page { url, title, .. } = item {
                                    {
                                        let option_url = url.clone();
                                        let option_selected = url == &selected_target_url;
                                        rsx! {
                                            button {
                                                key: "{url}",
                                                class: if option_selected { "flex w-full items-center gap-2 rounded-xl bg-foreground/[0.08] px-2.5 py-2 text-left text-sm text-foreground" } else { "flex w-full items-center gap-2 rounded-xl px-2.5 py-2 text-left text-sm text-foreground/75 transition hover:bg-foreground/[0.06] hover:text-foreground" },
                                                onmousedown: move |event| event.prevent_default(),
                                                onclick: move |_| {
                                                    start_target_url.set(option_url.clone());
                                                    target_menu_open.set(false);
                                                    selected.set(0);
                                                    nav_mode.set(false);
                                                    focus_prompt_end(PROMPT_INPUT_ID);
                                                },
                                                span { class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.07] text-[10px] font-semibold uppercase", "{title.chars().next().unwrap_or('A')}" }
                                                span { class: "min-w-0 flex-1 truncate", "{title}" }
                                                if option_selected {
                                                    svg {
                                                        class: "h-3.5 w-3.5 shrink-0 text-success",
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
                        target_menu_open.set(false);
                        query.set(value);
                        selected.set(0);
                        nav_mode.set(false);
                    },
                    on_keydown: start_keydown,
                    on_paste: move |_| {
                        let _ = send(&ChatPasteMedia);
                    },
                    on_attach: move |_| {
                        let _ = send(&ChatPickFiles);
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
                        let action_default_agent = default_target.clone();
                        let action_nav = nav;
                        move |_| {
                            if let Some(item) = action_results.get(sel).filter(|item| {
                                !start_prompt_mode
                                    || action_nav
                                    || prompt_target_matches_query(item, &action_query)
                                    || (matches!(item, ResultItem::Terminal { .. })
                                        && vmux_ui::launcher::results::terminal_matches_query(&action_query))
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
                                    let _ = send(&CommandBarActionEvent::prompt(
                                        action_query.trim(),
                                        "",
                                        &selected_attachments,
                                    ));
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
                            aria_label: translate("layout-bookmark-page"),
                            title: format!("{} (⌘D)", translate("layout-bookmark-page")),
                            class: "flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/10 hover:text-foreground",
                            onmousedown: move |event| {
                                event.prevent_default();
                                event.stop_propagation();
                            },
                            onclick: move |event| {
                                event.prevent_default();
                                event.stop_propagation();
                                let _ = send(&crate::event::BookmarksCommandEvent {
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
            if !target_menu_open() && media_menu_open {
                PromptPopup {
                    placement: PromptPopupPlacement::Downward,
                    id: "command-bar-results",
                    PromptMediaOptions {
                        items: prompt_media_options,
                        selected: media_sel,
                        loading: media_loading(),
                        loading_label: translate("agent-loading-media"),
                        empty_label: translate("agent-no-matching-media"),
                        on_hover: move |index| media_selected.set(index),
                        on_select: move |index| {
                            if let Some(entry) = media_entries.peek().get(index).cloned() {
                                select_start_media_entry(&entry, query, media_selected);
                            }
                        },
                    }
                }
            }
            if !target_menu_open() && !media_menu_open && !results.is_empty() {
                PromptPopup {
                    placement: if is_start { PromptPopupPlacement::Downward } else { PromptPopupPlacement::Inline },
                    id: "command-bar-results",
                    class: if is_start { "" } else { result_list_class() },
                for (i, item) in results.iter().enumerate() {
                    ResultRow {
                        key: "{i}",
                        index: i,
                        item: item.clone(),
                        selected: i == sel,
                        on_activate: {
                            let item = item.clone();
                            move |_| { execute(&item); }
                        },
                        space_switch,
                        start_prompt_mode,
                        query: q.clone(),
                        on_hover: move |_| {
                            if is_start {
                                selected.set(i);
                            }
                        },
                    }
                }
                }
            }
        }
    }
}

const HOST_SEARCH_DEBOUNCE_MS: u32 = 300;

/// Whether the search currently waiting to fire has been superseded.
type HostSearchTimer = Rc<RefCell<Option<Rc<Cell<bool>>>>>;

fn cancel_host_search(timer: &HostSearchTimer) {
    if let Some(cancelled) = timer.borrow_mut().take() {
        cancelled.set(true);
    }
}

/// Ask the host for results once the user stops typing, replacing whatever was already waiting.
fn schedule_host_search(timer: HostSearchTimer, callback: impl FnOnce() + 'static) {
    cancel_host_search(&timer);
    let cancelled = Rc::new(Cell::new(false));
    *timer.borrow_mut() = Some(cancelled.clone());
    spawn(async move {
        sleep_ms(HOST_SEARCH_DEBOUNCE_MS).await;
        if cancelled.get() {
            return;
        }
        timer.borrow_mut().take();
        callback();
    });
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
pub struct StartInlineTransition {
    pub target_url: String,
    pub prompt: String,
    pub attachments: Vec<crate::prompt_media::ChatAttachment>,
}

/// Props for [`CommandPalette`].
#[derive(Props, Clone, PartialEq)]
pub struct PaletteProps {
    /// Launcher payload (entries + open target); the input resets when its `open_id` changes.
    pub state: ReadSignal<CommandBarOpenEvent>,
    /// Presentation context (placeholder text and host expectations).
    pub variant: PaletteVariant,
    /// Called after an entry executes (the modal host closes it; home is a no-op).
    pub on_close: EventHandler<()>,
    /// Called when the user cancels (Esc / Ctrl-C).
    pub on_dismiss: EventHandler<()>,
    /// Called on query/selection change (the modal re-emits its size).
    pub on_activity: EventHandler<()>,
    #[props(default)]
    pub on_start_inline_transition: Option<EventHandler<StartInlineTransition>>,
}

/// Everything a palette shows, derived from what the host sent and what the user has typed.
///
/// Held as a memo rather than rebuilt inline because the keyboard now reaches this list from two
/// places: the render, and the host's answer to a key the page handed over. That answer arrives in
/// a listener registered once, which can read a signal but cannot see a render's locals — so the
/// list has to be somewhere both can look, or there would be two copies of it to disagree.
#[derive(Clone, PartialEq)]
struct PaletteRows {
    items: Vec<ResultItem>,
    prompt_targets: Vec<ResultItem>,
    default_target: Option<ResultItem>,
    /// The greyed-out remainder the first path completion would add to what has been typed.
    ghost: String,
}

impl PaletteRows {
    fn of(
        state: &CommandBarOpenEvent,
        query: &str,
        completions: &[PathEntry],
        history: &[HistoryEntry],
        variant: PaletteVariant,
        target_url: &str,
    ) -> Self {
        let is_start = matches!(variant, PaletteVariant::Start);
        let prompt_targets = if is_start {
            prompt_target_results(&state.pages, "")
        } else {
            Vec::new()
        };
        let default_target = prompt_targets
            .iter()
            .find(|item| prompt_target_url(item) == Some(target_url))
            .cloned()
            .or_else(|| prompt_targets.first().cloned());
        let start_prompt_mode = is_start && CommandBarQuery(query).is_start_prompt();

        let mut items: Vec<ResultItem> = if state.space_switch {
            space_switch_results(&state.spaces, &state.pages, query)
        } else if is_start && query.trim().is_empty() {
            open_session_results(&state.tabs, &state.pages)
        } else if start_prompt_mode {
            start_page_results(
                &state.pages,
                &state.work_dirs,
                &state.recent_files,
                &state.search_engines,
                query,
            )
        } else {
            let is_new_tab = matches!(state.target, Some(OpenTarget::InNewStack));
            let matched = filter_results(
                query,
                &state.tabs,
                &state.commands,
                &state.spaces,
                &state.pages,
                is_new_tab,
                history,
                &state.work_dirs,
                &state.recent_files,
            );
            let completions: &[PathEntry] = if completion_query(query).is_some() {
                completions
            } else {
                &[]
            };
            let matched = if completions.is_empty() {
                matched
            } else {
                let mut combined: Vec<ResultItem> = completions
                    .iter()
                    .take(8)
                    .map(|entry| ResultItem::File {
                        path: entry.full_path.clone(),
                        is_dir: entry.is_dir,
                    })
                    .collect();
                combined.extend(matched);
                combined
            };
            if is_start {
                matched
                    .into_iter()
                    .filter(|item| {
                        !matches!(
                            item,
                            ResultItem::Stack { url, .. } | ResultItem::Page { url, .. }
                                if url.trim_end_matches('/') == "vmux://start"
                        )
                    })
                    .collect()
            } else {
                matched
            }
        };
        if start_prompt_mode {
            prepend_prompt_targets(&mut items, default_target.as_ref(), &prompt_targets, query);
        }

        Self {
            items,
            prompt_targets,
            default_target,
            ghost: Self::ghost_of(query, completions),
        }
    }

    fn ghost_of(query: &str, completions: &[PathEntry]) -> String {
        if completion_query(query).is_none() {
            return String::new();
        }
        let Some(first) = completions.first() else {
            return String::new();
        };
        let typed = query.trim();
        let full = &first.full_path;
        if !full.to_lowercase().starts_with(&typed.to_lowercase())
            || !full.is_char_boundary(typed.len())
        {
            return String::new();
        }
        full[typed.len()..].to_string()
    }

    /// What the input holds once the greyed-out remainder is accepted.
    fn completed(&self, query: &str) -> String {
        format!("{query}{}", self.ghost)
    }

    /// The highlighted row, clamped to what is actually on screen.
    fn selected(&self, stored: usize) -> usize {
        stored.min(self.items.len().saturating_sub(1))
    }

    /// Where a move of one row lands. Clamped at both ends rather than wrapping, which is what the
    /// command bar has always done and what distinguishes it from a popup menu.
    fn step(&self, from: usize, direction: MenuDirection) -> usize {
        match direction {
            MenuDirection::Next => (from + 1).min(self.items.len().saturating_sub(1)),
            MenuDirection::Previous => from.saturating_sub(1),
        }
    }
}

/// The palette's keyboard, on the far side of the keymap.
///
/// Nothing here names a key. The page hands the stroke over, the core decides, and this performs
/// the verb it came back as — which is the only reason `Ctrl+n` can be rebound in `settings.json`
/// without the palette knowing it moved.
///
/// Every field is a signal or a handler rather than a value, because the answer arrives in a
/// listener registered on first render: a captured result list would be one keystroke stale by the
/// time the first key was pressed.
#[derive(Clone, Copy)]
struct PaletteKeys {
    rows: Memo<PaletteRows>,
    query: Signal<String>,
    selected: Signal<usize>,
    nav_mode: Signal<bool>,
    on_dismiss: EventHandler<()>,
}

impl PaletteKeys {
    fn apply(&mut self, key: CommandBarKey) {
        match key {
            CommandBarKey::Next => self.move_selection(MenuDirection::Next),
            CommandBarKey::Previous => self.move_selection(MenuDirection::Previous),
            CommandBarKey::Complete => self.accept_completion(),
            CommandBarKey::Dismiss => self.on_dismiss.call(()),
        }
    }

    fn move_selection(&mut self, direction: MenuDirection) {
        let rows = self.rows.read();
        let landed = rows.step(rows.selected(*self.selected.peek()), direction);
        drop(rows);
        self.selected.set(landed);
        self.nav_mode.set(true);
    }

    /// Accept the greyed-out remainder, and put the caret after it.
    ///
    /// The input's value is written directly as well as through the signal: Chromium does not
    /// scroll to a caret it did not move itself, so a long path would complete off-screen.
    fn accept_completion(&mut self) {
        let rows = self.rows.read();
        if rows.ghost.is_empty() {
            return;
        }
        let completed = rows.completed(&self.query.peek());
        drop(rows);
        self.query.set(completed.clone());
        self.selected.set(0);
        TextCaret::in_field(COMMAND_BAR_INPUT_ID).place(completed.len());
    }
}

/// What the user has typed, and which row is selected.
struct PaletteInput {
    query: Signal<String>,
    selected: Signal<usize>,
    nav_mode: Signal<bool>,
    /// The open this input was last cleared for, so a re-render does not wipe what is being typed.
    last_open_id: Signal<OpenId>,
    /// The open this input was last focused for, tracked apart from the reset because the two do
    /// not always happen in the same render.
    last_focus_open_id: Signal<OpenId>,
}

fn use_palette_input() -> PaletteInput {
    PaletteInput {
        query: use_signal(String::new),
        selected: use_signal(|| 0usize),
        nav_mode: use_signal(|| false),
        last_open_id: use_signal(|| OpenId(u64::MAX)),
        last_focus_open_id: use_signal(|| OpenId(u64::MAX)),
    }
}

/// Path rows the host answered with, and the request they belong to.
///
/// The id is what makes a late answer harmless: the host replies out of order under load, and a
/// reply carrying anything but the current id is dropped rather than rendered over newer results.
struct PathCompletions {
    entries: Signal<Vec<PathEntry>>,
    request_id: Signal<u64>,
    timer: HostSearchTimer,
}

fn use_path_completions() -> PathCompletions {
    PathCompletions {
        entries: use_signal(Vec::<PathEntry>::new),
        request_id: use_signal(|| 0u64),
        timer: use_hook(|| Rc::new(RefCell::new(None))),
    }
}

/// History rows the host answered with, and the request they belong to.
struct HistorySuggestions {
    entries: Signal<Vec<HistoryEntry>>,
    request_id: Signal<u64>,
    timer: HostSearchTimer,
}

fn use_history_suggestions() -> HistorySuggestions {
    HistorySuggestions {
        entries: use_signal(Vec::<HistoryEntry>::new),
        request_id: use_signal(|| 0u64),
        timer: use_hook(|| Rc::new(RefCell::new(None))),
    }
}

/// What the composer has attached, and the media picker's own browsing state.
///
/// `attachments` is what a prompt would carry; everything else exists only while the picker is
/// open. They are held together because closing the picker has to leave the attachments alone.
struct PromptMedia {
    attachments: Signal<Vec<ChatAttachment>>,
    previews: Signal<HashMap<String, ChatAttachment>>,
    entries: Signal<Vec<ChatMediaEntry>>,
    request_id: Signal<u64>,
    /// The query the open request was made for, so an answer to a stale one is ignored.
    requested_query: Signal<Option<String>>,
    timer: HostSearchTimer,
    loading: Signal<bool>,
    selected: Signal<usize>,
}

fn use_prompt_media() -> PromptMedia {
    PromptMedia {
        attachments: use_signal(Vec::<ChatAttachment>::new),
        previews: use_signal(HashMap::<String, ChatAttachment>::new),
        entries: use_signal(Vec::<ChatMediaEntry>::new),
        request_id: use_signal(|| 0u64),
        requested_query: use_signal(|| None::<String>),
        timer: use_hook(|| Rc::new(RefCell::new(None))),
        loading: use_signal(|| false),
        selected: use_signal(|| 0usize),
    }
}

/// Which page a prompt would go to, and whether the picker for it is showing.
struct PromptTarget {
    url: Signal<String>,
    menu_open: Signal<bool>,
}

fn use_prompt_target() -> PromptTarget {
    PromptTarget {
        url: use_signal(String::new),
        menu_open: use_signal(|| false),
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
    let reference = entry.reference();
    let replacement = if entry.is_dir {
        format!("@{reference}/")
    } else {
        if send(&ChatAttachPaths {
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

const COMMAND_BAR_INPUT_ID: &str = "command-bar-input";

/// Claim focus for the query field and offer its contents for overtyping.
///
/// The Ctrl chords this used to install as a capture-phase DOM listener are now part of the
/// field's own `onkeydown` — see [`handle_readline_chord`].
fn focus_command_bar_input() {
    FocusClaim::new(COMMAND_BAR_INPUT_ID).request();
    TextCaret::in_field(COMMAND_BAR_INPUT_ID).select_all_from_start_next_frame();
}

/// Put the caret in the launcher's prompt field.
///
/// The launcher owns the gesture and this crate owns the field, so the id stays here rather than
/// being spelled out by every caller that wants the composer focused.
pub fn focus_prompt_input() {
    focus_prompt_end(PROMPT_INPUT_ID);
}

/// Cmd+A and the Ctrl readline chords, offered the key before the field's own handling.
///
/// Returns whether the key was consumed.
///
/// This was a capture-phase listener on the input element, installed once behind a `_ctrlBound`
/// latch, because Chromium's macOS readline emulation acts on Ctrl+A/E and the handler had to run
/// first to preempt it. Dioxus dispatches on the bubble phase, so whether `prevent_default` still
/// wins that race is the one thing here no test settles — it needs a keyboard.
fn handle_readline_chord(event: &KeyboardEvent, mut query: Signal<String>, ghost: &str) -> bool {
    if handle_plain_meta_a(event) {
        return true;
    }
    if !event.modifiers().contains(Modifiers::CONTROL) {
        return false;
    }

    let action = match ctrl_key_capture_for_code(&event.code().to_string()) {
        CtrlKeyCapture::Ignore => return false,
        // Preempt the browser, then let the field's own handler read the same key.
        CtrlKeyCapture::PassToDioxus => {
            event.prevent_default();
            return false;
        }
        CtrlKeyCapture::Edit(action) => action,
    };

    event.prevent_default();
    event.stop_propagation();
    apply_ctrl_edit(&mut query, action, ghost);
    true
}

/// Run a readline edit against the query, then put the caret where it landed.
///
/// The arithmetic is [`CtrlEditAction::apply`]'s and the caret is [`TextCaret`]'s. The value goes
/// through the signal the field is bound to, so there is nothing to tell Dioxus afterwards — the
/// element write and the synthetic `input` event that used to follow it existed only because the
/// field was uncontrolled.
fn apply_ctrl_edit(query: &mut Signal<String>, action: CtrlEditAction, ghost: &str) {
    let caret = TextCaret::in_field(COMMAND_BAR_INPUT_ID);
    let value = query.peek().clone();
    let ghost = match action {
        CtrlEditAction::End => ghost,
        _ => "",
    };

    let edited = action.apply(&value, caret.position(), ghost);
    if edited.value != value {
        query.set(edited.value);
    }
    caret.place(edited.caret);
}

/// Close the start-page agent selector when a `mousedown` lands outside the popup and its trigger.
/// Capture-phase so it beats the buttons' own handlers; clicks inside (`#start-agent-selector`) or
/// on the trigger (`#start-agent-selector-trigger`) are left alone.
//
// `web` only. `DocumentListener` has a native stub that takes no event, so there is nothing to
// read a target from; dismissing on an outside pointer natively wants a backdrop element instead.
// Only the start page installs this, and the start page is not native yet.
#[cfg(web)]
fn start_menu_click_outside(
    mut menu_open: Signal<bool>,
) -> Option<vmux_ui::dom_listener::DocumentListener> {
    use vmux_ui::dom_listener::DocumentListener;
    use wasm_bindgen::JsCast;

    DocumentListener::capture("mousedown", move |event| {
        if !menu_open() {
            return;
        }
        let inside = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .and_then(|element| {
                element
                    .closest("#start-agent-selector, #start-agent-selector-trigger")
                    .ok()
                    .flatten()
            })
            .is_some();
        if !inside {
            menu_open.set(false);
        }
    })
}

/// Cmd+A with no other modifier selects the query rather than the page.
fn handle_plain_meta_a(event: &KeyboardEvent) -> bool {
    let modifiers = event.modifiers();
    let plain_meta = modifiers.contains(Modifiers::META)
        && !modifiers.contains(Modifiers::CONTROL)
        && !modifiers.contains(Modifiers::ALT)
        && !modifiers.contains(Modifiers::SHIFT);
    if !plain_meta || event.code() != Code::KeyA {
        return false;
    }

    event.prevent_default();
    event.stop_propagation();
    TextCaret::in_field(COMMAND_BAR_INPUT_ID).select_all();
    true
}
