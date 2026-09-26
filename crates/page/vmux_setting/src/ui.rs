#![allow(non_snake_case)]

use std::rc::Rc;

use crate::event::{CheckForUpdatesEvent, SettingsRequest};
use crate::state::{
    SettingsBindingRow, SettingsRenderField, SettingsRenderFieldId, SettingsRenderFieldKind,
    SettingsRenderItem, SettingsRenderItemId, SettingsSection, SettingsSelectOption,
    SettingsUiState,
};
use dioxus::prelude::*;
use serde_json::Value;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use vmux_ui::components::input::Input;
use vmux_ui::components::select::{
    Select, SelectGroup, SelectItemIndicator, SelectList, SelectOption, SelectTrigger, SelectValue,
};
use vmux_ui::components::switch::{Switch, SwitchThumb};
use vmux_ui::dioxus_ext::attributes;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;

#[vmux_native::page(
    url = crate::event::SETTINGS_PAGE_URL,
    title = "Settings",
    component = Page
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
    let SettingsUiState {
        mut sections,
        fields,
        items,
    } = use_ui_state::<SettingsUiState>()();
    let mut search = use_signal(String::new);

    if sections.is_empty() {
        return rsx! {
            div { class: "flex h-full items-center justify-center bg-background",
                div { class: "h-1.5 w-24 animate-pulse rounded-full bg-muted" }
            }
        };
    }

    let query = search().trim().to_lowercase();
    if !query.is_empty() {
        sections.retain(|section| section.search_text.contains(&query));
    }
    let catalog = SettingsRenderCatalog {
        fields: Rc::new(fields),
        items: Rc::new(items),
    };
    let search_placeholder = format!("{}…", translate("command-search"));

    rsx! {
        div { class: "flex h-full min-h-0 flex-row bg-background text-foreground",
            aside { class: "hidden w-56 shrink-0 border-r border-border px-4 py-6 lg:block",
                div { class: "mb-4 px-2",
                    div { class: "text-base font-semibold tracking-tight", {translate("settings-title")} }
                    div { class: "mt-0.5 text-[11px] text-muted-foreground", "settings.ron" }
                }
                nav { class: "flex flex-col gap-0.5",
                    for section in &sections {
                        a {
                            key: "{section.id}",
                            href: "#{section.id}",
                            class: "rounded-md px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-foreground/[0.04] hover:text-foreground",
                            "{section.title}"
                        }
                    }
                }
            }
            main { class: "min-h-0 min-w-0 flex-1 overflow-y-auto",
                div { class: "mx-auto max-w-3xl px-6 py-8 lg:px-10",
                    div { class: "mb-8 lg:hidden",
                        h1 { class: "text-xl font-semibold tracking-tight", {translate("settings-title")} }
                        p { class: "mt-1 text-sm text-muted-foreground",
                            {translate("settings-stored")}
                        }
                    }
                    input {
                        r#type: "search",
                        class: "sticky top-0 z-10 mb-6 w-full rounded-xl bg-background/95 px-4 py-2.5 text-sm text-foreground outline-none ring-1 ring-inset ring-border backdrop-blur-xl transition-colors placeholder:text-muted-foreground/60 focus:bg-muted/40 focus:ring-primary/40",
                        placeholder: "{search_placeholder}",
                        value: "{search}",
                        oninput: move |event: FormEvent| search.set(event.value()),
                    }
                    div { class: "flex flex-col gap-8",
                        for section in sections {
                            SectionView {
                                key: "{section.id}",
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

#[component]
fn SectionView(section: SettingsSection, catalog: SettingsRenderCatalog) -> Element {
    rsx! {
        section { id: "{section.id}", class: "scroll-mt-6",
            Card {
                CardHeader {
                    CardTitle { "{section.title}" }
                    if let Some(description) = section.description {
                        CardDescription { "{description}" }
                    }
                }
                CardContent {
                    RenderFields { fields: section.fields, catalog }
                }
            }
        }
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
                div { class: "flex flex-col divide-y divide-border/60 rounded-md border border-border/40 bg-muted/20 px-3",
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
        div { class: "flex items-center justify-between gap-6 py-3 first:pt-0 last:pb-0",
            div { class: "min-w-0 flex-1",
                div { class: "text-sm font-medium text-foreground", "{label}" }
                if let Some(hint) = hint {
                    p { class: "mt-0.5 text-xs leading-snug text-muted-foreground", "{hint}" }
                }
            }
            div { class: "shrink-0", {control} }
        }
    }
}

#[component]
fn StackedRow(label: String, hint: Option<String>, control: Element) -> Element {
    rsx! {
        div { class: "flex flex-col gap-2 py-3 first:pt-0 last:pb-0",
            div { class: "flex flex-col gap-0.5",
                div { class: "text-sm font-medium text-foreground", "{label}" }
                if let Some(hint) = hint {
                    p { class: "text-xs leading-snug text-muted-foreground", "{hint}" }
                }
            }
            {control}
        }
    }
}

#[component]
fn Toggle(path: String, value: bool) -> Element {
    rsx! {
        Switch {
            checked: value,
            on_checked_change: move |checked| {
                SettingsWriteIntent::send(&path, serde_json::json!(checked));
            },
            SwitchThumb { attributes: vec![] }
        }
    }
}

#[component]
fn IntInput(path: String, value: u64) -> Element {
    let mut draft = use_signal(|| value.to_string());
    let observed = value.to_string();
    use_effect(use_reactive!(|observed| {
        if draft.peek().as_str() != observed.as_str() {
            draft.set(observed);
        }
    }));
    rsx! {
        Input {
            attributes: attributes!(input {
                r#type: "number",
                step: "1",
                value: "{draft}",
                class: "w-24 text-right tabular-nums text-sm",
            }),
            oninput: move |event: FormEvent| {
                let value = event.value();
                draft.set(value.clone());
                if let Ok(value) = value.parse::<u64>() {
                    SettingsWriteIntent::send(&path, serde_json::json!(value));
                }
            },
            placeholder: None::<String>,
            children: rsx! {},
        }
    }
}

#[component]
fn NumberInput(path: String, value: f64, step: f64) -> Element {
    let mut draft = use_signal(|| value.to_string());
    let observed = value.to_string();
    use_effect(use_reactive!(|observed| {
        if draft.peek().as_str() != observed.as_str() {
            draft.set(observed);
        }
    }));
    rsx! {
        Input {
            attributes: attributes!(input {
                r#type: "number",
                step: "{step}",
                value: "{draft}",
                class: "w-24 text-right tabular-nums text-sm",
            }),
            oninput: move |event: FormEvent| {
                let value = event.value();
                draft.set(value.clone());
                if let Ok(value) = value.parse::<f64>() {
                    SettingsWriteIntent::send(&path, serde_json::json!(value));
                }
            },
            placeholder: None::<String>,
            children: rsx! {},
        }
    }
}

#[component]
fn TextInput(path: String, value: String, placeholder: Option<String>) -> Element {
    let mut draft = use_signal(|| value.clone());
    let observed = value;
    let placeholder = placeholder.unwrap_or_default();
    use_effect(use_reactive!(|observed| {
        if draft.peek().as_str() != observed.as_str() {
            draft.set(observed);
        }
    }));
    rsx! {
        Input {
            attributes: attributes!(input {
                r#type: "text",
                value: "{draft}",
                placeholder: "{placeholder}",
                class: "w-full text-sm",
            }),
            oninput: move |event: FormEvent| {
                let value = event.value();
                draft.set(value.clone());
                SettingsWriteIntent::send(&path, serde_json::json!(value));
            },
            placeholder: None::<String>,
            children: rsx! {},
        }
    }
}

#[component]
fn SelectInput(path: String, value: String, options: Vec<SettingsSelectOption>) -> Element {
    let selected: Option<Option<String>> = Some((!value.is_empty()).then(|| value.clone()));
    rsx! {
        Select::<String> {
            value: Into::<ReadSignal<Option<Option<String>>>>::into(Signal::new(selected)),
            default_value: (!value.is_empty()).then(|| value.clone()),
            placeholder: Into::<ReadSignal<String>>::into(Signal::new(String::new())),
            on_value_change: Callback::new(move |value: Option<String>| {
                if let Some(value) = value {
                    SettingsWriteIntent::send(&path, serde_json::json!(value));
                }
            }),
            attributes: vec![],
            SelectTrigger { attributes: vec![], SelectValue { attributes: vec![] } }
            SelectList { attributes: vec![],
                SelectGroup { attributes: vec![],
                    for (index, option) in options.iter().enumerate() {
                        SelectOption::<String> {
                            key: "{option.value}",
                            value: Into::<ReadSignal<String>>::into(Signal::new(option.value.clone())),
                            index,
                            text_value: Some(option.label.clone()),
                            attributes: vec![],
                            "{option.label}"
                            SelectItemIndicator {}
                        }
                    }
                }
            }
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
        div { class: "flex items-center justify-between gap-4 rounded-md border border-border/60 bg-muted/30 px-3 py-2",
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
            div { class: "rounded-md border border-dashed border-border/60 px-3 py-4 text-center text-xs text-muted-foreground",
                {translate("settings-empty")}
            }
        };
    };
    if matches!(first, SettingsRenderItem::Value { .. }) {
        return rsx! {
            div { class: "flex flex-col gap-1 rounded-md border border-border/60 bg-muted/30 p-2",
                for (index, item_id) in items.into_iter().enumerate() {
                    if let Some(SettingsRenderItem::Value { text }) = catalog.item(item_id) {
                        div { key: "{index}", class: "rounded bg-muted/40 px-2 py-1 font-mono text-[11px] text-foreground",
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
        div { class: "rounded-xl border border-border bg-muted/30 p-4",
            div { class: "mb-3 flex items-center justify-between gap-2",
                div { class: "min-w-0",
                    div { class: "truncate text-sm font-semibold text-foreground", "{title}" }
                }
                span { class: "rounded-full border border-border px-2 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground",
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
                    SettingsWriteIntent::send(
                        &path,
                        serde_json::json!({
                            "key": key,
                            "ctrl": modifiers.contains(Modifiers::CONTROL),
                            "shift": modifiers.contains(Modifiers::SHIFT),
                            "alt": modifiers.contains(Modifiers::ALT),
                            "super_key": modifiers.contains(Modifiers::META),
                        }),
                    );
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
                class: "inline-flex cursor-pointer items-center rounded-md border border-border bg-muted px-2 py-1 font-mono text-[11px] text-foreground transition-colors hover:border-foreground/40 hover:bg-muted/70",
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
        span { class: "inline-flex items-center rounded-md border border-border bg-muted px-2 py-1 font-mono text-[11px] text-foreground",
            "{text}"
        }
    }
}

struct SettingsWriteIntent;

impl SettingsWriteIntent {
    fn send(path: &str, value: Value) {
        let _ = send(&SettingsRequest {
            path: path.to_string(),
            value: value.into(),
        });
    }
}
