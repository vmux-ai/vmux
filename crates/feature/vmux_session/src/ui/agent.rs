use dioxus::prelude::*;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

#[component]
pub(super) fn DetailsToggle(
    details_available: bool,
    details_open: bool,
    on_details_toggle: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "absolute right-3 top-2 z-50 flex size-7 items-center justify-center rounded-md bg-background/90 text-muted-foreground transition-colors hover:bg-card hover:text-foreground disabled:opacity-35",
            title: translate("agent-details"),
            aria_label: translate("agent-details"),
            aria_expanded: details_open,
            disabled: !details_available,
            onclick: move |_| on_details_toggle.call(()),
            LineIconView { icon: LineIcon::Layout, class: "size-4" }
        }
    }
}
