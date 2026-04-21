#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_command_palette::event::{
    PaletteActionEvent, PaletteCommandEntry, PaletteOpenEvent, PaletteTab, PALETTE_OPEN_EVENT,
};
use vmux_ui::hooks::{try_cef_emit_serde, use_event_listener};

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
    tabs: &[PaletteTab],
    commands: &[PaletteCommandEntry],
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
    let _ = try_cef_emit_serde(&PaletteActionEvent {
        action: action.to_string(),
        value: value.to_string(),
    });
}

#[component]
pub fn App() -> Element {
    let mut state = use_signal(PaletteOpenEvent::default);
    let mut query = use_signal(String::new);
    let mut selected = use_signal(|| 0usize);
    let mut is_open = use_signal(|| false);

    let _listener = use_event_listener::<PaletteOpenEvent, _>(PALETTE_OPEN_EVENT, move |data| {
        query.set(data.url.clone());
        selected.set(0);
        state.set(data);
        is_open.set(true);
        // Focus input and install raw JS keydown listener for Ctrl bindings.
        // Dioxus e.modifiers().ctrl() is unreliable in CEF OSR, so we use
        // event.ctrlKey directly from the DOM KeyboardEvent in capture phase.
        document::eval(
            r#"setTimeout(() => {
  var el = document.getElementById('palette-input');
  if (!el) return;
  el.focus();
  el.select();
  if (el._ctrlBound) return;
  el._ctrlBound = true;
  el.addEventListener('keydown', function(e) {
    if (!e.ctrlKey) return;
    var c = e.code;
    var actions = {KeyN:'down',KeyJ:'down',KeyP:'up',KeyK:'up',KeyA:'home',KeyE:'end',KeyF:'fwd',KeyB:'back',KeyD:'del',KeyH:'bksp',KeyW:'delw',KeyU:'delbeg'};
    var a = actions[c];
    if (!a) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    if (a==='down') { el.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true})); return; }
    if (a==='up') { el.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true})); return; }
    if (a==='home') { el.setSelectionRange(0,0); return; }
    if (a==='end') { el.setSelectionRange(el.value.length,el.value.length); return; }
    if (a==='fwd') { var p=Math.min(el.selectionStart+1,el.value.length); el.setSelectionRange(p,p); return; }
    if (a==='back') { var p=Math.max(el.selectionStart-1,0); el.setSelectionRange(p,p); return; }
    if (a==='del') { var s=el.selectionStart,v=el.value; el.value=v.slice(0,s)+v.slice(s+1); el.setSelectionRange(s,s); el.dispatchEvent(new Event('input',{bubbles:true})); return; }
    if (a==='bksp') { var s=el.selectionStart; if(s>0){var v=el.value;el.value=v.slice(0,s-1)+v.slice(s);el.setSelectionRange(s-1,s-1);el.dispatchEvent(new Event('input',{bubbles:true}));} return; }
    if (a==='delw') { var s=el.selectionStart,v=el.value,i=s-1; while(i>0&&v[i-1]===' ')i--; while(i>0&&v[i-1]!==' ')i--; el.value=v.slice(0,i)+v.slice(s); el.setSelectionRange(i,i); el.dispatchEvent(new Event('input',{bubbles:true})); return; }
    if (a==='delbeg') { var s=el.selectionStart; el.value=el.value.slice(s); el.setSelectionRange(0,0); el.dispatchEvent(new Event('input',{bubbles:true})); return; }
  }, true);
}, 0);"#,
        );
    });

    let PaletteOpenEvent {
        url: _,
        tabs,
        commands,
    } = state();
    let q = query();
    let results = filter_results(&q, &tabs, &commands);
    let sel = selected().min(results.len().saturating_sub(1));

    // Auto-scroll selected item into view when selection changes
    use_effect(move || {
        let s = selected();
        document::eval(&format!(
            "document.getElementById('palette-item-{s}')?.scrollIntoView({{block:'nearest'}})"
        ));
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
                class: "glass flex w-full max-w-xl flex-col rounded-xl shadow-2xl",
                onclick: move |e| { e.stop_propagation(); },
                div { class: "p-2",
                    input {
                        id: "palette-input",
                        r#type: "text",
                        class: "w-full rounded-lg bg-muted px-3 py-2.5 text-base text-foreground outline-none placeholder:text-muted-foreground",
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
                                    let max = results.len().saturating_sub(1);
                                    selected.set((sel + 1).min(max));
                                }
                                Key::ArrowUp => {
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
                                id: "palette-item-{i}",
                                class: if i == sel {
                                    "flex cursor-pointer items-center justify-between rounded-lg bg-muted px-3 py-2"
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
