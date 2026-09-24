#![allow(non_snake_case)]

mod active_session;
mod bookmark;
mod header;
mod side_sheet;
mod stack;
mod state;
mod tab_drag;
mod update;
mod window_drag;

use self::header::HeaderView;
use self::side_sheet::SideSheetView;
use self::state::LayoutUi;
use crate::extension::ExtensionPopupModal;
use dioxus::prelude::*;
use vmux_command::panel::CommandBarPanel;
use vmux_ui::hooks::use_theme;

#[vmux_native::page(
    url = crate::event::LAYOUT_PAGE_URL,
    title = "vmux",
    component = Page,
    placement = layout,
    document_url = crate::event::LAYOUT_PAGE_URL,
    transparent,
    head = r#"<base href="/"/>
<style>
html, body { height: 100%; margin: 0; min-height: 0; }
body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; background: transparent; }
</style>
<link rel="stylesheet" href="./assets/index.css"/>
<link rel="stylesheet" href="./assets/theme.css"/>"#,
    body_class = "m-0 flex h-full min-h-0 flex-col overflow-hidden bg-transparent p-0 text-foreground antialiased"
)]
pub struct LayoutPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let layout_ui = LayoutUi::use_state();
    layout_ui.provide();
    let bookmark_menu_state = use_memo(move || layout_ui.value().bookmark_menu_action);
    use_context_provider(|| bookmark_menu_state);

    rsx! {
        div { class: "fixed inset-0 pointer-events-none text-foreground",
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
