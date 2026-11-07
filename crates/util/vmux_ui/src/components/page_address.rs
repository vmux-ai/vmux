use crate::clipboard::Clipboard;
use crate::components::icon::Icon;
use crate::components::manager::ManagerView;
use crate::hooks::{send, use_ui_state};
use crate::i18n::translate;
use dioxus::prelude::*;
use vmux_api::address::{AddressIntent, PageAddress, PageAddressPublish, PageAddressUiState};

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
pub fn PageToolbar(controls: Vec<ToolbarControl>) -> Element {
    rsx! { div { class: "flex shrink-0 flex-wrap items-center justify-end gap-2 px-3 py-2",
        for (index, control) in controls.into_iter().enumerate() {
            PageToolbarControl { key: "{index}:{control.description.label}", control }
        }
    } }
}

#[component]
fn PageToolbarControl(control: ToolbarControl) -> Element {
    let mut open = use_signal(|| false);
    let description = control.description;
    match description.kind {
        ControlKind::Badge => {
            rsx! { span { class: "px-1 text-[10px] tabular-nums text-muted-foreground", "{description.label}" } }
        }
        ControlKind::Search => rsx! {
            crate::components::manager::ManagerSearchAction {
                value: description.value, placeholder: description.label,
                onsearch: move |value| { if let Some(callback) = control.callback { callback.call(value); } },
                onkeydown: Some(EventHandler::new(move |event: KeyboardEvent| {
                    let key = event.key().to_string();
                    if description.keys.contains(&key) && let Some(callback) = control.key_callback { callback.call(key); }
                })),
            }
        },
        ControlKind::Button => {
            let icon = crate::icon::LineIcon::parse(&description.icon);
            rsx! {
                crate::components::button::Button {
                    variant: if description.danger { crate::components::button::ButtonVariant::Destructive } else if description.primary { crate::components::button::ButtonVariant::Primary } else { crate::components::button::ButtonVariant::Ghost },
                    size: crate::components::button::ButtonSize::Xs,
                    r#type: "button", disabled: description.disabled,
                    class: if description.active { "h-7 whitespace-nowrap bg-foreground/10" } else { "h-7 whitespace-nowrap" },
                    title: description.label.clone(), aria_label: description.label.clone(),
                    onclick: move |_| { if let Some(callback) = control.callback { callback.call(String::new()); } },
                    if let Some(icon) = icon { crate::icon::LineIconView { icon, class: "size-3.5" } }
                    if icon.is_none() || description.primary || description.danger || description.show_label { span { "{description.label}" } }
                }
            }
        }
        ControlKind::Menu => rsx! {
            crate::components::dropdown::Dropdown {
                open: open(), on_dismiss: move |_| open.set(false),
                crate::components::button::Button {
                    variant: if description.primary { crate::components::button::ButtonVariant::Primary } else { crate::components::button::ButtonVariant::Ghost },
                    size: crate::components::button::ButtonSize::Xs,
                    r#type: "button", disabled: description.disabled,
                    class: "h-7 whitespace-nowrap", aria_label: description.label.clone(),
                    onclick: move |_| open.set(!open()),
                    if let Some(icon) = crate::icon::LineIcon::parse(&description.icon) { crate::icon::LineIconView { icon, class: "size-3.5" } }
                    if description.primary || description.show_label || description.icon.is_empty() { "{description.label}" }
                }
                if open() {
                    crate::components::dropdown::DropdownPopup { class: "min-w-48 p-1",
                        for option in description.options {
                            {
                                let value = option.value;
                                rsx! { crate::components::dropdown::DropdownOption {
                                    label: option.label, selected: false,
                                    onclick: move |_| { open.set(false); if let Some(callback) = control.callback { callback.call(value.clone()); } },
                                    leading: rsx! { if let Some(icon) = crate::icon::LineIcon::parse(&option.icon) { crate::icon::LineIconView { icon, class: "size-3.5" } } },
                                } }
                            }
                        }
                    }
                }
            }
        },
    }
}

#[derive(Clone, PartialEq, Default)]
struct ControlDescription {
    label: String,
    icon: String,
    value: String,
    disabled: bool,
    active: bool,
    primary: bool,
    show_label: bool,
    danger: bool,
    kind: ControlKind,
    keys: Vec<String>,
    options: Vec<ToolbarChoice>,
}

#[derive(Clone, PartialEq)]
pub struct ToolbarChoice {
    pub value: String,
    pub label: String,
    pub icon: String,
}

#[derive(Clone, Copy, PartialEq, Default)]
enum ControlKind {
    #[default]
    Button,
    Search,
    Badge,
    Menu,
}

#[derive(Clone, PartialEq)]
pub struct ToolbarControl {
    description: ControlDescription,
    callback: Option<EventHandler<String>>,
    key_callback: Option<EventHandler<String>>,
}

impl ToolbarControl {
    pub fn button(label: impl Into<String>, callback: impl FnMut(()) + 'static) -> Self {
        let callback = EventHandler::new(callback);
        Self {
            description: ControlDescription {
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
            description: ControlDescription {
                kind: ControlKind::Search,
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
            description: ControlDescription {
                kind: ControlKind::Badge,
                label: label.into(),
                ..Default::default()
            },
            callback: None,
            key_callback: None,
        }
    }

    pub fn menu(
        label: String,
        options: Vec<ToolbarChoice>,
        callback: impl FnMut(String) + 'static,
    ) -> Self {
        Self {
            description: ControlDescription {
                label,
                options,
                kind: ControlKind::Menu,
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
    use dioxus::core::{AttributeValue, Mutation};
    use dioxus_html::{EventData, SerializedHtmlEventConverter, SerializedMouseData};

    #[component]
    fn ToolbarPage() -> Element {
        let mut count = use_signal(|| 0);
        rsx! {
            PageToolbar { controls: vec![ToolbarControl::button("Run", move |_| count.set(count() + 1))] }
            div { "count={count}" }
        }
    }

    #[test]
    fn page_controls_render_and_update_their_own_page_state() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(ToolbarPage);
        let rendered = dom.rebuild_to_vec();
        let button = rendered
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
            .expect("the page must render its own control");
        dom.runtime().handle_event(
            "click",
            Event::new(
                EventData::Mouse(SerializedMouseData::default()).into_any(),
                true,
            ),
            button,
        );
        dom.render_immediate_to_vec();
        assert!(dioxus_ssr::render(&dom).contains("count=1"));
    }
}
