#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::address::PageAddressUiState;
use vmux_ui::components::address::AddressControls;
use vmux_ui::components::page_address::PageToolbar;
use vmux_ui::hooks::use_ui_state;

#[component]
pub fn EditorAddress() -> Element {
    let address = use_ui_state::<PageAddressUiState>().state;
    let state = address.read();
    let Some(address) = state.address.clone() else {
        return rsx! {};
    };
    rsx! { PageToolbar { actions: rsx! { AddressControls { state: address } } } }
}
