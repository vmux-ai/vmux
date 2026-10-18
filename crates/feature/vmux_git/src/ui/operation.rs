#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::icon::{LineIcon, LineIconView};

#[component]
pub(super) fn HeaderOperationButton(
    icon: LineIcon,
    shortcut: &'static str,
    label: String,
    disabled: bool,
    danger: bool,
    onpress: EventHandler<()>,
) -> Element {
    let title = format!("{label} ({shortcut})");

    rsx! {
        button {
            r#type: "button",
            title,
            aria_label: "{label}",
            class: if danger {
                "flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-ansi-1/10 hover:text-ansi-1 disabled:opacity-30"
            } else {
                "flex size-6 items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground disabled:opacity-30"
            },
            disabled,
            onclick: move |event: MouseEvent| {
                event.stop_propagation();
                onpress.call(());
            },
            LineIconView { icon, class: "size-3.5" }
        }
    }
}
