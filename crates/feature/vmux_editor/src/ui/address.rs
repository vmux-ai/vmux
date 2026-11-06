use dioxus::prelude::*;
use vmux_api::address::PageAddressUiState;
use vmux_ui::components::address::AddressControls;
use vmux_ui::hooks::use_ui_state;

#[component]
pub(super) fn EditorAddress() -> Element {
    let source = use_ui_state::<PageAddressUiState>().state;
    let Some(state) = source.read().address.clone() else {
        return rsx! {};
    };
    rsx! { div { class: "flex shrink-0 items-center justify-end gap-2 px-3 py-2",
        AddressControls { state }
    } }
}
