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
use vmux_ui::components::breadcrumb::{BreadcrumbOption, BreadcrumbSelector, BreadcrumbSeparator};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::input::Input;
use vmux_ui::components::manager::{
    ManagerSearchAction, ManagerSelect, ManagerSelectItem, WorkspaceBreadcrumbHeader, WorkspacePage,
};
use vmux_ui::components::switch::{Switch, SwitchThumb};
use vmux_ui::dioxus_ext::attributes;
use vmux_ui::focus::FocusClaim;
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;

const SETTINGS_SCROLL_SPY: &str = r#"
globalThis.__vmuxSettingsScrollSpyCleanup?.();
const viewport = document.getElementById('settings-scroll-viewport');
const sections = [...document.querySelectorAll('[data-vmux-settings-section]')];
if (viewport && sections.length) {
  let active = '';
  let frame = 0;
  const update = () => {
    frame = 0;
    const threshold = viewport.getBoundingClientRect().top + 24;
    let selected = sections[0];
    for (const section of sections) {
      if (section.getBoundingClientRect().top <= threshold) selected = section;
      else break;
    }
    if (viewport.scrollTop + viewport.clientHeight >= viewport.scrollHeight - 2) {
      selected = sections[sections.length - 1];
    }
    if (selected.id !== active) {
      active = selected.id;
      dioxus.send(active);
    }
  };
  const schedule = () => {
    if (frame) return;
    frame = requestAnimationFrame(update);
  };
  const observer = new ResizeObserver(schedule);
  observer.observe(viewport);
  for (const section of sections) observer.observe(section);
  viewport.addEventListener('scroll', schedule, { passive: true });
  window.addEventListener('resize', schedule, true);
  globalThis.__vmuxSettingsScrollSpyCleanup = () => {
    if (frame) cancelAnimationFrame(frame);
    observer.disconnect();
    viewport.removeEventListener('scroll', schedule);
    window.removeEventListener('resize', schedule, true);
    delete globalThis.__vmuxSettingsScrollSpyCleanup;
  };
  update();
}
"#;

#[vmux_page::page(
    component = Page,
    states = ["ThemeUiState", "SettingsUiState"]
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
    let mut active_section = use_signal(|| {
        sections
            .first()
            .map(|section| section.id.clone())
            .unwrap_or_default()
    });
    let current_section = sections
        .iter()
        .find(|section| section.id == active_section())
        .or_else(|| sections.first())
        .cloned();
    let section_options = sections
        .iter()
        .map(|section| BreadcrumbOption {
            label: section.title.clone(),
            value: section.id.clone(),
            identity_seed: String::new(),
            icon_url: String::new(),
            icon: None,
        })
        .collect::<Vec<_>>();
    rsx! {
        div { class: "flex h-full min-h-0 flex-col bg-background text-foreground",
            WorkspaceBreadcrumbHeader {
                current: WorkspacePage::Settings.option(),
                options: WorkspacePage::options(),
                actions: rsx! {
                    ManagerSearchAction {
                        value: query.clone(),
                        placeholder: translate("command-search"),
                        onsearch: move |query| {
                            send(&SettingsFilterRequest { query });
                        },
                    }
                },
                if let Some(section) = current_section {
                    BreadcrumbSeparator {}
                    BreadcrumbSelector {
                        label: section.title,
                        value: section.id,
                        options: section_options,
                        active: true,
                        onselect: move |id: String| {
                            active_section.set(id.clone());
                            SettingsSectionNavigation::open(&id);
                        },
                    }
                }
            }
            div { class: "flex min-h-0 flex-1 flex-row",
                aside { class: "hidden w-56 shrink-0 px-4 py-4 lg:block",
                    nav { class: "flex flex-col gap-0.5",
                        for section in &sections {
                            button {
                                key: "{section.id}",
                                r#type: "button",
                                class: if section.id == active_section() {
                                    "rounded-md bg-foreground/[0.065] px-2 py-1.5 text-left text-sm text-foreground"
                                } else {
                                    "rounded-md px-2 py-1.5 text-left text-sm text-muted-foreground transition-colors hover:bg-foreground/[0.04] hover:text-foreground"
                                },
                                onclick: {
                                    let id = section.id.clone();
                                    move |_| {
                                        active_section.set(id.clone());
                                        SettingsSectionNavigation::open(&id);
                                    }
                                },
                                "{section.title}"
                            }
                        }
                    }
                }
                main { id: "settings-scroll-viewport", class: "min-h-0 min-w-0 flex-1 overflow-y-auto",
                    SettingsScrollSpy {
                        active: active_section,
                    }
                    div { class: "mx-auto max-w-3xl px-6 py-6 lg:px-10",
                        div { class: "mb-8 lg:hidden",
                            p { class: "text-sm text-muted-foreground",
                                {translate("settings-stored")}
                            }
                        }
                        div { class: "flex flex-col",
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
}

#[component]
fn SectionView(section: SettingsSection, catalog: SettingsRenderCatalog) -> Element {
    rsx! {
        section {
            id: "{section.id}",
            class: "scroll-mt-3 border-b border-foreground/[0.07] py-6 first:pt-3 last:border-b-0",
            "data-vmux-settings-section": "",
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

struct SettingsSectionNavigation;

impl SettingsSectionNavigation {
    fn open(id: &str) {
        let id = serde_json::to_string(id).unwrap_or_else(|_| "\"\"".to_string());
        let _ = document::eval(&format!(
            "document.getElementById({id})?.scrollIntoView({{behavior:'smooth',block:'start'}});"
        ));
    }
}

#[component]
fn SettingsScrollSpy(active: Signal<String>) -> Element {
    use_future(move || async move {
        let mut eval = document::eval(SETTINGS_SCROLL_SPY);
        while let Ok(id) = eval.recv::<String>().await {
            active.set(id);
        }
    });
    use_drop(move || {
        let _ = document::eval("globalThis.__vmuxSettingsScrollSpyCleanup?.();");
    });
    rsx! {}
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
                            send(&CheckForUpdatesEvent);
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
                    send(&SettingsRequest {
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
                send(&SettingsEditRequest {
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
                send(&SettingsEditRequest {
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
                send(&SettingsEditRequest {
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
                send(&SettingsRequest {
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
                    send(&SettingsRequest {
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
