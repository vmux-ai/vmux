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
) -> Vec<ResultItem> {
    let q = query.trim();
    if q.is_empty() {
        let mut items: Vec<ResultItem> = tabs
            .iter()
            .map(|t| ResultItem::Tab {
                title: t.title.clone(),
                url: t.url.clone(),
                pane_id: t.pane_id,
                tab_index: t.tab_index,
            })
            .collect();
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

    if !commands_only && !search.is_empty() {
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
    let mut is_open = use_signal(|| false);

    let _listener =
        use_event_listener::<CommandBarOpenEvent, _>(COMMAND_BAR_OPEN_EVENT, move |data| {
        query.set(data.url.clone());
        selected.set(0);
        state.set(data);
        is_open.set(true);
        // Focus input and install Ctrl shortcuts via web_sys.
        // Dioxus e.modifiers().ctrl() is unreliable in CEF OSR, so we
        // listen directly on the DOM in capture phase.
        focus_and_install_ctrl_bindings();
    });

    let CommandBarOpenEvent {
        url: _,
        tabs,
        commands,
    } = state();
    let q = query();
    let results = filter_results(&q, &tabs, &commands);
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
            ResultItem::Tab {
                pane_id, tab_index, ..
            } => {
                emit_action("switch_tab", &format!("{pane_id}:{tab_index}"));
            }
            ResultItem::Command { id, .. } => {
                emit_action("command", id);
            }
            ResultItem::Navigate { url } => {
                emit_action("navigate", url);
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
                class: "glass flex w-full max-w-xl flex-col rounded-lg shadow-2xl",
                onclick: move |e| { e.stop_propagation(); },
                div { class: "p-2",
                    input {
                        id: "command-bar-input",
                        r#type: "text",
                        class: "glass w-full rounded-lg px-3 py-2.5 text-base text-foreground outline-none placeholder:text-muted-foreground",
                        placeholder: "Type a URL, search tabs, or > for commands...",
                        value: "{q}",
                        autofocus: true,
                        oninput: move |e| {
                            query.set(e.value());
                            selected.set(0);
                        },
                        onkeydown: move |e| {
                            match e.key() {
                                Key::Escape => { is_open.set(false); emit_action("dismiss", ""); }
                                Key::ArrowDown => {
                                    e.prevent_default();
                                    let max = results.len().saturating_sub(1);
                                    selected.set((sel + 1).min(max));
                                }
                                Key::ArrowUp => {
                                    e.prevent_default();
                                    selected.set(sel.saturating_sub(1));
                                }
                                Key::Enter => {
                                    if let Some(item) = results.get(sel) {
                                        execute(item);
                                    } else if !q.is_empty() {
                                        emit_action("navigate", &q);
                                    }
                                }
                                _ => {}
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
                                    "glass flex cursor-pointer items-center justify-between rounded-lg px-3 py-2"
                                } else {
                                    "flex cursor-pointer items-center justify-between rounded-lg px-3 py-2 hover:bg-muted/50"
                                },
                                onclick: {
                                    let item = item.clone();
                                    move |_| { execute(&item); }
                                },
                                match item {
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
                                        span { class: "min-w-0 break-all text-base text-foreground", "Navigate to {url}" }
                                        span { class: "ml-2 shrink-0 text-sm text-muted-foreground", "\u{21b5}" }
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
/// via a capture-phase keydown listener (web_sys).
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
            "KeyN" | "KeyJ" => "down",
            "KeyP" | "KeyK" => "up",
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
            "down" => {
                let _ = input2.dispatch_event(
                    &web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(
                        "keydown",
                        web_sys::KeyboardEventInit::new().key("ArrowDown").bubbles(true),
                    )
                    .unwrap(),
                );
            }
            "up" => {
                let _ = input2.dispatch_event(
                    &web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(
                        "keydown",
                        web_sys::KeyboardEventInit::new().key("ArrowUp").bubbles(true),
                    )
                    .unwrap(),
                );
            }
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
                let _ = input2.dispatch_event(
                    &web_sys::Event::new_with_event_init_dict(
                        "input",
                        web_sys::EventInit::new().bubbles(true),
                    )
                    .unwrap(),
                );
            }
            "bksp" => {
                let s = input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) as usize;
                if s > 0 {
                    let v = input2.value();
                    let new_val = format!("{}{}", &v[..s - 1], &v[s..]);
                    input2.set_value(&new_val);
                    let _ = input2.set_selection_range((s - 1) as u32, (s - 1) as u32);
                    let _ = input2.dispatch_event(
                        &web_sys::Event::new_with_event_init_dict(
                            "input",
                            web_sys::EventInit::new().bubbles(true),
                        )
                        .unwrap(),
                    );
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
                let _ = input2.dispatch_event(
                    &web_sys::Event::new_with_event_init_dict(
                        "input",
                        web_sys::EventInit::new().bubbles(true),
                    )
                    .unwrap(),
                );
            }
            "delbeg" => {
                let s = input2.selection_start().unwrap_or(Some(0)).unwrap_or(0) as usize;
                let v = input2.value();
                input2.set_value(&v[s..]);
                let _ = input2.set_selection_range(0, 0);
                let _ = input2.dispatch_event(
                    &web_sys::Event::new_with_event_init_dict(
                        "input",
                        web_sys::EventInit::new().bubbles(true),
                    )
                    .unwrap(),
                );
            }
            _ => {}
        }
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);

    let target: &web_sys::EventTarget = input.as_ref();
    let mut opts = web_sys::AddEventListenerOptions::new();
    opts.capture(true);
    let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
        "keydown",
        closure.as_ref().unchecked_ref(),
        &opts,
    );
    closure.forget();
}
