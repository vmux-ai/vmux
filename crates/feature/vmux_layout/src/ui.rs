#![allow(non_snake_case)]

use self::extension::ExtensionPopupModal;
use self::header::HeaderView;
use self::side_sheet::SideSheetView;
use self::state::LayoutUi;
use dioxus::prelude::*;
pub(crate) use error::ErrorPage;
use vmux_command::CommandBarPanel;
use vmux_ui::hooks::use_theme;

mod active_session;
mod bookmark;
mod error;
mod extension;
mod header;
mod remote;
mod side_sheet;
mod stack;
mod state;
mod tab_drag;
mod update;
mod window_drag;

#[vmux_page::page(
    component = Page,
    states = [
        "ThemeUiState",
        "LayoutUiState",
        "ExtensionsUiState",
        "CommandBarUiState",
        "KeyClaimsUiState",
    ],
    placement = layout,
    transparent,
    stylesheet = "./assets/index.css",
    body_class = "m-0 flex h-full min-h-0 flex-col overflow-hidden bg-transparent p-0 text-foreground antialiased"
)]
pub struct LayoutPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let layout_ui = LayoutUi::use_state();
    layout_ui.provide();
    let center_offset = layout_ui
        .value()
        .layout
        .unwrap_or_default()
        .main_cef_center_offset();

    rsx! {
        div { class: "fixed inset-0 pointer-events-none text-foreground",
            SideSheetView {}
            HeaderView {}
            CommandBarPanel { center_offset }
            ExtensionPopup {}
        }
    }
}

#[component]
fn ExtensionPopup() -> Element {
    let ui = LayoutUi::current().value();
    if ui.extension_popup.id.is_empty() {
        return rsx! {};
    }

    rsx! {
        ExtensionPopupModal {
            popup: ui.extension_popup,
            preferred_size: ui.extension_popup_size,
        }
    }
}
