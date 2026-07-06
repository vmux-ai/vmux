#![allow(non_snake_case)]

use crate::command_bar::keyboard::{
    CtrlEditAction, CtrlKeyCapture, ctrl_key_capture_for_code,
    ignore_physical_rerouted_ctrl_keydown,
};
use crate::command_bar::results::{CommandBarResultItem as ResultItem, filter_results};
use crate::command_bar::style::{
    command_bar_input_class, command_bar_input_row_class, command_bar_input_wrap_class,
    result_content_row_class, result_favicon_class, result_history_url_class, result_item_class,
    result_leading_icon_class, result_list_class, result_primary_text_class,
    result_secondary_text_class, result_shortcut_badge_class, result_terminal_path_class,
    result_trailing_slot_class,
};
use dioxus::prelude::*;
use vmux_command::event::{
    CommandBarActionEvent, CommandBarOpenEvent, HISTORY_SUGGESTIONS_RESPONSE_EVENT, HistoryEntry,
    HistorySuggestionsRequest, HistorySuggestionsResponse, PATH_COMPLETE_RESPONSE,
    PathCompleteRequest, PathCompleteResponse, PathEntry, command_bar_should_refocus, is_data_uri,
    looks_like_url, should_open_typed_query_on_enter,
};
use vmux_command::open_target::OpenTarget;
use vmux_ui::components::icon::Icon;
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener};
use vmux_ui::icon::PageIconView;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// Where a [`CommandPalette`] is rendered: the Cmd+K modal or the `vmux://start/` page.
#[derive(Clone, Copy, PartialEq)]
pub enum PaletteVariant {
    /// The Cmd+K command-bar modal overlay.
    Modal,
    /// The `vmux://start/` launcher page.
    Start,
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
}

