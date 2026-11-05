use crate::clipboard::Clipboard;
use crate::components::icon::Icon;
use crate::components::manager::ManagerView;
use crate::hooks::{send, use_ui_state};
use crate::i18n::translate;
use dioxus::prelude::*;
use vmux_api::address::{
    AddressControl, AddressControlChoice, AddressControlKind, AddressIntent, PageAddress,
    PageAddressPublish, PageAddressUiState, PageToolbarPublish,
};

#[component]
pub fn PageBreadcrumbs(
    state: PageAddress,
    #[props(default)] onintent: Option<EventHandler<AddressIntent>>,
) -> Element {
    use_effect(use_reactive!(|(state,)| {
        let _ = send(&PageAddressPublish {
            address: state.clone(),
        });
    }));
    let source = use_ui_state::<PageAddressUiState>().state;
    let mut observed = use_signal(move || {
        source
            .peek()
            .effect
            .as_ref()
            .map_or(0, |effect| effect.revision)
    });
    use_effect(move || {
        let effect = source.read().effect.clone();
        let Some(effect) = effect else {
            return;
        };
        if effect.revision <= *observed.peek() {
            return;
        }
        observed.set(effect.revision);
        if source
            .read()
            .address
            .as_ref()
            .is_none_or(|address| !effect.matches(address))
        {
            return;
        }
        if let Some(onintent) = onintent {
            onintent.call(effect.intent);
        }
    });
    rsx! {}
}

#[component]
pub fn PageToolbar(mut controls: Vec<ToolbarControl>) -> Element {
    let slot = use_hook(ToolbarControl::allocate);
    let source = use_ui_state::<PageAddressUiState>().state;
    let mut observed = use_signal(move || {
        source
            .peek()
            .effect
            .as_ref()
            .map_or(0, |effect| effect.revision)
    });
    let callbacks =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(Vec::<ToolbarControl>::new())));
    let mut described = Vec::with_capacity(controls.len());
    for (index, control) in controls.iter_mut().enumerate() {
        control.description.id = format!("toolbar-{slot}-{index}");
        described.push(control.description.clone());
    }
    *callbacks.borrow_mut() = controls;
    use_effect(use_reactive!(|(described,)| {
        let _ = send(&PageToolbarPublish {
            id: slot,
            controls: described.clone(),
        });
    }));
    use_drop(move || {
        let _ = send(&PageToolbarPublish {
            id: slot,
            controls: Vec::new(),
        });
    });
    use_effect(move || {
        let source = source.read();
        let Some(effect) = &source.effect else {
            return;
        };
        if effect.revision <= *observed.peek() {
            return;
        }
        observed.set(effect.revision);
        if source
            .address
            .as_ref()
            .is_none_or(|address| !effect.matches(address))
        {
            return;
        }
        let (id, value, keyboard) = match &effect.intent {
            AddressIntent::Control { id, value } => (id, value, false),
            AddressIntent::ControlKey { id, key } => (id, key, true),
            _ => return,
        };
        let callback = callbacks
            .borrow()
            .iter()
            .find(|control| control.description.id == *id)
            .and_then(|control| {
                if keyboard {
                    control.key_callback
                } else {
                    control.callback
                }
            });
        if let Some(callback) = callback {
            callback.call(value.clone());
        }
    });
    rsx! {}
}

#[derive(Clone, PartialEq)]
pub struct ToolbarControl {
    description: AddressControl,
    callback: Option<EventHandler<String>>,
    key_callback: Option<EventHandler<String>>,
}

impl ToolbarControl {
    fn allocate() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    pub fn button(label: impl Into<String>, callback: impl FnMut(()) + 'static) -> Self {
        let callback = EventHandler::new(callback);
        Self {
            description: AddressControl {
                label: label.into(),
                ..Default::default()
            },
            callback: Some(EventHandler::new(move |_| callback.call(()))),
            key_callback: None,
        }
    }

