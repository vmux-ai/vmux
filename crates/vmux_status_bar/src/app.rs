//! Root [`App`] component (status strip). Tailwind is bundled via [`asset!`] / `dx` (see `assets/input.css`).

use crate::bridge::EVAL_SCRIPT;
use crate::payload::{BridgeMsg, apply_payload};

use dioxus::prelude::*;
use vmux_ui::dioxus_ext::{attributes, merge_attributes};
use vmux_ui::webview::components::{
    UiRow,
    badge::{Badge, BadgeVariant},
    separator::Separator,
};
use vmux_ui::webview::hooks::use_eval_loop;

const SEG_PAD: &str = "inline-flex max-h-full shrink-0 items-center !text-left px-0.5";
const ROW_INNER: &str =
    "min-w-0 w-full max-w-full flex-nowrap items-center !justify-start gap-1 !text-left";

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
        document::Stylesheet { href: asset!("/assets/input.css") }
        div {
            id: "bar",
            class: "flex min-h-0 w-full flex-1 flex-row items-center !justify-start overflow-hidden px-2 py-0 leading-none select-none !text-left",
            aria_label: "status",
            UiRow {
                class: ROW_INNER,
                Badge {
                    variant: BadgeVariant::Outline,
                    attributes: merge_attributes(vec![attributes!(span { class: SEG_PAD })]),
                    "{user_host}"
                }
                Separator {
                    horizontal: false,
                    decorative: true,
                    attributes: vec![],
                    children: rsx! {},
                }
                Badge {
                    variant: BadgeVariant::Outline,
                    attributes: merge_attributes(vec![attributes!(span { class: SEG_PAD })]),
                    "{win_label}"
                }
                Separator {
                    horizontal: false,
                    decorative: true,
                    attributes: vec![],
                    children: rsx! {},
                }
                Badge {
                    variant: BadgeVariant::Outline,
                    attributes: merge_attributes(vec![attributes!(span { class: SEG_PAD })]),
                    "{clock}"
                }
            }
        }
    }
}
