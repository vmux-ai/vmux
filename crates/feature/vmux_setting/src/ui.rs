#![allow(non_snake_case)]

use std::rc::Rc;

use crate::event::{
    CheckForUpdatesEvent, SettingsEditRequest, SettingsFilterRequest, SettingsRequest,
};
use crate::state::{
    SettingsBindingRow, SettingsRenderField, SettingsRenderFieldId, SettingsRenderFieldKind,
    SettingsRenderItem, SettingsRenderItemId, SettingsSection, SettingsSelectOption,
    SettingsUiState,
};
use dioxus::prelude::*;
use vmux_api::address::{AddressCrumb, AddressIntent, AddressOption, PageAddress};
use vmux_api::page_location::PageLocationUiState;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::input::Input;
use vmux_ui::components::manager::{
    ManagerSearchAction, ManagerSelect, ManagerSelectItem, WorkspaceLayout,
};
use vmux_ui::components::page_address::PageBreadcrumbs;
use vmux_ui::components::switch::{Switch, SwitchThumb};
use vmux_ui::dioxus_ext::attributes;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;
use vmux_ui::scroll::{ScrollIntoView, SectionNavigation};

const SETTINGS_SCROLL_ID: &str = "settings-scroll-viewport";

#[vmux_page::page(
    component = Page,
    states = ["PageAddressUiState", "PageLocationUiState", "ThemeUiState", "SettingsUiState"]
)]
pub(crate) struct SettingsPage;

#[derive(Clone, PartialEq)]
struct SettingsRenderCatalog {
    fields: Rc<Vec<SettingsRenderField>>,
    items: Rc<Vec<SettingsRenderItem>>,
}

impl SettingsRenderCatalog {
    fn field(&self, id: SettingsRenderFieldId) -> Option<SettingsRenderField> {
        self.fields.get(id.0 as usize).cloned()
    }

    fn item(&self, id: SettingsRenderItemId) -> Option<SettingsRenderItem> {
        self.items.get(id.0 as usize).cloned()
    }
}

