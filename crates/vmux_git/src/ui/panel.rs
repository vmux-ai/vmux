#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::badge::Badge;
use vmux_ui::icon::{GitIconView, LineIcon, LineIconView};

#[component]
pub(super) fn PanelHeader(
    index: u8,
    title: String,
    count: Option<usize>,
    icon: PanelIcon,
    icon_class: &'static str,
    badge_class: &'static str,
    focused: bool,
    #[props(default)] actions: Option<Element>,
) -> Element {
    rsx! {
        div { class: if focused {
                "flex h-7 shrink-0 items-center gap-1.5 border-b border-ansi-2/20 bg-gradient-to-r from-ansi-2/[0.08] to-transparent px-2"
            } else {
                "flex h-7 shrink-0 items-center gap-1.5 border-b border-foreground/[0.07] bg-gradient-to-r from-foreground/[0.035] to-transparent px-2"
            },
            div { class: "flex size-5 shrink-0 items-center justify-center rounded-md {icon_class}",
                match icon {
                    PanelIcon::Git => rsx! { GitIconView { class: "h-3 w-3" } },
                    PanelIcon::Line(icon) => rsx! { LineIconView { icon, class: "h-3 w-3" } },
                }
            }
            span { class: if focused { "font-mono text-[9px] font-semibold text-ansi-2" } else { "font-mono text-[9px] font-semibold text-muted-foreground" }, "[{index}]" }
            span { class: "min-w-0 flex-1 truncate text-[11px] font-semibold tracking-[-0.01em]", "{title}" }
            if let Some(count) = count {
                Badge { class: "min-h-4 min-w-4 rounded-full border px-1 text-[8px] font-semibold tabular-nums {badge_class}", "{count}" }
            }
            if let Some(actions) = actions {
                {actions}
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum PanelIcon {
    Git,
    Line(LineIcon),
}

#[component]
pub(super) fn HeaderActionButton(
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
            title: "{title}",
            aria_label: "{label}",
            class: if danger {
                "flex h-5 min-w-5 items-center justify-center rounded border border-ansi-1/20 bg-ansi-1/[0.05] px-1 font-mono text-[8px] font-semibold text-ansi-1 hover:bg-ansi-1/12 disabled:opacity-30"
            } else {
                "flex h-5 min-w-5 items-center justify-center rounded border border-foreground/10 bg-background/45 px-1 font-mono text-[8px] font-semibold text-muted-foreground hover:bg-foreground/[0.08] hover:text-foreground disabled:opacity-30"
            },
            disabled,
            onclick: move |event: MouseEvent| {
                event.stop_propagation();
                onpress.call(());
            },
            LineIconView { icon, class: "size-3" }
        }
    }
}
