#![allow(non_snake_case)]

use crate::command_bar::keyboard::{
    CtrlEditAction, CtrlKeyCapture, ctrl_key_capture_for_code,
    ignore_physical_rerouted_ctrl_keydown,
};
use crate::command_bar::results::{CommandBarResultItem as ResultItem, filter_results};
use crate::command_bar::style::{
    command_bar_input_class, command_bar_input_row_class, command_bar_input_wrap_class,
    command_bar_root_class, command_bar_shell_class, result_content_row_class,
    result_favicon_class, result_history_url_class, result_item_class, result_leading_icon_class,
    result_list_class, result_primary_text_class, result_secondary_text_class,
    result_shortcut_badge_class, result_terminal_path_class, result_trailing_slot_class,
};
use dioxus::prelude::*;
use vmux_command::event::{
    COMMAND_BAR_OPEN_EVENT, CommandBarActionEvent, CommandBarOpenEvent, CommandBarReadyEvent,
    CommandBarRenderedEvent, CommandBarSizeEvent, HISTORY_SUGGESTIONS_RESPONSE_EVENT, HistoryEntry,
    HistorySuggestionsRequest, HistorySuggestionsResponse, PATH_COMPLETE_RESPONSE,
    PathCompleteRequest, PathCompleteResponse, PathEntry, command_bar_open_should_ack,
    command_bar_open_should_reset_input, looks_like_url, should_open_typed_query_on_enter,
};
use vmux_ui::components::icon::Icon;
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut state = use_signal(CommandBarOpenEvent::default);
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    let mut is_new_stack = use_signal(|| false);
    let mut is_open = use_signal(|| false);
    let mut nav_mode = use_signal(|| false);
    let mut current_open_id = use_signal(|| 0u64);
    let mut last_rendered_open_id = use_signal(|| 0u64);
    let mut ready_sent = use_signal(|| false);
    let mut observed_size_open_id = use_signal(|| None::<u64>);
    let mut outside_pointer_listener_installed = use_signal(|| false);

    let mut path_completions = use_signal(Vec::<PathEntry>::new);
    let mut history_suggestions = use_signal(Vec::<HistoryEntry>::new);
    let mut suggestions_request_id = use_signal(|| 0u64);

    let open_listener =
        use_bin_event_listener::<CommandBarOpenEvent, _>(COMMAND_BAR_OPEN_EVENT, move |data| {
            let open_id = data.open_id;
            let should_reset_input =
                command_bar_open_should_reset_input(current_open_id(), open_id);
            if !should_reset_input {
                if command_bar_open_should_ack(open_id) {
                    let _ = try_cef_bin_emit_rkyv(&CommandBarRenderedEvent { open_id });
                }
                return;
            }
            current_open_id.set(open_id);
            path_completions.set(Vec::new());
            history_suggestions.set(Vec::new());
            query.set(data.url.clone());
            selected.set(0);
            nav_mode.set(false);
            is_new_stack.set(matches!(
                data.target,
                Some(vmux_command::open_target::OpenTarget::InNewStack)
            ));
            state.set(data);
            is_open.set(true);
            if command_bar_open_should_ack(open_id) {
                last_rendered_open_id.set(0);
            }
        });

    use_effect(move || {
        if !(open_listener.is_loading)()
            && !ready_sent()
            && try_cef_bin_emit_rkyv(&CommandBarReadyEvent).is_ok()
        {
            ready_sent.set(true);
        }
    });

    use_effect(move || {
        let open = is_open();
        let open_id = current_open_id();
        if open
            && open_id != 0
            && last_rendered_open_id() != open_id
            && try_cef_bin_emit_rkyv(&CommandBarRenderedEvent { open_id }).is_ok()
        {
            last_rendered_open_id.set(open_id);
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

    // Focus input and install emacs-style Ctrl shortcuts AFTER Dioxus renders
    // the input element. Running in the event listener above is too early --
    // Dioxus hasn't created the DOM element yet.
    use_effect(move || {
        if is_open() {
            focus_and_install_ctrl_bindings();
        }
    });

    use_effect(move || {
        if outside_pointer_listener_installed() {
            return;
        }
        if install_command_bar_outside_pointer_listener(is_open) {
            outside_pointer_listener_installed.set(true);
        }
    });

    use_effect(move || {
        if !is_open() || !state().native_windowed {
            return;
        }
        let open_id = current_open_id();
        if observed_size_open_id() == Some(open_id) {
            return;
        }
        if install_command_bar_size_observer() {
            observed_size_open_id.set(Some(open_id));
        }
    });

    use_effect(move || {
        if !is_open() || !state().native_windowed {
            return;
        }
        let _ = query();
        let _ = selected();
        let _ = nav_mode();
        let _ = path_completions();
        let _ = history_suggestions();
        schedule_command_bar_size_emit();
    });

    let CommandBarOpenEvent {
        url: _,
        native_windowed,
        space_name,
        spaces,
        tabs,
        commands,
        pages,
        target: open_target,
        ..
    } = state();
    let q = query();
    let is_new_tab = is_new_stack();
    let results = {
        let history = history_suggestions();
        let mut r = filter_results(&q, &tabs, &commands, &spaces, &pages, is_new_tab, &history);
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
            if full.to_lowercase().starts_with(&q_trimmed.to_lowercase()) {
                full[q_trimmed.len()..].to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };

    // Auto-scroll selected item into view when selection changes.
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

    let mut execute = move |item: &ResultItem| {
        is_open.set(false);
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
        }
    };

    if !is_open() {
        return rsx! { div { class: "h-full w-full" } };
    }

    rsx! {
        div {
            class: command_bar_root_class(native_windowed),
            onclick: move |_| { dismiss_command_bar(is_open); },
            div {
                id: "command-bar-shell",
                class: command_bar_shell_class(native_windowed),
                onclick: move |e| { e.stop_propagation(); },
                // Inner glow overlay
                div { class: "pointer-events-none absolute inset-0 rounded-2xl bg-gradient-to-br from-white/20 to-transparent" }
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
                                placeholder: if is_new_tab {
                                    "Search or type a URL, or select Terminal..."
                                } else {
                                    "Type a URL, search tabs, or > for commands..."
                                },
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
                                        dismiss_command_bar(is_open);
                                    } else if e.key() == Key::Enter {
                                        if should_open_typed_query_on_enter(open_target, nav_mode(), &q) {
                                            is_open.set(false);
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
                                    ResultItem::Stack { title, url, .. } => rsx! {
                                        div { class: result_content_row_class(),
                                            Favicon {
                                                favicon_url: String::new(),
                                                url: url.clone(),
                                                class: result_favicon_class().to_string(),
                                                globe_class: result_leading_icon_class().to_string(),
                                            }
                                            div { class: "flex min-w-0 flex-1 flex-col overflow-hidden",
                                                span { class: result_primary_text_class(), "{title}" }
                                                span { class: result_secondary_text_class(), "{url}" }
                                            }
                                        }
                                        span { class: result_trailing_slot_class(), "Stack" }
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
                                    ResultItem::Page { url, title, icon, favicon, shortcut } => rsx! {
                                        div { class: result_content_row_class(),
                                            if *favicon {
                                                Favicon {
                                                    favicon_url: String::new(),
                                                    url: url.clone(),
                                                    class: result_favicon_class().to_string(),
                                                    globe_class: result_leading_icon_class().to_string(),
                                                }
                                            } else {
                                                {page_icon(icon)}
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
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn looks_like_path(s: &str) -> bool {
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

fn page_icon(icon: &str) -> Element {
    let icon_class = result_leading_icon_class();
    match icon {
        "settings" => rsx! { Icon { class: icon_class,
            circle { cx: "12", cy: "12", r: "3" }
            path { d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" }
        } },
        "layers" => rsx! { Icon { class: icon_class,
            path { d: "M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z" }
            path { d: "m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65" }
            path { d: "m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65" }
        } },
        "clock" => rsx! { Icon { class: icon_class,
            circle { cx: "12", cy: "12", r: "10" }
            path { d: "M12 6v6l4 2" }
        } },
        "activity" => rsx! { Icon { class: icon_class,
            path { d: "M22 12h-4l-3 9L9 3l-3 9H2" }
        } },
        "sparkles" => rsx! { Icon { class: icon_class,
            path { d: "m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z" }
        } },
        "terminal" => rsx! { Icon { class: icon_class,
            path { d: "m4 17 6-6-6-6" }
            path { d: "M12 19h8" }
        } },
        _ => rsx! { Icon { class: icon_class,
            path { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }
            path { d: "M14 2v4a2 2 0 0 0 2 2h4" }
        } },
    }
}

fn emit_action(action: &str, value: &str) {
    emit_action_with_target(action, value, None);
}

fn emit_action_with_target(
    action: &str,
    value: &str,
    target: Option<vmux_command::open_target::OpenTarget>,
) {
    let _ = try_cef_bin_emit_rkyv(&CommandBarActionEvent {
        action: action.to_string(),
        value: value.to_string(),
        target,
    });
}

fn dismiss_command_bar(mut is_open: Signal<bool>) {
    if !is_open() {
        return;
    }
    is_open.set(false);
    emit_action("dismiss", "");
}

fn install_command_bar_outside_pointer_listener(is_open: Signal<bool>) -> bool {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return false;
    };
    if js_sys::Reflect::get(
        &document,
        &JsValue::from_str("_commandBarOutsidePointerBound"),
    )
    .map(|v| v.is_truthy())
    .unwrap_or(false)
    {
        return true;
    }

    let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
        if !is_open() {
            return;
        }
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let Some(shell) = document.get_element_by_id("command-bar-shell") else {
            return;
        };
        let Some(target) = event.target() else {
            return;
        };
        let inside_shell = target
            .dyn_ref::<web_sys::Node>()
            .is_some_and(|node| shell.contains(Some(node)));
        if inside_shell {
            return;
        }
        dismiss_command_bar(is_open);
    }) as Box<dyn FnMut(web_sys::Event)>);

    let options = web_sys::AddEventListenerOptions::new();
    options.set_capture(true);
    if document
        .add_event_listener_with_callback_and_add_event_listener_options(
            "pointerdown",
            closure.as_ref().unchecked_ref(),
            &options,
        )
        .is_err()
    {
        return false;
    }
    let _ = js_sys::Reflect::set(
        &document,
        &JsValue::from_str("_commandBarOutsidePointerBound"),
        &JsValue::TRUE,
    );
    closure.forget();
    true
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

    // Guard against double-binding.
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
                let s = input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) as usize;
                let v = input2.value();
                let new_val = format!("{}{}", &v[..s], &v[(s + 1).min(v.len())..]);
                input2.set_value(&new_val);
                let _ = input2.set_selection_range(s as u32, s as u32);
                dispatch_input_event(&input2);
            }
            CtrlEditAction::Backspace => {
                let s = input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) as usize;
                if s > 0 {
                    let v = input2.value();
                    let new_val = format!("{}{}", &v[..s - 1], &v[s..]);
                    input2.set_value(&new_val);
                    let _ = input2.set_selection_range((s - 1) as u32, (s - 1) as u32);
                    dispatch_input_event(&input2);
                }
            }
            CtrlEditAction::DeleteWord => {
                let s = input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) as usize;
                let v = input2.value();
                let bytes = v.as_bytes();
                let mut i = s.saturating_sub(1);
                while i > 0 && bytes[i - 1] == b' ' {
                    i -= 1;
                }
                while i > 0 && bytes[i - 1] != b' ' {
                    i -= 1;
                }
                let new_val = format!("{}{}", &v[..i], &v[s..]);
                input2.set_value(&new_val);
                let _ = input2.set_selection_range(i as u32, i as u32);
                dispatch_input_event(&input2);
            }
            CtrlEditAction::DeleteToBeginning => {
                let s = input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) as usize;
                let v = input2.value();
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

fn emit_command_bar_size() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id("command-bar-shell") else {
        return;
    };
    let shell: web_sys::HtmlElement = el.unchecked_into();
    let document_width = document
        .document_element()
        .map(|el| el.scroll_width())
        .unwrap_or(0);
    let body_width = document.body().map(|body| body.scroll_width()).unwrap_or(0);
    let result_list_extra_height = command_bar_results_extra_height(&document);
    let width = shell
        .offset_width()
        .max(shell.scroll_width())
        .max(document_width)
        .max(body_width)
        .max(1) as u32;
    let height = shell
        .offset_height()
        .max(shell.scroll_height() + result_list_extra_height)
        .max(1) as u32;
    let _ = try_cef_bin_emit_rkyv(&CommandBarSizeEvent { width, height });
}

fn command_bar_results_extra_height(document: &web_sys::Document) -> i32 {
    let Some(el) = document.get_element_by_id("command-bar-results") else {
        return 0;
    };
    let list: web_sys::HtmlElement = el.clone().unchecked_into();
    let max_outer_height = web_sys::window()
        .and_then(|window| window.get_computed_style(&el).ok().flatten())
        .and_then(|style| style.get_property_value("max-height").ok())
        .and_then(|value| css_px_value(&value))
        .map(|height| height.ceil() as i32);
    let border_height = (list.offset_height() - list.client_height()).max(0);
    let natural_outer_height = list.scroll_height() + border_height;
    let ideal_outer_height = max_outer_height
        .map(|height| natural_outer_height.min(height))
        .unwrap_or(natural_outer_height);
    (ideal_outer_height - list.offset_height()).max(0)
}

fn css_px_value(value: &str) -> Option<f64> {
    let value = value.trim().strip_suffix("px")?.parse::<f64>().ok()?;
    value.is_finite().then_some(value.max(0.0))
}

fn schedule_command_bar_size_emit() {
    emit_command_bar_size();
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::wrap(Box::new(move || {
        emit_command_bar_size();
    }) as Box<dyn FnMut()>);
    let _ = window.request_animation_frame(callback.as_ref().unchecked_ref());
    callback.forget();
}

fn install_command_bar_size_observer() -> bool {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return false;
    };
    let Some(el) = document.get_element_by_id("command-bar-shell") else {
        return false;
    };
    schedule_command_bar_size_emit();
    let callback = Closure::wrap(Box::new(move |_entries: JsValue| {
        schedule_command_bar_size_emit();
    }) as Box<dyn FnMut(JsValue)>);
    let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) else {
        return false;
    };
    observer.observe(&el);
    std::mem::forget(observer);
    callback.forget();
    true
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

/// Dispatch a synthetic "input" event so Dioxus picks up value changes.
fn dispatch_input_event(el: &web_sys::HtmlInputElement) {
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    if let Ok(evt) = web_sys::Event::new_with_event_init_dict("input", &init) {
        let _ = el.dispatch_event(&evt);
    }
}