/// The shared command-bar body: input, live-filtered results, file-path completion,
/// history suggestions, keyboard navigation, and action dispatch. Rendered by both
/// the Cmd+K modal ([`PaletteVariant::Modal`]) and the start launcher ([`PaletteVariant::Start`]).
#[component]
pub fn CommandPalette(props: PaletteProps) -> Element {
    let state = props.state;
    let variant = props.variant;
    let on_close = props.on_close;
    let on_dismiss = props.on_dismiss;
    let on_activity = props.on_activity;

    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    let mut nav_mode = use_signal(|| false);
    let mut path_completions = use_signal(Vec::<PathEntry>::new);
    let mut history_suggestions = use_signal(Vec::<HistoryEntry>::new);
    let mut suggestions_request_id = use_signal(|| 0u64);
    let mut last_open_id = use_signal(|| u64::MAX);
    let mut last_focus_open_id = use_signal(|| u64::MAX);

    use_effect(move || {
        let s = state();
        if last_open_id() != s.open_id {
            last_open_id.set(s.open_id);
            query.set(s.url.clone());
            selected.set(0);
            nav_mode.set(false);
            path_completions.set(Vec::new());
            history_suggestions.set(Vec::new());
        }
    });

    let _path_listener =
        use_bin_event_listener::<PathCompleteResponse, _>(PATH_COMPLETE_RESPONSE, move |data| {
            path_completions.set(data.completions);
        });

    use_effect(move || {
        let q = query();
        let Some(path_query) = completion_query(&q) else {
            path_completions.set(Vec::new());
            return;
        };
        let _ = try_cef_bin_emit_rkyv(&PathCompleteRequest { query: path_query });
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

    use_effect(move || {
        let q = query();
        let trimmed = q.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('>')
            || trimmed.starts_with('/')
            || trimmed.starts_with('~')
            || trimmed.starts_with("vmux://")
            || trimmed.starts_with("file:")
        {
            history_suggestions.set(Vec::new());
            return;
        }
        let id = *suggestions_request_id.peek() + 1;
        suggestions_request_id.set(id);
        let _ = try_cef_bin_emit_rkyv(&HistorySuggestionsRequest {
            query: trimmed.to_string(),
            limit: 5,
            request_id: id,
        });
    });

    use_effect(move || {
        let open_id = state().open_id;
        if command_bar_should_refocus(last_focus_open_id(), open_id) {
            last_focus_open_id.set(open_id);
            focus_and_install_ctrl_bindings();
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
    let open_target = state_val.target;
    let is_new_tab = matches!(open_target, Some(OpenTarget::InNewStack));

    let q = query();
    let results = {
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
        if completions.is_empty() {
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
        }
    };
    let results: Vec<ResultItem> = if matches!(variant, PaletteVariant::Start) {
        results
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
        results
    };
    let sel = selected().min(results.len().saturating_sub(1));
    let active_item = results.get(sel).cloned();
    let nav = nav_mode();
    let display_text = if nav {
        match &active_item {
            Some(ResultItem::Command { name, .. }) => format!("> {name}"),
            Some(ResultItem::Navigate { url }) => url.clone(),
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

    let execute = move |item: &ResultItem| {
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
    };

    let placeholder = match variant {
        PaletteVariant::Start => "Search or ask\u{2026}",
        PaletteVariant::Modal => {
            if is_new_tab {
                "Search or type a URL, or select Terminal..."
            } else {
                "Type a URL, search tabs, or > for commands..."
            }
        }
    };

    rsx! {
        div { class: "p-2",
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
                                let is_u = url.contains("://") || (url.contains('.') && !url.contains(' '));
                                (false, false, is_u)
                            }
                            Some(ResultItem::History { .. }) => (false, false, true),
                            Some(ResultItem::File { .. }) => (false, true, false),
                            Some(ResultItem::WorkDir { .. }) => (false, true, false),
                            Some(ResultItem::RecentFile { .. }) => (false, true, false),
                            None => (false, false, false),
                        }
                    } else {
                        let trimmed = q.trim();
                        let cmd = trimmed.starts_with('>');
                        let pth = !cmd && (trimmed.starts_with('/') || trimmed.starts_with('~'));
                        let url = !cmd && !pth && (trimmed.contains("://") || (trimmed.contains('.') && !trimmed.contains(' ')));
                        (cmd, pth, url)
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
                        oninput: move |e| {
                            query.set(e.value());
                            selected.set(0);
                            nav_mode.set(false);
                        },
                        onkeydown: move |e| {
                            if e.key() == Key::Tab {
                                e.prevent_default();
                                let gt = ghost_text.clone();
                                if !gt.is_empty() {
                                    let new_val = format!("{}{}", q, gt);
                                    query.set(new_val.clone());
                                    selected.set(0);
                                    if let Some(el) = web_sys::window()
                                        .and_then(|w| w.document())
                                        .and_then(|d| d.get_element_by_id("command-bar-input"))
                                    {
                                        let input: web_sys::HtmlInputElement = el.unchecked_into();
                                        input.set_value(&new_val);
                                        let len = new_val.len() as u32;
                                        let _ = input.set_selection_range(len, len);
                                    }
                                }
                                return;
                            }

                            let ctrl = e.modifiers().contains(Modifiers::CONTROL);
                            let vmux_synthetic = is_vmux_synthetic_dioxus_keydown(&e);
                            if ctrl
                                && ignore_physical_rerouted_ctrl_keydown(
                                    &e.code().to_string(),
                                    vmux_synthetic,
                                )
                            {
                                e.prevent_default();
                                return;
                            }
                            let go_down = (e.key() == Key::ArrowDown && !ctrl)
                                || (ctrl && matches!(e.code(), Code::KeyN | Code::KeyJ));
                            let go_up = (e.key() == Key::ArrowUp && !ctrl)
                                || (ctrl && matches!(e.code(), Code::KeyP | Code::KeyK));

                            if go_down {
                                e.prevent_default();
                                let max = results.len().saturating_sub(1);
                                selected.set((sel + 1).min(max));
                                nav_mode.set(true);
                            } else if go_up {
                                e.prevent_default();
                                selected.set(sel.saturating_sub(1));
                                nav_mode.set(true);
                            } else if e.key() == Key::Escape
                                || (ctrl && e.code() == Code::KeyC)
                            {
                                on_dismiss.call(());
                            } else if e.key() == Key::Enter {
                                let prefer_page = matches!(
                                    results.get(sel),
                                    Some(ResultItem::Page { url, .. })
                                        if q.trim().starts_with("vmux://")
                                            && url.starts_with(q.trim())
                                );
                                if !prefer_page
                                    && should_open_typed_query_on_enter(open_target, nav_mode(), &q)
                                {
                                    on_close.call(());
                                    emit_action_with_target("open", &q, open_target);
                                } else if let Some(item) = results.get(sel) {
                                    execute(item);
                                } else if !q.is_empty() {
                                    emit_action_with_target("open", &q, open_target);
                                }
                            }
                        },
                    }
                }
            }
        }
        if !results.is_empty() {
            div { id: "command-bar-results", class: result_list_class(),
                for (i, item) in results.iter().enumerate() {
                    div {
                        key: "{i}",
                        id: "command-bar-item-{i}",
                        class: result_item_class(i == sel),
                        onclick: {
                            let item = item.clone();
                            move |_| { execute(&item); }
                        },
                        match item {
                            ResultItem::Terminal { path } => rsx! {
                                div { class: result_content_row_class(),
                                    span { class: "shrink-0 text-sm text-muted-foreground", ">_" }
                                    if path.is_empty() {
                                        span { class: "text-sm text-foreground", "Terminal" }
                                    } else {
                                        span { class: "shrink-0 text-sm text-foreground", "Open in Terminal" }
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
                                    if location.is_empty() { "Stack" } else { "{location}" }
                                }
                            },
                            ResultItem::Space { name, profile, is_active, tab_count, .. } => rsx! {
                                div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                    div { class: "flex min-w-0 items-center gap-2",
                                        span { class: result_primary_text_class(), "{name}" }
                                        if *is_active {
                                            span { class: "rounded-full bg-blue-500/15 px-2 py-0.5 text-xs text-blue-300", "active" }
                                        }
                                    }
                                    span { class: result_secondary_text_class(), "{profile}" }
                                }
                                span { class: result_trailing_slot_class(), "{tab_count} tabs" }
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
                                        span { class: result_primary_text_class(), "{title}" }
                                        span { class: result_secondary_text_class(), "{url}" }
                                    }
                                }
                                span { class: result_trailing_slot_class(),
                                    if shortcut.is_empty() {
                                        "New tab"
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
                                        span { class: "text-sm text-foreground", "Search" }
                                    } else if looks_like_url(url) {
                                        span { class: result_primary_text_class(), "Open \"{url}\"" }
                                    } else {
                                        span { class: result_primary_text_class(), "Search \"{url}\"" }
                                    }
                                }
                                if !url.is_empty() {
                                    span { class: result_trailing_slot_class(), "\u{21b5}" }
                                } else {
                                    span { class: result_trailing_slot_class() }
                                }
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
    let len = input.value().len() as u32;
    let _ = input.set_selection_range(0, len);

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
            }
            CtrlEditAction::End => {
                let ghost = input2.get_attribute("data-ghost").unwrap_or_default();
                if !ghost.is_empty() {
                    let new_val = format!("{}{}", input2.value(), ghost);
                    input2.set_value(&new_val);
                    let len = new_val.len() as u32;
                    let _ = input2.set_selection_range(len, len);
                    dispatch_input_event(&input2);
                } else {
                    let len = input2.value().len() as u32;
                    let _ = input2.set_selection_range(len, len);
                }
            }
            CtrlEditAction::Forward => {
                let p = (input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) + 1)
                    .min(input2.value().len() as u32);
                let _ = input2.set_selection_range(p, p);
            }
            CtrlEditAction::Back => {
                let p = input2
                    .selection_start()
                    .unwrap_or(Some(0))
                    .unwrap_or(0)
                    .saturating_sub(1);
                let _ = input2.set_selection_range(p, p);
            }
            CtrlEditAction::Delete => {
                let v = input2.value();
                let s = floor_char_boundary(&v, raw_selection_start(&input2));
                let end = v[s..].chars().next().map(|c| s + c.len_utf8()).unwrap_or(s);
                let new_val = format!("{}{}", &v[..s], &v[end..]);
                input2.set_value(&new_val);
                let _ = input2.set_selection_range(s as u32, s as u32);
                dispatch_input_event(&input2);
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
            }
            CtrlEditAction::DeleteToBeginning => {
                let v = input2.value();
                let s = floor_char_boundary(&v, raw_selection_start(&input2));
                input2.set_value(&v[s..]);
                let _ = input2.set_selection_range(0, 0);
                dispatch_input_event(&input2);
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
