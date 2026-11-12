use std::rc::Rc;

use dioxus::prelude::*;
use vmux_api::mcp::{
    ConnectorPopupBoundsRequest, ConnectorPopupCloseRequest, ConnectorPopupShowRequest,
    ConnectorPopupState,
};
use vmux_ui::components::connector_icon::ConnectorIcon;
use vmux_ui::hooks::send;

use super::popup::NativePopupSurface;
use super::state::LayoutUi;

#[derive(Clone, Copy)]
pub(super) struct ToolbarAnchor(Signal<Option<Rc<MountedData>>>);

impl ToolbarAnchor {
    pub(super) fn provide() {
        let node = use_signal(|| None);
        use_context_provider(|| Self(node));
    }

    fn mount(mut self, node: Rc<MountedData>) {
        self.0.set(Some(node));
    }

    async fn measure(self) -> Option<(i32, i32)> {
        let node = self.0.peek().clone()?;
        let rect = node.get_client_rect().await.ok()?;
        Some((
            (rect.origin.x + rect.size.width).round() as i32,
            (rect.origin.y + rect.size.height).round() as i32,
        ))
    }
}

#[component]
pub(super) fn ConnectorBar() -> Element {
    let state = LayoutUi::current().use_selector(|state| state.connector_popup.clone())();
    let anchor = use_context::<ToolbarAnchor>();
    if state.id.is_empty() {
        return rsx! {};
    }
    rsx! {
        button { r#type: "button", class: "flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-foreground/80 hover:bg-foreground/[0.08]",
            title: state.name.clone(), aria_label: state.name.clone(), aria_expanded: state.open,
            onmounted: move |event| anchor.mount(event.data()),
            onclick: move |_| { let _ = send(&ConnectorPopupShowRequest { id: state.id.clone() }); },
            ConnectorIcon { id: state.provider, class: "size-4 shrink-0" }
        }
    }
}

#[component]
pub(super) fn ConnectorPopup() -> Element {
    let state = LayoutUi::current().use_selector(|state| state.connector_popup.clone())();
    if state.id.is_empty() || !state.open {
        return rsx! {};
    }
    rsx! { Modal { key: "{state.id}", state } }
}

#[component]
fn Modal(state: ConnectorPopupState) -> Element {
    let toolbar = use_context::<ToolbarAnchor>();
    let mut anchor = use_signal(|| (i32::MAX, 48));
    use_effect(move || {
        spawn(async move {
            if let Some(measured) = toolbar.measure().await {
                anchor.set(measured);
            }
        });
    });
    let (right, bottom) = anchor();
    let bounds_id = state.id.clone();
    let hide_id = state.id.clone();
    rsx! {
        NativePopupSurface {
            id: format!("connector-popup:{}", state.id), right, top: bottom.max(40) + 8, width: 360.0, height: 600.0,
            onbounds: move |(left, top, width, height)| { let _ = send(&ConnectorPopupBoundsRequest { id: bounds_id.clone(), left, top, width, height }); },
            ondismiss: move |_| { let _ = send(&ConnectorPopupCloseRequest { id: hide_id.clone() }); },
            children: rsx! {},
        }
    }
}
