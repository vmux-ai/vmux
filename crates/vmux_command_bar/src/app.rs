#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command_bar::event::{
    COMMAND_BAR_OPEN_EVENT, CommandBarActionEvent, CommandBarCommandEntry, CommandBarOpenEvent,
    CommandBarTab, PATH_COMPLETE_RESPONSE, PathCompleteRequest, PathCompleteResponse, PathEntry,
};
use vmux_ui::components::icon::Icon;
use vmux_ui::hooks::{try_cef_emit_serde, use_event_listener, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[derive(Clone, PartialEq)]
enum ResultItem {
    Terminal {
        path: String,
    },
    Tab {
        title: String,
        url: String,
        pane_id: u64,
        tab_index: usize,
    },
    Command {
        id: String,
        name: String,
        shortcut: String,
    },
    Navigate {
        url: String,
    },
}

use vmux_command_bar::event::looks_like_path;

fn filter_results(
    query: &str,
    tabs: &[CommandBarTab],
    commands: &[CommandBarCommandEntry],
    new_tab: bool,
    current_url: &str,
) -> Vec<ResultItem> {
    let current_is_terminal = current_url.starts_with("vmux://terminal");
    let show_terminal = new_tab || !current_is_terminal;
    let q = query.trim();
    if q.is_empty() {
        let mut items: Vec<ResultItem> = Vec::new();
        items.push(ResultItem::Navigate { url: String::new() });
        if show_terminal {
            items.push(ResultItem::Terminal {
                path: String::new(),
            });
        }
        items.extend(tabs.iter().map(|t| ResultItem::Tab {
            title: t.title.clone(),
            url: t.url.clone(),
            pane_id: t.pane_id,
            tab_index: t.tab_index,
        }));
        items.extend(commands.iter().map(|c| ResultItem::Command {
            id: c.id.clone(),
            name: c.name.clone(),
            shortcut: c.shortcut.clone(),
        }));
        return items;
    }

    let starts_with_cmd = q.starts_with('>');
    let search = if starts_with_cmd { q[1..].trim() } else { q };
    let search_lower = search.to_lowercase();

    let mut items = Vec::new();

    let is_path = looks_like_path(search);

    if !starts_with_cmd && is_path {
        items.push(ResultItem::Terminal {
            path: search.to_string(),
        });
    }

    if !starts_with_cmd && !is_path && show_terminal && "terminal".contains(&search_lower) {
        items.push(ResultItem::Terminal {
            path: String::new(),
        });
    }

    // Commands always shown when > prefix
    if starts_with_cmd {
        for c in commands {
            if search.is_empty()
                || c.name.to_lowercase().contains(&search_lower)
                || c.id.contains(&search_lower)
            {
                items.push(ResultItem::Command {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    shortcut: c.shortcut.clone(),
                });
            }
        }
    }

    // Tabs (always for non-command mode; as fallback when > has text)
    if !starts_with_cmd || !search.is_empty() {
        for t in tabs {
            if search.is_empty()
                || t.title.to_lowercase().contains(&search_lower)
                || t.url.to_lowercase().contains(&search_lower)
            {
                items.push(ResultItem::Tab {
                    title: t.title.clone(),
                    url: t.url.clone(),
                    pane_id: t.pane_id,
                    tab_index: t.tab_index,
                });
            }
        }
    }

    // Commands (non-command mode)
    if !starts_with_cmd {
        for c in commands {
            if c.name.to_lowercase().contains(&search_lower) || c.id.contains(&search_lower) {
                items.push(ResultItem::Command {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    shortcut: c.shortcut.clone(),
                });
            }
        }
    }

    // Navigate/search as fallback
    if !search.is_empty() {
        items.push(ResultItem::Navigate {
            url: search.to_string(),
        });
    }

    items
}

fn emit_action(action: &str, value: &str, new_tab: bool) {
    let _ = try_cef_emit_serde(&CommandBarActionEvent {
        action: action.to_string(),
        value: value.to_string(),
        new_tab,
    });
}

#[component]
pub fn App() -> Element {
    use_theme();
    let mut state = use_signal(CommandBarOpenEvent::default);
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    let mut new_tab = use_signal(|| false);
    let mut is_open = use_signal(|| false);
    let mut nav_mode = use_signal(|| false);

    let mut path_completions = use_signal(Vec::<PathEntry>::new);

    let _listener =
        use_event_listener::<CommandBarOpenEvent, _>(COMMAND_BAR_OPEN_EVENT, move |data| {
            query.set(data.url.clone());
            selected.set(0);
            nav_mode.set(false);
            new_tab.set(data.new_tab);
            state.set(data);
            is_open.set(true);
        });

    let _path_listener =
        use_event_listener::<PathCompleteResponse, _>(PATH_COMPLETE_RESPONSE, move |data| {
            path_completions.set(data.completions);
        });

    use_effect(move || {
        let q = query();
        if !looks_like_path(q.trim()) {
            path_completions.set(Vec::new());
            return;
        }
        let _ = try_cef_emit_serde(&PathCompleteRequest {
            query: q.trim().to_string(),
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

    let CommandBarOpenEvent {
        url: current_url,
        tabs,
        commands,
        new_tab: _,
    } = state();
    let q = query();
    let is_new_tab = new_tab();
    let results = {
        let mut r = filter_results(&q, &tabs, &commands, is_new_tab, &current_url);
        let completions = path_completions();
        if !completions.is_empty() {
            let path_items: Vec<ResultItem> = completions
                .iter()
                .filter(|e| e.is_dir)
                .take(5)
                .map(|e| ResultItem::Terminal {
                    path: e.full_path.clone(),
                })
                .collect();
            let typed_terminal = r
                .iter()
                .find(|item| matches!(item, ResultItem::Terminal { path } if !path.is_empty()))
                .cloned();
            r.retain(|item| !matches!(item, ResultItem::Terminal { path } if !path.is_empty()));
            let mut combined = Vec::new();
            if let Some(ref entry @ ResultItem::Terminal { path: ref tp }) = typed_terminal
                && !path_items
                    .iter()
                    .any(|item| matches!(item, ResultItem::Terminal { path } if path == tp))
            {
                combined.push(entry.clone());
            }
            combined.extend(path_items);
            combined.extend(r);
            combined
        } else {
            r
        }
    };
    let sel = selected().min(results.len().saturating_sub(1));
    let active_item = results.get(sel).cloned();
    let nav = nav_mode();
    let display_text = if nav {
        match &active_item {
            Some(ResultItem::Command { name, .. }) => format!("> {name}"),
            Some(ResultItem::Navigate { url }) => url.clone(),
            Some(ResultItem::Tab { url, .. }) => url.clone(),
            Some(ResultItem::Terminal { path }) if path.is_empty() => "Terminal".to_string(),
            Some(ResultItem::Terminal { path }) => path.clone(),
            None => q.clone(),
        }
    } else {
        q.clone()
    };

    let ghost_text = {
        let q_trimmed = q.trim();
        let completions = path_completions();
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
        let nt = new_tab();
        match item {
            ResultItem::Terminal { path } => {
                emit_action("terminal", path, nt);
            }
            ResultItem::Tab {
                pane_id, tab_index, ..
            } => {
                emit_action("switch_tab", &format!("{pane_id}:{tab_index}"), nt);
            }
            ResultItem::Command { id, .. } => {
                emit_action("command", id, nt);
            }
            ResultItem::Navigate { url } => {
                if !url.is_empty() {
                    emit_action("navigate", url, nt);
                }
            }
        }
    };

    if !is_open() {
        return rsx! { div { class: "h-full w-full" } };
    }

    rsx! {
        div {
            class: "flex h-full w-full items-start justify-center pt-[15%]",
            onclick: move |_| { is_open.set(false); emit_action("dismiss", "", new_tab()); },
            div {
                class: "relative flex w-full max-w-xl flex-col overflow-hidden rounded-2xl border border-white/20 bg-white/10 shadow-2xl backdrop-blur-2xl backdrop-saturate-150",
                onclick: move |e| { e.stop_propagation(); },
                // Inner glow overlay
                div { class: "pointer-events-none absolute inset-0 rounded-2xl bg-gradient-to-br from-white/20 to-transparent" }
                div { class: "p-2",
                    div { class: "flex items-center gap-2 rounded-lg bg-white/5 px-3",
                        {
                            let icon_class = "h-4 w-4 shrink-0 text-muted-foreground";
                            let (is_command, is_path, is_url) = if nav {
                                match &active_item {
                                    Some(ResultItem::Command { .. }) => (true, false, false),
                                    Some(ResultItem::Terminal { path }) if path.is_empty() => (true, false, false),
                                    Some(ResultItem::Terminal { .. }) => (false, true, false),
                                    Some(ResultItem::Tab { .. }) => (false, false, true),
                                    Some(ResultItem::Navigate { url }) => {
                                        let is_u = url.contains("://") || (url.contains('.') && !url.contains(' '));
                                        (false, false, is_u)
                                    }
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
                        div { class: "relative flex-1",
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
                                class: "w-full py-2.5 text-base text-foreground bg-transparent outline-none placeholder:text-muted-foreground",
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
                                        is_open.set(false);
                                        emit_action("dismiss", "", new_tab());
                                    } else if e.key() == Key::Enter {
                                        if let Some(item) = results.get(sel) {
                                            execute(item);
                                        } else if !q.is_empty() {
                                            emit_action("navigate", &q, new_tab());
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
                if !results.is_empty() {
                    div { class: "max-h-80 overflow-y-auto border-t border-border p-1",
                        for (i, item) in results.iter().enumerate() {
                            div {
                                key: "{i}",
                                id: "command-bar-item-{i}",
                                class: if i == sel {
                                    "flex cursor-pointer items-center justify-between rounded-lg bg-white/10 px-3 py-2"
                                } else {
                                    "flex cursor-pointer items-center justify-between rounded-lg px-3 py-2 hover:bg-white/5"
                                },
                                onclick: {
                                    let item = item.clone();
                                    move |_| { execute(&item); }
                                },
                                match item {
                                    ResultItem::Terminal { path } => rsx! {
                                        div { class: "flex items-center gap-2",
                                            span { class: "shrink-0 text-base text-muted-foreground", ">_" }
                                            if path.is_empty() {
                                                if is_new_tab {
                                                    span { class: "text-base text-foreground", "Open Terminal" }
                                                } else {
                                                    span { class: "text-base text-foreground", "Open Terminal in New Tab" }
                                                }
                                            } else {
                                                span { class: "text-base text-foreground", "Open in Terminal" }
                                                span { class: "ml-1 text-sm text-muted-foreground", "{path}" }
                                            }
                                        }
                                    },
                                    ResultItem::Tab { title, url, .. } => rsx! {
                                        div { class: "flex min-w-0 flex-col",
                                            span { class: "truncate text-base text-foreground", "{title}" }
                                            span { class: "truncate text-sm text-muted-foreground", "{url}" }
                                        }
                                        span { class: "ml-2 shrink-0 text-sm text-muted-foreground", "Tab" }
                                    },
                                    ResultItem::Command { name, shortcut, .. } => rsx! {
                                        div { class: "flex items-center gap-2",
                                            span { class: "shrink-0 text-base text-muted-foreground", ">_" }
                                            span { class: "text-base text-foreground", "{name}" }
                                        }
                                        span { class: "ml-2 shrink-0 rounded bg-muted px-1.5 py-0.5 text-sm text-muted-foreground", "{shortcut}" }
                                    },
                                    ResultItem::Navigate { url } => rsx! {
                                        div { class: "flex items-center gap-2",
                                            Icon { class: "h-4 w-4 shrink-0 text-muted-foreground",
                                                circle { cx: "11", cy: "11", r: "8" }
                                                path { d: "m21 21-4.3-4.3" }
                                            }
                                            if url.is_empty() {
                                                span { class: "text-base text-foreground", "Search" }
                                            } else {
                                                span { class: "min-w-0 break-all text-base text-foreground", "Search \"{url}\"" }
                                            }
                                        }
                                        if !url.is_empty() {
                                            span { class: "ml-2 shrink-0 text-sm text-muted-foreground", "\u{21b5}" }
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
    let _ = input.set_selection_range(len, len);

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
        if !e.ctrl_key() {
            return;
        }
        let code = e.code();
        let action = match code.as_str() {
            "KeyA" => "home",
            "KeyE" => "end",
            "KeyF" => "fwd",
            "KeyB" => "back",
            "KeyD" => "del",
            "KeyH" => "bksp",
            "KeyW" => "delw",
            "KeyU" => "delbeg",
            "KeyC" | "KeyN" | "KeyJ" | "KeyP" | "KeyK" => {
                e.prevent_default();
                return;
            }
            _ => return,
        };
        e.prevent_default();
        e.stop_immediate_propagation();

        match action {
            "home" => {
                let _ = input2.set_selection_range(0, 0);
            }
            "end" => {
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
            "fwd" => {
                let p = (input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) + 1)
                    .min(input2.value().len() as u32);
                let _ = input2.set_selection_range(p, p);
            }
            "back" => {
                let p = input2
                    .selection_start()
                    .unwrap_or(Some(0))
                    .unwrap_or(0)
                    .saturating_sub(1);
                let _ = input2.set_selection_range(p, p);
            }
            "del" => {
                let s = input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) as usize;
                let v = input2.value();
                let new_val = format!("{}{}", &v[..s], &v[(s + 1).min(v.len())..]);
                input2.set_value(&new_val);
                let _ = input2.set_selection_range(s as u32, s as u32);
                dispatch_input_event(&input2);
            }
            "bksp" => {
                let s = input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) as usize;
                if s > 0 {
                    let v = input2.value();
                    let new_val = format!("{}{}", &v[..s - 1], &v[s..]);
                    input2.set_value(&new_val);
                    let _ = input2.set_selection_range((s - 1) as u32, (s - 1) as u32);
                    dispatch_input_event(&input2);
                }
            }
            "delw" => {
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
            "delbeg" => {
                let s = input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) as usize;
                let v = input2.value();
                input2.set_value(&v[s..]);
                let _ = input2.set_selection_range(0, 0);
                dispatch_input_event(&input2);
            }
            _ => {}
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

/// Dispatch a synthetic "input" event so Dioxus picks up value changes.
fn dispatch_input_event(el: &web_sys::HtmlInputElement) {
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    if let Ok(evt) = web_sys::Event::new_with_event_init_dict("input", &init) {
        let _ = el.dispatch_event(&evt);
    }
}