    pub fn search(
        value: String,
        placeholder: String,
        callback: impl FnMut(String) + 'static,
    ) -> Self {
        Self {
            description: AddressControl {
                kind: AddressControlKind::Search,
                value,
                label: placeholder,
                ..Default::default()
            },
            callback: Some(EventHandler::new(callback)),
            key_callback: None,
        }
    }

    pub fn badge(label: impl Into<String>) -> Self {
        Self {
            description: AddressControl {
                kind: AddressControlKind::Badge,
                label: label.into(),
                ..Default::default()
            },
            callback: None,
            key_callback: None,
        }
    }

    pub fn menu(
        label: String,
        options: Vec<AddressControlChoice>,
        callback: impl FnMut(String) + 'static,
    ) -> Self {
        Self {
            description: AddressControl {
                label,
                options,
                kind: AddressControlKind::Menu,
                ..Default::default()
            },
            callback: Some(EventHandler::new(callback)),
            key_callback: None,
        }
    }

    pub fn view(
        view: ManagerView,
        list_label: String,
        grid_label: String,
        callback: impl FnMut(ManagerView) + 'static,
    ) -> Vec<Self> {
        let callback = EventHandler::new(callback);
        vec![
            Self::button(list_label, move |_| callback.call(ManagerView::List))
                .icon(crate::icon::LineIcon::List)
                .active(view == ManagerView::List),
            Self::button(grid_label, move |_| callback.call(ManagerView::Grid))
                .icon(crate::icon::LineIcon::Grid2X2)
                .active(view == ManagerView::Grid),
        ]
    }

    pub fn icon(mut self, icon: crate::icon::LineIcon) -> Self {
        self.description.icon = format!("{icon:?}");
        self
    }

    pub fn keys(mut self, keys: &[&str], callback: impl FnMut(String) + 'static) -> Self {
        for key in keys {
            self.description.keys.push((*key).to_string());
        }
        self.key_callback = Some(EventHandler::new(callback));
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.description.active = active;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.description.disabled = disabled;
        self
    }

    pub fn primary(mut self) -> Self {
        self.description.primary = true;
        self
    }

    pub fn text(mut self) -> Self {
        self.description.show_label = true;
        self
    }

    pub fn danger(mut self) -> Self {
        self.description.danger = true;
        self
    }
}

