#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command_bar::event::{
    CommandBarActionEvent, CommandBarCommandEntry, CommandBarOpenEvent, CommandBarTab,
    COMMAND_BAR_OPEN_EVENT,
};
use vmux_ui::hooks::{try_cef_emit_serde, use_event_listener, use_theme};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[derive(Clone, PartialEq)]
enum ResultItem {
    Terminal,
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

fn filter_results(
    query: &str,
    tabs: &[CommandBarTab],
    commands: &[CommandBarCommandEntry],
    new_tab: bool,
) -> Vec<ResultItem> {
    let q = query.trim();
    if q.is_empty() {
        let mut items: Vec<ResultItem> = Vec::new();
        if new_tab {
            items.push(ResultItem::Navigate { url: String::new() });
            items.push(ResultItem::Terminal);
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

    let commands_only = q.starts_with('>');
    let search = if commands_only { q[1..].trim() } else { q };
    let search_lower = search.to_lowercase();

    let mut items = Vec::new();

    if !commands_only && new_tab {
        items.push(ResultItem::Navigate { url: search.to_string() });
        if "terminal".contains(&search_lower) {
            items.push(ResultItem::Terminal);
        }
    }

    if !commands_only {
        for t in tabs {
            if t.title.to_lowercase().contains(&search_lower)
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

    for c in commands {
        if c.name.to_lowercase().contains(&search_lower) || c.id.contains(&search_lower) {
            items.push(ResultItem::Command {
                id: c.id.clone(),
                name: c.name.clone(),
                shortcut: c.shortcut.clone(),
            });
        }
    }

    if !commands_only && !search.is_empty() && !new_tab {
        items.push(ResultItem::Navigate {
            url: search.to_string(),
        });
    }

    items
}

fn emit_action(action: &str, value: &str) {
    let _ = try_cef_emit_serde(&CommandBarActionEvent {
        action: action.to_string(),
        value: value.to_string(),
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

    let _listener =
        use_event_listener::<CommandBarOpenEvent, _>(COMMAND_BAR_OPEN_EVENT, move |data| {
            query.set(data.url.clone());
            selected.set(0);
            new_tab.set(data.new_tab);
            state.set(data);
            is_open.set(true);
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
        url: _,
        tabs,
        commands,
        new_tab: _,
    } = state();
    let q = query();
    let is_new_tab = new_tab();
    let results = filter_results(&q, &tabs, &commands, is_new_tab);
    let sel = selected().min(results.len().saturating_sub(1));

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
            ResultItem::Terminal => {
                emit_action("terminal", "");
            }
            ResultItem::Tab {
                pane_id, tab_index, ..
            } => {
                emit_action("switch_tab", &format!("{pane_id}:{tab_index}"));
            }
            ResultItem::Command { id, .. } => {
                emit_action("command", id);
            }
            ResultItem::Navigate { url } => {
                if !url.is_empty() {
                    emit_action("navigate", url);
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
            onclick: move |_| { is_open.set(false); emit_action("dismiss", ""); },
            div {
                class: "flex w-full max-w-xl flex-col rounded-xl border border-white/20 bg-white/10 shadow-2xl shadow-black/40 ring-1 ring-white/10 backdrop-blur-2xl backdrop-saturate-150",
                onclick: move |e| { e.stop_propagation(); },
                div { class: "p-2",
                    input {
                        id: "command-bar-input",
                        r#type: "text",
                        class: "w-full rounded-lg bg-white/5 px-3 py-2.5 text-base text-foreground outline-none placeholder:text-muted-foreground",
                        placeholder: if is_new_tab {
                            "Search or type a URL, or select Terminal..."
                        } else {
                            "Type a URL, search tabs, or > for commands..."
                        },
                        value: "{q}",
                        autofocus: true,
                        oninput: move |e| {
                            query.set(e.value());
                            selected.set(0);
                        },
                        onkeydown: move |e| {
                            let ctrl = e.modifiers().contains(Modifiers::CONTROL);
                            let go_down = e.key() == Key::ArrowDown
                                || (ctrl && matches!(e.code(), Code::KeyN | Code::KeyJ));
                            let go_up = e.key() == Key::ArrowUp
                                || (ctrl && matches!(e.code(), Code::KeyP | Code::KeyK));

                            if go_down {
                                e.prevent_default();
                                let max = results.len().saturating_sub(1);
                                selected.set((sel + 1).min(max));
                            } else if go_up {
                                e.prevent_default();
                                selected.set(sel.saturating_sub(1));
                            } else if e.key() == Key::Escape {
                                is_open.set(false);
                                emit_action("dismiss", "");
                            } else if e.key() == Key::Enter {
                                if let Some(item) = results.get(sel) {
                                    execute(item);
                                } else if !q.is_empty() {
                                    emit_action("navigate", &q);
                                }
                            }
                        },
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
                                    ResultItem::Terminal => rsx! {
                                        div { class: "flex items-center gap-2",
                                            span { class: "shrink-0 text-base text-muted-foreground", ">_" }
                                            span { class: "text-base text-foreground", "Terminal" }
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
                                        span { class: "text-base text-foreground", "{name}" }
                                        span { class: "ml-2 shrink-0 rounded bg-muted px-1.5 py-0.5 text-sm text-muted-foreground", "{shortcut}" }
                                    },
                                    ResultItem::Navigate { url } => rsx! {
                                        div { class: "flex items-center gap-2",
                                            span { class: "shrink-0 text-base text-muted-foreground", "\u{1F50D}" }
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

// ---------------------------------------------------------------------------
// Ctrl shortcuts helper
// ---------------------------------------------------------------------------

/// Focus the command-bar input and install emacs-style Ctrl shortcuts
/// (cursor movement, deletion) via a capture-phase keydown listener (web_sys).
///
/// Ctrl+N/P/J/K (up/down navigation) are handled in the Dioxus `onkeydown`
/// handler so they can update selection signals directly.
fn focus_and_install_ctrl_bindings() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = document.get_element_by_id("command-bar-input") else {
        return;
    };
    let input: web_sys::HtmlInputElement = el.unchecked_into();
    input.focus().ok();
    input.select();

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
            _ => return,
        };
        e.prevent_default();
        e.stop_immediate_propagation();

        match action {
            "home" => {
                let _ = input2.set_selection_range(0, 0);
            }
            "end" => {
                let len = input2.value().len() as u32;
                let _ = input2.set_selection_range(len, len);
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
