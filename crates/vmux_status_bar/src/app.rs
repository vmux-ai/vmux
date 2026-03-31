//! Root [`App`] component (status strip). Markup uses Tailwind (`assets/input.css` → `assets/status.css`).

use crate::bridge::EVAL_SCRIPT;
use crate::payload::{BridgeMsg, apply_payload};

use dioxus::prelude::*;
use vmux_ui::hooks::use_eval_loop;

const STATUS_CSS: &str = include_str!("../assets/status.css");

#[component]
pub fn App() -> Element {
    let mut user_host = use_signal(String::new);
    let mut win_label = use_signal(|| "0:web*".to_string());
    let mut clock = use_signal(String::new);

    use_eval_loop::<String, _>(EVAL_SCRIPT, move |ron_text| {
        let msg: BridgeMsg = match ron::from_str(&ron_text) {
            Ok(m) => m,
            Err(_) => return,
        };
        match msg {
            BridgeMsg::Clock { text } => clock.set(text),
            BridgeMsg::Status { payload } => apply_payload(&payload, user_host, win_label),
        }
    });

    rsx! {
        style { dangerous_inner_html: STATUS_CSS }
        div {
            id: "bar",
            class: "flex min-h-0 w-full flex-1 flex-row items-center !justify-start overflow-hidden px-2 py-0 leading-none select-none !text-left",
            aria_label: "status",
            div {
                class: "flex min-w-0 w-full max-w-full flex-nowrap items-center !justify-start gap-1 !text-left",
                span { class: "inline-flex max-h-full shrink-0 items-center !text-left px-0.5", "{user_host}" }
                span { class: "shrink-0 !text-left font-bold text-tmux-dim", "|" }
                span { class: "inline-flex max-h-full shrink-0 items-center !text-left px-0.5", "{win_label}" }
                span { class: "shrink-0 !text-left font-bold text-tmux-dim", "|" }
                span { class: "inline-flex max-h-full shrink-0 items-center !text-left px-0.5", "{clock}" }
            }
        }
    }
}
