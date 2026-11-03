#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::address::PageAddressUiState;
use vmux_ui::components::address::AddressTrail;
use vmux_ui::hooks::use_ui_state;

#[component]
pub fn EditorBreadcrumbs() -> Element {
    let address = use_ui_state::<PageAddressUiState>().state;
    let state = address.read();
    if state.in_header {
        return rsx! {};
    }
    let Some(address) = state.address.clone() else {
        return rsx! {};
    };
    rsx! { div { class: "flex h-8 min-w-0 shrink-0 items-center px-2", AddressTrail { state: address } } }
}
