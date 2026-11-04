#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::address::PageAddressUiState;
use vmux_ui::components::address::AddressTrail;
use vmux_ui::components::page_address::PageAddressContent;
use vmux_ui::hooks::use_ui_state;

#[component]
pub fn EditorAddress() -> Element {
    let address = use_ui_state::<PageAddressUiState>().state;
    let state = address.read();
    let Some(address) = state.address.clone() else {
        return rsx! {};
    };
    rsx! { PageAddressContent { AddressTrail { state: address } } }
}
