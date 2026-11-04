#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_api::address::UiPageAddress;
use vmux_ui::components::address::AddressTrail;
use vmux_ui::components::page_address::PageAddressContent;
use vmux_ui::hooks::UiState;

#[component]
pub fn EditorAddress() -> Element {
    let address = UiState::<UiPageAddress>::use_context().state;
    let state = address.read();
    let Some(address) = state.address.clone() else {
        return rsx! {};
    };
    rsx! { PageAddressContent { AddressTrail { state: address } } }
}