#[component]
pub fn AddressSurface(url: String, onactivate: EventHandler<()>, children: Element) -> Element {
    let copy_url = url.clone();
    rsx! {
        div { class: "group/address relative flex h-8 min-w-0 flex-1 items-center",
            button {
                r#type: "button",
                "data-vmux-url-edit": "",
                class: "absolute inset-0 appearance-none rounded-md border-0 bg-transparent outline-none cursor-text focus-visible:bg-foreground/[0.04]",
                title: url.clone(),
                aria_label: url,
                onclick: move |_| onactivate.call(()),
            }
            div {
                class: "pointer-events-none relative flex h-full min-w-0 flex-1 items-center px-1 [&_button]:pointer-events-auto [&_a]:pointer-events-auto [&_input]:pointer-events-auto [&_textarea]:pointer-events-auto [&_select]:pointer-events-auto [&_[role=button]]:pointer-events-auto [&_[role=tab]]:pointer-events-auto [&_[popover]]:pointer-events-auto [&_[contenteditable]]:pointer-events-auto [&_[data-vmux-address-item]]:pointer-events-auto [&_[data-vmux-dropdown-root]]:pointer-events-auto",
                {children}
            }
            if !copy_url.is_empty() {
                button {
                    r#type: "button",
                    class: "relative z-10 ml-1 flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover/address:opacity-100 focus-visible:opacity-100 hover:bg-foreground/[0.05] hover:text-foreground",
                    title: translate("common-copy-url"),
                    aria_label: translate("common-copy-url"),
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        Clipboard::write(&copy_url);
                    },
                    Icon { class: "size-3.5",
                        rect { x: "9", y: "9", width: "13", height: "13", rx: "2" }
                        path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::address::AddressTrail;
    use crate::hooks::EventListenerError;
    use crate::transport::{BytesListener, HostBinding, PageHost};
    use dioxus::core::{AttributeValue, Mutation};
    use dioxus_html::{EventData, SerializedHtmlEventConverter, SerializedMouseData};
    use std::cell::RefCell;
    use std::rc::Rc;
    use vmux_api::BinEvent;
    use vmux_api::address::{AddressEffect, PageAddressRequest, PageAddressUiStatePatch};

    #[derive(Default)]
    struct ToolbarHost {
        controls: RefCell<Vec<AddressControl>>,
        requests: RefCell<Vec<PageAddressRequest>>,
        listeners: RefCell<Vec<BytesListener>>,
    }

    impl PageHost for ToolbarHost {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == PageToolbarPublish::id() {
                let update =
                    rkyv::from_bytes::<PageToolbarPublish, rkyv::rancor::Error>(bytes).unwrap();
                *self.controls.borrow_mut() = update.controls;
            } else if id == PageAddressRequest::id() {
                self.requests.borrow_mut().push(
                    rkyv::from_bytes::<PageAddressRequest, rkyv::rancor::Error>(bytes).unwrap(),
                );
            }
            Ok(())
        }

        fn listen(&self, id: &str, listener: BytesListener) -> Result<(), EventListenerError> {
            if id == PageAddressUiState::id() {
                self.listeners.borrow_mut().push(listener);
            }
            Ok(())
        }
    }

    impl ToolbarHost {
        fn deliver(&self, address: &PageAddress, effect: Option<AddressEffect>) {
            let frame =
                vmux_api::UiStateFrame::<PageAddressUiState, PageAddressUiStatePatch>::Snapshot(
                    PageAddressUiState {
                        address: Some(address.clone()),
                        effect,
                    },
                );
            let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
            for listener in self.listeners.borrow_mut().iter_mut() {
                listener(&bytes);
            }
        }
    }

    #[component]
    fn ToolbarPage() -> Element {
        let mut count = use_signal(|| 0);
        rsx! {
            PageToolbar { controls: vec![ToolbarControl::button("Run", move |_| count.set(count() + 1))] }
            div { "count={count}" }
        }
    }

    #[component]
    fn WindowToolbar(address: PageAddress) -> Element {
        rsx! { AddressTrail { state: address } }
    }

    #[test]
    fn window_controls_render_outside_the_page_and_dispatch_to_their_source_state() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let source_host = Rc::new(ToolbarHost::default());
        let window_host = Rc::new(ToolbarHost::default());
        let mut page = VirtualDom::new(ToolbarPage);
        page.provide_root_context(HostBinding(source_host.clone()));
        page.rebuild_in_place();
        page.render_immediate_to_vec();
        assert!(!dioxus_ssr::render(&page).contains("<button"));
        let address = PageAddress {
            owner: 7,
            revision: 1,
            url: "git:///repo".into(),
            controls: source_host.controls.borrow().clone(),
            ..Default::default()
        };
        assert!(!address.controls.is_empty());
        source_host.deliver(&address, None);
        page.render_immediate_to_vec();
        let mut window = VirtualDom::new_with_props(
            WindowToolbar,
            WindowToolbarProps {
                address: address.clone(),
            },
        );
        window.provide_root_context(HostBinding(window_host.clone()));
        let mounted = window.rebuild_to_vec();
        let button = mounted
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-label",
                    value: AttributeValue::Text(value),
                    id,
                    ..
                } if value == "Run" => Some(*id),
                _ => None,
            })
            .expect("the window must render the page control");
        window.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            button,
        );
        window.render_immediate_to_vec();
        let request = window_host.requests.borrow().last().unwrap().clone();
        source_host.deliver(
            &address,
            Some(AddressEffect {
                revision: 1,
                owner: request.owner,
                url: request.url,
                address_revision: request.revision,
                intent: request.intent,
            }),
        );
        page.render_immediate_to_vec();
        page.render_immediate_to_vec();
        assert!(dioxus_ssr::render(&page).contains("count=1"));
        assert!(source_host.requests.borrow().is_empty());
    }
}
