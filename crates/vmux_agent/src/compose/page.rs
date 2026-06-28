#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use vmux_core::event::AgentComposeSubmitEvent;
use vmux_terminal::matrix_rain::MatrixRain;
use vmux_ui::agent_accent::agent_accent;
use vmux_ui::favicon::Favicon;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_theme};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

const PROMPT_ID: &str = "agent-compose-prompt";

const EXAMPLES: &[&str] = &[
    "Find me a hotel with AC near Paris for this weekend",
    "Find the best flight from Paris to Tokyo next month",
    "Build a landing site for my new restaurant — make it themeable",
    "Open a PR for my staged changes",
];

#[component]
pub fn Page() -> Element {
    use_theme();
    let segment = use_hook(read_segment);
    let accent = agent_accent(&segment);
    let label = title_case(&segment);
    let favicon_url = format!("vmux://agent/{segment}/cli/");
    let words = vec![label.to_uppercase()];

    let mut draft = use_signal(String::new);
    let mut committed = use_signal(|| false);
    let draft_empty = draft.read().is_empty();
    let caret = if draft_empty {
        "caret-transparent"
    } else {
        "caret-current"
    };

    let emit_segment = segment.clone();
    let mut submit = move |run: bool| {
        committed.set(run);
        let _ = try_cef_bin_emit_rkyv(&AgentComposeSubmitEvent {
            agent: emit_segment.clone(),
            text: draft.peek().clone(),
            submit: run,
        });
    };

    rsx! {
        div {
            class: "relative h-screen w-screen overflow-hidden bg-term-bg text-foreground",
            MatrixRain { accent_rgb: accent.rain_rgb.to_string(), words }
            div {
                class: "relative z-10 flex h-full w-full items-center justify-center",
                div {
                    class: "flex w-full max-w-xl flex-col gap-3 px-4",
                    div {
                        class: "flex items-center gap-3 self-center rounded-2xl bg-black/40 px-5 py-4 ring-1 ring-inset ring-white/10 backdrop-blur-md",
                        div {
                            class: "flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-white/[0.06] ring-1 ring-inset ring-white/10",
                            Favicon {
                                favicon_url: "".to_string(),
                                url: favicon_url.clone(),
                                class: "h-5 w-5 shrink-0 rounded object-contain".to_string(),
                                globe_class: "h-5 w-5 text-muted-foreground".to_string(),
                            }
                        }
                        div {
                            div { class: "text-sm font-semibold {accent.accent_text}", "Start {label}" }
                            div {
                                class: "flex items-center gap-1.5 text-xs text-muted-foreground",
                                span { class: "font-mono", "> compose a first prompt" }
                            }
                        }
                    }
                    div {
                        class: "pointer-events-auto rounded-2xl bg-black/40 p-3 ring-1 ring-inset ring-white/10 backdrop-blur-md focus-within:ring-white/25",
                        div {
                            class: "relative",
                            textarea {
                                id: "{PROMPT_ID}",
                                rows: "4",
                                autofocus: true,
                                onmounted: move |_| focus_prompt(),
                                class: "relative z-10 w-full resize-none border-0 bg-transparent p-0 text-sm leading-relaxed text-foreground outline-none {caret}",
                                value: "{draft}",
                                oninput: move |e: Event<FormData>| draft.set(e.value()),
                                onkeydown: move |e: Event<KeyboardData>| {
                                    let data = e.data();
                                    let Some(raw) = data.downcast::<web_sys::KeyboardEvent>() else {
                                        return;
                                    };
                                    if raw.is_composing() {
                                        return;
                                    }
                                    match raw.key().as_str() {
                                        "Enter" if !raw.shift_key() => {
                                            e.prevent_default();
                                            submit(true);
                                        }
                                        "Escape" => {
                                            e.prevent_default();
                                            submit(false);
                                        }
                                        _ => {}
                                    }
                                },
                            }
                            if draft_empty {
                                PromptGhost { accent_bg: accent.accent_bg.to_string() }
                            }
                        }
                        div {
                            class: "mt-1 flex items-center justify-between px-0.5 text-[10px] text-muted-foreground/70",
                            span { "Enter to start · Shift+Enter for newline · Esc to skip" }
                            if committed() {
                                span { class: "uppercase tracking-wide {accent.accent_text}", "starting…" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn read_segment() -> String {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .map(|p| {
            p.trim_matches('/')
                .split('/')
                .next()
                .unwrap_or("")
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "agent".to_string())
}

fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn focus_prompt() {
    focus_prompt_now();
    if let Some(win) = web_sys::window() {
        let cb = Closure::once_into_js(focus_prompt_now);
        let _ = win.request_animation_frame(cb.unchecked_ref());
    }
}

fn focus_prompt_now() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(PROMPT_ID))
        && let Ok(html) = el.dyn_into::<web_sys::HtmlElement>()
    {
        let _ = html.focus();
    }
}

/// Animated ghost that types out example prompts with a blinking caret while the
/// box is empty. Cycles through [`EXAMPLES`]; stops the moment the user types.
#[component]
fn PromptGhost(accent_bg: String) -> Element {
    let ex_idx = use_signal(|| 0usize);
    let typed = use_signal(|| 0usize);
    let cb: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = use_hook(|| Rc::new(RefCell::new(None)));
    let timer: Rc<RefCell<Option<i32>>> = use_hook(|| Rc::new(RefCell::new(None)));
    use_effect({
        let cb = cb.clone();
        let timer = timer.clone();
        move || start_typewriter(ex_idx, typed, cb.clone(), timer.clone())
    });
    use_drop({
        let cb = cb.clone();
        let timer = timer.clone();
        move || {
            if let Some(id) = timer.borrow_mut().take()
                && let Some(win) = web_sys::window()
            {
                win.clear_interval_with_handle(id);
            }
            *cb.borrow_mut() = None;
        }
    });
    let example = EXAMPLES[ex_idx() % EXAMPLES.len()];
    let full = example.chars().count();
    let shown: String = example.chars().take(typed().min(full)).collect();
    rsx! {
        div {
            class: "pointer-events-none absolute inset-0 z-0 whitespace-pre-wrap break-words text-sm leading-relaxed text-muted-foreground/40",
            "{shown}"
            span {
                class: "ml-px inline-block h-[1.05em] w-px translate-y-[3px] animate-pulse {accent_bg}",
            }
        }
    }
}

fn start_typewriter(
    mut ex_idx: Signal<usize>,
    mut typed: Signal<usize>,
    cb_cell: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
    timer_cell: Rc<RefCell<Option<i32>>>,
) {
    const PAUSE_TICKS: usize = 28;
    let cb = Closure::wrap(Box::new(move || {
        let idx = *ex_idx.peek();
        let full = EXAMPLES[idx % EXAMPLES.len()].chars().count();
        let t = *typed.peek();
        if t >= full + PAUSE_TICKS {
            typed.set(0);
            ex_idx.set((idx + 1) % EXAMPLES.len());
        } else {
            typed.set(t + 1);
        }
    }) as Box<dyn FnMut()>);
    if let Some(win) = web_sys::window()
        && let Ok(id) = win
            .set_interval_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), 60)
    {
        *timer_cell.borrow_mut() = Some(id);
    }
    *cb_cell.borrow_mut() = Some(cb);
}
