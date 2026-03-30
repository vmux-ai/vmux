//! Root [`App`] component (status strip). Markup uses Tailwind (`assets/input.css` → `assets/status.css`).

use crate::bridge::EVAL_SCRIPT;
use crate::payload::{apply_payload, BridgeMsg};

use dioxus::prelude::*;

const STATUS_CSS: &str = include_str!("../assets/status.css");

#[component]
pub fn App() -> Element {
    let user_host = use_signal(String::new);
    let win_label = use_signal(|| "0:web*".to_string());
    let clock = use_signal(String::new);

    use_effect(move || {
        spawn(async move {
            let mut clock = clock;
            let mut eval = document::eval(EVAL_SCRIPT);
            loop {
                let Ok(raw) = eval.recv::<serde_json::Value>().await else {
                    break;
                };
                let msg: BridgeMsg = match serde_json::from_value(raw) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                match msg {
                    BridgeMsg::Clock { text } => clock.set(text),
                    BridgeMsg::Status { payload } => apply_payload(payload, user_host, win_label),
                }
            }
        });
    });

    rsx! {
        style { dangerous_inner_html: STATUS_CSS }
        div {
            id: "bar",
            class: "flex min-h-0 w-full flex-1 flex-row items-center justify-start overflow-hidden px-2 py-0 leading-none select-none text-left",
            aria_label: "status",
            div {
                class: "flex min-w-0 w-full max-w-full flex-nowrap items-center justify-start gap-1 text-left",
                span { class: "inline-flex max-h-full shrink-0 items-center px-0.5", "{user_host}" }
                span { class: "shrink-0 font-bold text-tmux-dim", "|" }
                span { class: "inline-flex max-h-full shrink-0 items-center px-0.5", "{win_label}" }
                span { class: "shrink-0 font-bold text-tmux-dim", "|" }
                span { class: "inline-flex max-h-full shrink-0 items-center px-0.5", "{clock}" }
            }
        }
    }
}
