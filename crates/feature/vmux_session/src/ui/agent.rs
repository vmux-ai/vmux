use dioxus::prelude::*;
use vmux_ui::components::page_address::{PageToolbar, ToolbarControl};
use vmux_ui::i18n::translate;
use vmux_ui::icon::LineIcon;

#[component]
pub(super) fn ChatHeader(
    details_available: bool,
    details_open: bool,
    on_details_toggle: EventHandler<()>,
) -> Element {
    rsx! { PageToolbar { controls: vec![
        ToolbarControl::button(translate("agent-details"), move |_| on_details_toggle.call(()))
            .icon(LineIcon::Layout).active(details_open).disabled(!details_available),
    ] } }
}
