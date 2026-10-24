#![allow(non_snake_case)]

use self::extension::ExtensionPopupModal;
use self::header::HeaderView;
use self::side_sheet::SideSheetView;
use self::state::LayoutUi;
use dioxus::prelude::*;
pub(crate) use error::ErrorPage;
use vmux_command::CommandBarPanel;
use vmux_ui::components::dropdown::Dropdown;
use vmux_ui::hooks::{send, use_theme};

use crate::event::LayoutOverlayEvent;

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
        vmux_ui::theme::UiTheme,
        crate::state::UiLayout,
        vmux_api::extension::UiExtensions,
        vmux_api::command_bar::UiCommandBar,
        vmux_api::input::UiKeyClaims,
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
        div {
            class: "fixed inset-0 pointer-events-none text-foreground",
            style: "--vmux-command-bar-center-offset:{center_offset}px;",
            SideSheetView {}
            HeaderView {}
            CommandBarPanel {}
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

#[component]
pub(super) fn LayoutDropdown(
    id: String,
    open: bool,
    #[props(default)] class: String,
    #[props(default)] keyboard_navigation: bool,
    on_dismiss: EventHandler<()>,
    children: Element,
) -> Element {
    let dismiss_revision = LayoutUi::current().value().overlay_dismiss_revision;
    let mut observed_dismiss_revision = use_signal(|| dismiss_revision);
    use_effect(move || {
        if dismiss_revision <= *observed_dismiss_revision.peek() {
            return;
        }
        observed_dismiss_revision.set(dismiss_revision);
        if open {
            on_dismiss.call(());
        }
    });
    let overlay_id = id.clone();
    use_effect(move || {
        send(&LayoutOverlayEvent {
            id: overlay_id.clone(),
            active: open,
        });
    });
    use_drop(move || {
        send(&LayoutOverlayEvent { id, active: false });
    });
    rsx! {
        Dropdown {
            open,
            class,
            keyboard_navigation,
            on_dismiss,
            {children}
        }
    }
}