#[component]
pub fn Page() -> Element {
    use_theme();
    let state = use_ui_state::<SettingsUiState>().state;
    let SettingsUiState {
        query,
        sections,
        fields,
        items,
    } = state();
    let navigation = SettingsSectionNavigation::use_state(state);
    let active_section = navigation.navigation.active;

    if sections.is_empty() {
        return rsx! {
            div { class: "flex h-full items-center justify-center bg-background",
                div { class: "h-1.5 w-24 animate-pulse rounded-full bg-muted" }
            }
        };
    }

    let catalog = SettingsRenderCatalog {
        fields: Rc::new(fields),
        items: Rc::new(items),
    };
    let current_section = sections
        .iter()
        .find(|section| section.id == active_section())
        .or_else(|| sections.first())
        .cloned();
    let section_options = sections
        .iter()
        .map(|section| AddressOption {
            label: section.title.clone(),
            icon: Some(section.icon()),
            intent: Some(AddressIntent::Section {
                id: section.id.clone(),
            }),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    let mut address = PageAddress::default();
    if let Some(section) = current_section {
        let icon = section.icon();
        address.fixed.push(AddressCrumb {
            entry: AddressOption {
                label: section.title,
                icon: Some(icon),
                intent: Some(AddressIntent::Section { id: section.id }),
                ..Default::default()
            },
            options: section_options,
            active: true,
            ..Default::default()
        });
    }
    rsx! {
        div { class: "flex h-full min-h-0 flex-col bg-background text-foreground",
            PageBreadcrumbs {
                state: address,
                onintent: move |intent| {
                    if let AddressIntent::Section { id } = intent {
                        navigation.select(id);
                    }
                },
            }
            WorkspaceLayout {
                search: rsx! {
                    ManagerSearchAction {
                        value: query.clone(),
                        placeholder: translate("command-search"),
                        onsearch: move |query| {
                            let _ = send(&SettingsFilterRequest { query });
                        },
                    }
                },
                sidebar: rsx! {
                    nav { class: "flex flex-col gap-0.5 px-2 py-4",
                        for section in &sections {
                            button {
                                key: "{section.id}",
                                r#type: "button",
                                class: if section.id == active_section() {
                                    "flex items-center gap-2 rounded-md bg-foreground/[0.065] px-2 py-1.5 text-left text-sm text-foreground"
                                } else {
                                    "flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-muted-foreground transition-colors hover:bg-foreground/[0.04] hover:text-foreground"
                                },
                                onclick: {
                                    let id = section.id.clone();
                                    move |_| {
                                        navigation.select(id.clone());
                                    }
                                },
                                vmux_ui::icon::BuiltinIconView {
                                    icon: section.icon(),
                                    class: "size-3.5 shrink-0".to_string(),
                                }
                                span { class: "min-w-0 truncate", "{section.title}" }
                            }
                        }
                    }
                },
                main {
                    id: SETTINGS_SCROLL_ID,
                    class: "min-h-0 min-w-0 flex-1 overflow-y-auto",
                    onmounted: move |_| navigation.measure(),
                    onscroll: move |_| navigation.measure(),
                    onresize: move |_| navigation.measure(),
                    div { class: "mx-auto max-w-3xl px-6 py-6 lg:px-10",
                        div { class: "mb-8 lg:hidden",
                            p { class: "text-sm text-muted-foreground",
                                {translate("settings-stored")}
                            }
                        }
                        div { class: "flex flex-col",
                            for (index, section) in sections.into_iter().enumerate() {
                                SectionView {
                                    key: "{section.id}",
                                    index,
                                    section,
                                    catalog: catalog.clone(),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SectionView(index: usize, section: SettingsSection, catalog: SettingsRenderCatalog) -> Element {
    rsx! {
        section {
            id: "{section.id}",
            class: "scroll-mt-3 border-b border-foreground/[0.07] py-6 first:pt-3 last:border-b-0",
            "data-vmux-scroll-section": "{index}",
            header { class: "mb-3",
                h2 { class: "text-sm font-semibold text-foreground/85", "{section.title}" }
                if let Some(description) = section.description {
                    p { class: "mt-1 text-[11px] leading-4 text-muted-foreground", "{description}" }
                }
            }
            RenderFields { fields: section.fields, catalog }
        }
    }
}

#[derive(Clone, Copy)]
struct SettingsSectionNavigation {
    section_ids: Memo<Vec<String>>,
    navigation: SectionNavigation,
    measuring: Signal<bool>,
    pending: Signal<bool>,
}

impl SettingsSectionNavigation {
    fn use_state(state: Signal<SettingsUiState>) -> Self {
        let section_ids = use_memo(move || {
            let mut ids = Vec::new();
            for section in &state.read().sections {
                ids.push(section.id.clone());
            }
            ids
        });
        let navigation = Self {
            section_ids,
            navigation: SectionNavigation::use_state(),
            measuring: use_signal(|| false),
            pending: use_signal(|| false),
        };
        let location = use_ui_state::<PageLocationUiState>().state;
        use_effect(move || {
            let fragment = location.read().fragment.clone();
            let ids = section_ids.read();
            if ids.contains(&fragment) {
                navigation.navigation.receive(&fragment, fragment.clone());
            }
        });
        let mut applied = use_signal(|| 0u64);
        use_effect(move || {
            let reveal = (navigation.navigation.reveal)();
            if reveal.target.is_empty() || *applied.peek() == reveal.revision {
                return;
            }
            applied.set(reveal.revision);
            ScrollIntoView::section(SETTINGS_SCROLL_ID, &reveal.target, reveal.behavior);
        });
        use_effect(move || {
            section_ids.read();
            navigation.measure();
        });
        navigation
    }

    fn select(self, id: String) {
        self.navigation.select(id.clone(), id);
    }

    fn measure(mut self) {
        if *self.measuring.peek() {
            self.pending.set(true);
            return;
        }
        self.measuring.set(true);
        let ids = (self.section_ids)();
        spawn(async move {
            let section = ScrollIntoView::section_at_top(SETTINGS_SCROLL_ID).await;
            self.measuring.set(false);
            if (self.section_ids)() == ids
                && let Some(index) = section
                && let Some(id) = ids.get(index as usize)
            {
                self.navigation.follow(id.clone(), id.clone());
            }
            if *self.pending.peek() {
                self.pending.set(false);
                self.measure();
            }
        });
    }
}

#[component]
fn RenderFields(fields: Vec<SettingsRenderFieldId>, catalog: SettingsRenderCatalog) -> Element {
    rsx! {
        div { class: "flex flex-col divide-y divide-border",
            for field_id in fields {
                if let Some(field) = catalog.field(field_id) {
                    FieldView {
                        key: "{field.path}",
                        field,
                        catalog: catalog.clone(),
                    }
                }
            }
        }
    }
}

#[component]
fn FieldView(field: SettingsRenderField, catalog: SettingsRenderCatalog) -> Element {
    let SettingsRenderField {
        path,
        label,
        hint,
        kind,
    } = field;
    match kind {
        SettingsRenderFieldKind::Toggle { value } => rsx! {
            Row { label, hint,
                control: rsx! { Toggle { path, value } },
            }
        },
        SettingsRenderFieldKind::Integer { value } => rsx! {
            Row { label, hint,
                control: rsx! { IntInput { path, value } },
            }
        },
        SettingsRenderFieldKind::Number { value, step } => rsx! {
            Row { label, hint,
                control: rsx! { NumberInput { path, value, step } },
            }
        },
        SettingsRenderFieldKind::Text { value, placeholder } => rsx! {
            StackedRow { label, hint,
                control: rsx! { TextInput { path, value, placeholder } },
            }
        },
        SettingsRenderFieldKind::Select { value, options } => rsx! {
            Row { label, hint,
                control: rsx! { SelectInput { path, value, options } },
            }
        },
        SettingsRenderFieldKind::Chord { text } => rsx! {
            Row { label, hint,
                control: rsx! { ChordEditor { path, text } },
            }
        },
        SettingsRenderFieldKind::Bindings { rows } => rsx! {
            BindingsField { label, hint, rows }
        },
        SettingsRenderFieldKind::Group { fields } => rsx! {
            div { class: "flex flex-col py-3 first:pt-0 last:pb-0",
                SubgroupHeading { label }
                if let Some(hint) = hint {
                    p { class: "px-1 pb-2 text-xs text-muted-foreground", "{hint}" }
                }
                div { class: "flex flex-col divide-y divide-foreground/[0.06] pl-3",
                    RenderFields { fields, catalog }
                }
            }
        },
        SettingsRenderFieldKind::Array { items } => rsx! {
            div { class: "flex flex-col py-3 first:pt-0 last:pb-0",
                div { class: "flex items-center justify-between pb-2",
                    SubgroupHeading { label }
                    span { class: "text-[11px] text-muted-foreground", "{items.len()}" }
                }
                if let Some(hint) = hint {
                    p { class: "pb-2 text-xs text-muted-foreground", "{hint}" }
                }
                ArrayBody { items, catalog }
            }
        },
        SettingsRenderFieldKind::UpdateCheck {
            button_label,
            disabled,
        } => rsx! {
            Row { label, hint,
                control: rsx! {
                    Button {
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Xs,
                        class: "h-7 px-2.5 text-[11px]".to_string(),
                        disabled,
                        onclick: move |_| {
                            let _ = send(&CheckForUpdatesEvent);
                        },
                        "{button_label}"
                    }
                },
            }
        },
    }
}

#[component]
fn Row(label: String, hint: Option<String>, control: Element) -> Element {
    rsx! {
        div { class: "flex items-center justify-between gap-6 py-2.5 first:pt-0 last:pb-0",
            div { class: "min-w-0 flex-1",
                div { class: "text-xs font-medium text-foreground/90", "{label}" }
                if let Some(hint) = hint {
                    p { class: "mt-0.5 text-[10px] leading-4 text-muted-foreground", "{hint}" }
                }
            }
            div { class: "shrink-0", {control} }
        }
    }
}

#[component]
fn StackedRow(label: String, hint: Option<String>, control: Element) -> Element {
    rsx! {
        div { class: "flex flex-col gap-2 py-2.5 first:pt-0 last:pb-0",
            div { class: "flex flex-col gap-0.5",
                div { class: "text-xs font-medium text-foreground/90", "{label}" }
                if let Some(hint) = hint {
                    p { class: "text-[10px] leading-4 text-muted-foreground", "{hint}" }
                }
            }
            {control}
        }
    }
}

#[component]
fn Toggle(path: String, value: bool) -> Element {
    rsx! {
        div { class: "origin-right scale-90",
            Switch {
                checked: value,
                on_checked_change: move |checked| {
                    let _ = send(&SettingsRequest {
                        path: path.clone(),
                        value: serde_json::json!(checked).into(),
                    });
                },
                SwitchThumb { attributes: vec![] }
            }
        }
    }
}

#[component]
fn IntInput(path: String, value: String) -> Element {
    rsx! {
        Input {
            attributes: attributes!(input {
                r#type: "number",
                step: "1",
                value: "{value}",
                class: "h-7 w-20 bg-transparent px-2 py-1 text-right text-xs tabular-nums dark:bg-transparent",
            }),
            oninput: move |event: FormEvent| {
                let _ = send(&SettingsEditRequest {
                    path: path.clone(),
                    draft: event.value(),
                });
            },
            placeholder: None::<String>,
            children: rsx! {},
        }
    }
}

#[component]
fn NumberInput(path: String, value: String, step: f64) -> Element {
    rsx! {
        Input {
            attributes: attributes!(input {
                r#type: "number",
                step: "{step}",
                value: "{value}",
                class: "h-7 w-20 bg-transparent px-2 py-1 text-right text-xs tabular-nums dark:bg-transparent",
            }),
            oninput: move |event: FormEvent| {
                let _ = send(&SettingsEditRequest {
                    path: path.clone(),
                    draft: event.value(),
                });
            },
            placeholder: None::<String>,
            children: rsx! {},
        }
    }
}

#[component]
fn TextInput(path: String, value: String, placeholder: Option<String>) -> Element {
    let placeholder = placeholder.unwrap_or_default();
    rsx! {
        Input {
            attributes: attributes!(input {
                r#type: "text",
                value: "{value}",
                placeholder: "{placeholder}",
                class: "h-7 w-full bg-transparent px-2 py-1 text-xs dark:bg-transparent",
            }),
            oninput: move |event: FormEvent| {
                let _ = send(&SettingsEditRequest {
                    path: path.clone(),
                    draft: event.value(),
                });
            },
            placeholder: None::<String>,
            children: rsx! {},
        }
    }
}

#[component]
fn SelectInput(path: String, value: String, options: Vec<SettingsSelectOption>) -> Element {
    let items = options
        .into_iter()
        .map(|option| ManagerSelectItem {
            value: option.value,
            label: option.label,
        })
        .collect();
    rsx! {
        ManagerSelect {
            items,
            value: (!value.is_empty()).then_some(value),
            placeholder: String::new(),
            onselect: move |value| {
                let _ = send(&SettingsRequest {
                    path: path.clone(),
                    value: serde_json::json!(value).into(),
                });
            },
        }
    }
}

#[component]
fn BindingsField(label: String, hint: Option<String>, rows: Vec<SettingsBindingRow>) -> Element {
    rsx! {
        div { class: "flex flex-col py-3 first:pt-0 last:pb-0",
            div { class: "mb-3 flex items-center justify-between gap-2",
                div { class: "text-sm font-medium text-foreground", "{label}" }
                span { class: "text-[11px] text-muted-foreground", "{rows.len()}" }
            }
            if let Some(hint) = hint {
                p { class: "mb-2 text-xs text-muted-foreground", "{hint}" }
            }
            if rows.is_empty() {
                div { class: "text-xs text-muted-foreground", {translate("settings-none")} }
            } else {
                div { class: "flex flex-col gap-1",
                    for (index, row) in rows.into_iter().enumerate() {
                        BindingRow { key: "{index}", row }
                    }
                }
            }
        }
    }
}

#[component]
fn BindingRow(row: SettingsBindingRow) -> Element {
    rsx! {
        div { class: "flex items-center justify-between gap-4 border-b border-foreground/[0.06] px-1 py-2 last:border-b-0",
            span { class: "truncate font-mono text-xs text-foreground", "{row.command}" }
            if let Some(path) = row.edit_path {
                ChordEditor { path, text: row.chord }
            } else {
                Kbd { text: row.chord }
            }
        }
    }
}

#[component]
fn ArrayBody(items: Vec<SettingsRenderItemId>, catalog: SettingsRenderCatalog) -> Element {
    let Some(first) = items.first().and_then(|id| catalog.item(*id)) else {
        return rsx! {
            div { class: "px-3 py-4 text-center text-xs text-muted-foreground",
                {translate("settings-empty")}
            }
        };
    };
    if matches!(first, SettingsRenderItem::Value { .. }) {
        return rsx! {
            div { class: "flex flex-col gap-1",
                for (index, item_id) in items.into_iter().enumerate() {
                    if let Some(SettingsRenderItem::Value { text }) = catalog.item(item_id) {
                        div { key: "{index}", class: "border-b border-foreground/[0.06] px-1 py-1.5 font-mono text-[11px] text-foreground last:border-b-0",
                            "{text}"
                        }
                    }
                }
            }
        };
    }
    rsx! {
        div { class: "flex flex-col gap-3",
            for (index, item_id) in items.into_iter().enumerate() {
                if let Some(SettingsRenderItem::Object { title, fields }) = catalog.item(item_id) {
                    ArrayItemCard { key: "{index}", title, fields, catalog: catalog.clone() }
                }
            }
        }
    }
}

#[component]
fn ArrayItemCard(
    title: String,
    fields: Vec<SettingsRenderFieldId>,
    catalog: SettingsRenderCatalog,
) -> Element {
    rsx! {
        div { class: "border-b border-foreground/[0.07] py-4 last:border-b-0",
            div { class: "mb-3 flex items-center justify-between gap-2",
                div { class: "min-w-0",
                    div { class: "truncate text-sm font-semibold text-foreground", "{title}" }
                }
                span { class: "px-1 text-[10px] uppercase tracking-wide text-muted-foreground",
                    {translate("settings-item")}
                }
            }
            RenderFields { fields, catalog }
        }
    }
}

#[component]
fn SubgroupHeading(label: String) -> Element {
    rsx! {
        div { class: "px-1 py-1.5",
            div { class: "text-[11px] font-semibold uppercase tracking-wider text-muted-foreground",
                "{label}"
            }
        }
    }
}

const KEY_CAPTURE_ID: &str = "vmux-settings-key-capture";

#[component]
fn ChordEditor(path: String, text: String) -> Element {
    let mut recording = use_signal(|| false);
    let mut feedback = use_signal(|| None::<String>);

    use_effect(move || {
        if recording() {
            FocusClaim::new(KEY_CAPTURE_ID).request();
        }
    });

    if recording() {
        let preview = feedback().unwrap_or_else(|| translate("settings-press-key"));
        rsx! {
            button {
                r#type: "button",
                id: KEY_CAPTURE_ID,
                tabindex: "0",
                class: "inline-flex animate-pulse items-center gap-2 rounded-md border border-primary bg-primary/15 px-3 py-1 font-mono text-[11px] text-foreground outline-none",
                onkeydown: move |event: KeyboardEvent| {
                    event.prevent_default();
                    event.stop_propagation();
                    let key = event.key();
                    if key == Key::Escape {
                        recording.set(false);
                        feedback.set(None);
                        return;
                    }
                    if matches!(key, Key::Control | Key::Shift | Key::Alt | Key::Meta) {
                        return;
                    }
                    let key = match key {
                        Key::Character(key) => key,
                        key => key.to_string(),
                    };
                    let modifiers = event.modifiers();
                    let _ = send(&SettingsRequest {
                        path: path.clone(),
                        value: serde_json::json!({
                            "key": key,
                            "ctrl": modifiers.contains(Modifiers::CONTROL),
                            "shift": modifiers.contains(Modifiers::SHIFT),
                            "alt": modifiers.contains(Modifiers::ALT),
                            "super_key": modifiers.contains(Modifiers::META),
                        })
                        .into(),
                    });
                    feedback.set(Some(translate("settings-saved")));
                    recording.set(false);
                },
                onblur: move |_| {
                    recording.set(false);
                    feedback.set(None);
                },
                "{preview}"
            }
        }
    } else {
        rsx! {
            button {
                r#type: "button",
                class: "inline-flex cursor-pointer items-center rounded-md bg-foreground/[0.055] px-2 py-1 font-mono text-[11px] text-foreground transition-colors hover:bg-foreground/[0.09]",
                title: translate("settings-record-key"),
                onclick: move |_| {
                    feedback.set(None);
                    recording.set(true);
                },
                "{text}"
            }
        }
    }
}

#[component]
fn Kbd(text: String) -> Element {
    rsx! {
        span { class: "inline-flex items-center rounded-md bg-foreground/[0.055] px-2 py-1 font-mono text-[11px] text-foreground",
            "{text}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::{ElementId, Mutation, VirtualDom};
    use dioxus::html::{EventData, ScrollBehavior, SerializedScrollData};
    use std::cell::{Cell, RefCell};
    use std::future::Future;
    use vmux_api::address::PageAddressPublish;
    use vmux_api::page_location::PageLocationRequest;
    use vmux_api::{BinEvent, UiStateFrame};
    use vmux_ui::hooks::EventListenerError;
    use vmux_ui::transport::{BytesListener, HostScope, PageHost, TextOffsetAnswer};

    #[derive(Default)]
    struct SettingsHost {
        section: Cell<u32>,
        locations: RefCell<Vec<String>>,
        addresses: RefCell<Vec<PageAddress>>,
        scrolls: RefCell<Vec<String>>,
        listeners: RefCell<Vec<(String, BytesListener)>>,
    }

    impl PageHost for SettingsHost {
        fn send(&self, id: &str, bytes: &[u8]) -> Result<(), EventListenerError> {
            if id == PageLocationRequest::id() {
                let request =
                    rkyv::from_bytes::<PageLocationRequest, rkyv::rancor::Error>(bytes).unwrap();
                self.locations.borrow_mut().push(request.fragment);
            } else if id == PageAddressPublish::id() {
                let request =
                    rkyv::from_bytes::<PageAddressPublish, rkyv::rancor::Error>(bytes).unwrap();
                self.addresses.borrow_mut().push(request.address);
            }
            Ok(())
        }

        fn listen(&self, id: &str, listener: BytesListener) -> Result<(), EventListenerError> {
            self.listeners.borrow_mut().push((id.to_string(), listener));
            Ok(())
        }

        fn section_at_top(&self, _: &str) -> TextOffsetAnswer {
            Box::pin(std::future::ready(Some(self.section.get())))
        }

        fn scroll_section(&self, _: &str, element: &str, _: ScrollBehavior) {
            self.scrolls.borrow_mut().push(element.to_string());
        }
    }

    impl SettingsHost {
        fn receive(&self, id: &str, bytes: &[u8]) {
            let mut found = false;
            for (source, listener) in self.listeners.borrow_mut().iter_mut() {
                if source == id {
                    listener(bytes);
                    found = true;
                }
            }
            assert!(found, "the page must subscribe to {id}");
        }

        fn settle(dom: &mut VirtualDom) -> Option<ElementId> {
            let mut scroll = None;
            for _ in 0..8 {
                let mut context = std::task::Context::from_waker(std::task::Waker::noop());
                let _ = std::pin::pin!(dom.wait_for_work()).poll(&mut context);
                for edit in dom.render_immediate_to_vec().edits {
                    if let Mutation::NewEventListener { name, id } = edit
                        && name == "scroll"
                    {
                        scroll = Some(id);
                    }
                }
            }
            scroll
        }
    }

    #[test]
    fn scrolling_settings_updates_the_breadcrumb_and_location_without_replaying_scroll() {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
        let host = Rc::new(SettingsHost::default());
        let _host = HostScope::enter(host.clone());
        let mut dom = VirtualDom::new(Page);
        dom.rebuild_to_vec();
        SettingsHost::settle(&mut dom);
        let mut state = SettingsUiState::default();
        for (id, title) in [
            ("general", "General"),
            ("appearance", "Appearance"),
            ("spaces", "Spaces"),
        ] {
            state.sections.push(SettingsSection {
                id: id.to_string(),
                title: title.to_string(),
                description: None,
                search_text: title.to_string(),
                fields: Vec::new(),
            });
        }
        let frame = UiStateFrame::<SettingsUiState, SettingsUiState>::Snapshot(state);
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
        host.receive(SettingsUiState::id(), &bytes);
        let scroll = SettingsHost::settle(&mut dom).unwrap();
        host.locations.borrow_mut().clear();
        host.section.set(2);
        let event = EventData::Scroll(SerializedScrollData {
            scroll_top: 600.0,
            scroll_left: 0.0,
            scroll_width: 800,
            scroll_height: 1200,
            client_width: 800,
            client_height: 400,
        });
        dom.runtime()
            .handle_event("scroll", Event::new(event.into_any(), false), scroll);
        SettingsHost::settle(&mut dom);
        assert_eq!(host.locations.borrow().as_slice(), ["spaces"]);
        assert_eq!(
            host.addresses.borrow().last().unwrap().fixed[0].entry.label,
            "Spaces"
        );
        let frame = UiStateFrame::<PageLocationUiState, PageLocationUiState>::Snapshot(
            PageLocationUiState {
                fragment: "spaces".to_string(),
            },
        );
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&frame).unwrap();
        host.receive(PageLocationUiState::id(), &bytes);
        SettingsHost::settle(&mut dom);
        assert!(host.scrolls.borrow().is_empty());
    }
}
