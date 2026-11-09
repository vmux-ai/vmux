use dioxus::prelude::*;
use vmux_api::address::PageAddressUiState;
use vmux_ui::components::address::AddressControls;
use vmux_ui::hooks::use_ui_state;

#[component]
pub(super) fn EditorAddress(show_search: bool) -> Element {
    let source = use_ui_state::<PageAddressUiState>().state;
    let Some(mut state) = source.read().address.clone() else {
        return rsx! {};
    };
    state.search_available &= show_search && !state.search_active;
    rsx! { AddressControls { state } }
}
