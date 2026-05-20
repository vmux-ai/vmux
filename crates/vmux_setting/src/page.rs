#![allow(non_snake_case)]

use dioxus::prelude::*;
use serde_json::{Map, Value};
use vmux_setting::event::{
    SETTINGS_LIST_EVENT, SETTINGS_SCHEMA_EVENT, SettingsCommandEvent, SettingsListEvent,
    SettingsSchemaEvent,
};
use vmux_setting::schema::{SettingsSchema, WidgetKind};
use vmux_ui::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use vmux_ui::components::input::Input;
use vmux_ui::dioxus_ext::attributes;
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};
use wasm_bindgen::JsCast;

#[component]
pub fn Page() -> Element {
    use_theme();
    let mut snapshot = use_signal(|| Value::Null);
    let mut schema = use_signal(SettingsSchema::default);

    let _values =
        use_bin_event_listener::<SettingsListEvent, _>(SETTINGS_LIST_EVENT, move |data| {
            let parsed: Value = serde_json::from_str(&data.json).unwrap_or(Value::Null);
            snapshot.set(parsed);
        });

    let _schema =
        use_bin_event_listener::<SettingsSchemaEvent, _>(SETTINGS_SCHEMA_EVENT, move |data| {
            if let Ok(parsed) = serde_json::from_str::<SettingsSchema>(&data.json) {
                schema.set(parsed);
            }
        });

    let s = snapshot.read().clone();
    if s.is_null() {
        return rsx! {
            div { class: "flex h-full items-center justify-center text-sm text-muted-foreground",
                "Loading settings…"
            }
        };
    }
    let sch = schema.read().clone();

    let top = match s.as_object() {
        Some(obj) => obj.clone(),
        None => Map::new(),
    };
    let sections = compute_sections(&top, &sch);

    rsx! {
        div { class: "flex h-full min-h-0 flex-row bg-background text-foreground",
            aside { class: "hidden w-56 shrink-0 border-r border-border px-4 py-6 lg:block",
                div { class: "mb-4 px-2",
                    div { class: "text-base font-semibold tracking-tight", "Settings" }
                    div { class: "mt-0.5 text-[11px] text-muted-foreground", "settings.ron" }
                }
                nav { class: "flex flex-col gap-0.5",
                    for sec in &sections {
                        a {
                            key: "{sec.id}",
                            href: "#{sec.id}",
                            class: "rounded-md px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground",
                            "{sec.title}"
                        }
                    }
                }
            }
            main { class: "min-w-0 flex-1 overflow-y-auto",
                div { class: "mx-auto max-w-3xl px-6 py-8 lg:px-10",
                    div { class: "mb-8 lg:hidden",
                        h1 { class: "text-xl font-semibold tracking-tight", "Settings" }
                        p { class: "mt-1 text-sm text-muted-foreground",
                            "Stored in ~/Library/Application Support/Vmux/settings.ron"
                        }
                    }
                    div { class: "flex flex-col gap-8",
                        for sec in sections {
                            SectionView {
                                key: "{sec.id}",
                                id: sec.id,
                                title: sec.title,
                                description: sec.description,
                                root_path: sec.root_path,
                                value: sec.value,
                                schema: sch.clone(),
                            }
                        }
                    }
                }
            }
        }
    }
}

fn emit_update(path: &str, value: Value) {
    let _ = try_cef_bin_emit_rkyv(&SettingsCommandEvent {
        path: path.to_string(),
        value: value.to_string(),
    });
}

#[derive(Clone)]
struct PreparedSection {
    id: String,
    title: String,
    description: Option<String>,
    root_path: String,
    value: Value,
}

fn compute_sections(top: &Map<String, Value>, schema: &SettingsSchema) -> Vec<PreparedSection> {
    let mut out = Vec::new();
    let mut consumed: std::collections::HashSet<String> = std::collections::HashSet::new();

    if !schema.sections.is_empty() {
        for spec in &schema.sections {
            if !spec.synthetic_keys.is_empty() {
                let mut obj = Map::new();
                for k in &spec.synthetic_keys {
                    if let Some(v) = top.get(k) {
                        obj.insert(k.clone(), v.clone());
                        consumed.insert(k.clone());
                    }
                }
                if obj.is_empty() {
                    continue;
                }
                out.push(PreparedSection {
                    id: spec.id.clone(),
                    title: spec.title.clone(),
                    description: spec.description.clone(),
                    root_path: spec.root_path.clone(),
                    value: Value::Object(obj),
                });
            } else if let Some(v) = top.get(&spec.root_path) {
                consumed.insert(spec.root_path.clone());
                out.push(PreparedSection {
                    id: spec.id.clone(),
                    title: spec.title.clone(),
                    description: spec.description.clone(),
                    root_path: spec.root_path.clone(),
                    value: v.clone(),
                });
            }
        }
    }

    let mut leftover_scalars = Map::new();
    for (k, v) in top {
        if consumed.contains(k) {
            continue;
        }
        if v.is_object() {
            out.push(PreparedSection {
                id: k.clone(),
                title: snake_to_title(k),
                description: None,
                root_path: k.clone(),
                value: v.clone(),
            });
        } else {
            leftover_scalars.insert(k.clone(), v.clone());
        }
    }
    if !leftover_scalars.is_empty() {
        let synthetic = PreparedSection {
            id: "general-extra".to_string(),
            title: "Other".to_string(),
            description: None,
            root_path: String::new(),
            value: Value::Object(leftover_scalars),
        };
        if out.iter().any(|s| s.id == "general") {
            out.push(synthetic);
        } else {
            out.insert(0, synthetic);
        }
    }
    out
}

#[component]
fn SectionView(
    id: String,
    title: String,
    description: Option<String>,
    root_path: String,
    value: Value,
    schema: SettingsSchema,
) -> Element {
    rsx! {
        section { id: "{id}", class: "scroll-mt-6",
            Card {
                CardHeader {
                    CardTitle { "{title}" }
                    if let Some(desc) = description {
                        CardDescription { "{desc}" }
                    }
                }
                CardContent {
                    div { class: "flex flex-col divide-y divide-border",
                        ObjectBody { value, parent_path: root_path, depth: 0, schema }
                    }
                }
            }
        }
    }
}

#[component]
fn ObjectBody(value: Value, parent_path: String, depth: usize, schema: SettingsSchema) -> Element {
    let obj = match value.as_object() {
        Some(o) => o.clone(),
        None => return rsx! {},
    };
    let order = schema
        .field(&parent_path)
        .map(|f| f.order.clone())
        .unwrap_or_default();
    let keys = order_keys(&obj, &order);
    rsx! {
        for key in keys {
            FieldView {
                key: "{key}",
                name: key.clone(),
                value: obj[&key].clone(),
                parent_path: parent_path.clone(),
                depth,
                schema: schema.clone(),
            }
        }
    }
}

#[component]
fn FieldView(
    name: String,
    value: Value,
    parent_path: String,
    depth: usize,
    schema: SettingsSchema,
) -> Element {
    let path = if parent_path.is_empty() {
        name.clone()
    } else {
        format!("{parent_path}.{name}")
    };
    let spec = schema.field(&path).cloned().unwrap_or_default();
    if spec.omit {
        return rsx! {};
    }
    let label = spec.label.clone().unwrap_or_else(|| snake_to_title(&name));
    let hint = spec.hint.clone();

    if let Some(widget) = spec.widget {
        return rsx! {
            WidgetView { widget, path, value, label, hint }
        };
    }

    match &value {
        Value::Bool(b) => rsx! {
            Row { label, hint,
                control: rsx! { Toggle { path: path.clone(), value: *b } },
            }
        },
        Value::Number(n) if n.is_u64() => rsx! {
            Row { label, hint,
                control: rsx! {
                    IntInput { path: path.clone(), value: n.as_u64().unwrap_or(0) }
                },
            }
        },
        Value::Number(n) => rsx! {
            Row { label, hint,
                control: rsx! {
                    NumberInput {
                        path: path.clone(),
                        value: n.as_f64().unwrap_or(0.0),
                        step: spec.step.unwrap_or(1.0),
                    }
                },
            }
        },
        Value::String(s) => rsx! {
            StackedRow { label, hint,
                control: rsx! {
                    TextInput {
                        path: path.clone(),
                        value: s.clone(),
                        placeholder: spec.placeholder.clone(),
                    }
                },
            }
        },
        Value::Object(_) => rsx! {
            div { class: "flex flex-col py-3 first:pt-0 last:pb-0",
                SubgroupHeading { label: label.clone() }
                if let Some(h) = hint {
                    p { class: "px-1 pb-2 text-xs text-muted-foreground", "{h}" }
                }
                div { class: "flex flex-col divide-y divide-border/60 rounded-md border border-border/40 bg-muted/20 px-3",
                    ObjectBody { value, parent_path: path.clone(), depth: depth + 1, schema }
                }
            }
        },
        Value::Array(arr) => rsx! {
            div { class: "flex flex-col py-3 first:pt-0 last:pb-0",
                div { class: "flex items-center justify-between pb-2",
                    SubgroupHeading { label: label.clone() }
                    span { class: "text-[11px] text-muted-foreground", "{arr.len()}" }
                }
                if let Some(h) = hint {
                    p { class: "pb-2 text-xs text-muted-foreground", "{h}" }
                }
                ArrayBody {
                    items: arr.clone(),
                    parent_path: path.clone(),
                    depth: depth + 1,
                    schema,
                }
            }
        },
        Value::Null => rsx! {},
    }
}

#[component]
fn ArrayBody(
    items: Vec<Value>,
    parent_path: String,
    depth: usize,
    schema: SettingsSchema,
) -> Element {
    if items.is_empty() {
        return rsx! {
            div { class: "rounded-md border border-dashed border-border/60 px-3 py-4 text-center text-xs text-muted-foreground",
                "(empty)"
            }
        };
    }
    let all_objects = items.iter().all(Value::is_object);
    if !all_objects {
        return rsx! {
            div { class: "flex flex-col gap-1 rounded-md border border-border/60 bg-muted/30 p-2",
                for (i, item) in items.iter().enumerate() {
                    div { key: "{i}", class: "rounded bg-muted/40 px-2 py-1 font-mono text-[11px] text-foreground",
                        "{item}"
                    }
                }
            }
        };
    }
    rsx! {
        div { class: "flex flex-col gap-3",
            for (i, item) in items.iter().cloned().enumerate() {
                ArrayItemCard {
                    key: "{i}",
                    index: i,
                    item: item,
                    parent_path: parent_path.clone(),
                    depth,
                    schema: schema.clone(),
                }
            }
        }
    }
}

#[component]
fn ArrayItemCard(
    index: usize,
    item: Value,
    parent_path: String,
    depth: usize,
    schema: SettingsSchema,
) -> Element {
    let title = item
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("Item {}", index + 1));
    let item_path = format!("{parent_path}[{index}]");
    rsx! {
        div { class: "rounded-xl border border-border bg-muted/30 p-4",
            div { class: "mb-3 flex items-center justify-between gap-2",
                div { class: "min-w-0",
                    div { class: "truncate text-sm font-semibold text-foreground", "{title}" }
                }
                span { class: "rounded-full border border-border px-2 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground",
                    "Item"
                }
            }
            div { class: "flex flex-col divide-y divide-border/60",
                ObjectBody { value: item, parent_path: item_path, depth, schema }
            }
        }
    }
}

#[component]
fn WidgetView(
    widget: WidgetKind,
    path: String,
    value: Value,
    label: String,
    hint: Option<String>,
) -> Element {
    match widget {
        WidgetKind::LeaderKbd => {
            let text = format_combo(&value);
            rsx! {
                Row { label, hint,
                    control: rsx! { ChordEditor { path: path.clone(), text } },
                }
            }
        }
        WidgetKind::BindingsList => {
            let arr = value.as_array().cloned().unwrap_or_default();
            rsx! {
                div { class: "flex flex-col py-3 first:pt-0 last:pb-0",
                    div { class: "mb-3 flex items-center justify-between gap-2",
                        div { class: "text-sm font-medium text-foreground", "{label}" }
                        span { class: "text-[11px] text-muted-foreground", "{arr.len()}" }
                    }
                    if let Some(h) = hint {
                        p { class: "mb-2 text-xs text-muted-foreground", "{h}" }
                    }
                    if arr.is_empty() {
                        div { class: "text-xs text-muted-foreground", "(none)" }
                    } else {
                        div { class: "flex flex-col gap-1",
                            for (i, binding) in arr.iter().enumerate() {
                                BindingRow { key: "{i}", index: i, binding: binding.clone() }
                            }
                        }
                    }
                }
            }
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

#[component]
fn Row(label: String, hint: Option<String>, control: Element) -> Element {
    rsx! {
        div { class: "flex items-center justify-between gap-6 py-3 first:pt-0 last:pb-0",
            div { class: "min-w-0 flex-1",
                div { class: "text-sm font-medium text-foreground", "{label}" }
                if let Some(h) = hint {
                    p { class: "mt-0.5 text-xs leading-snug text-muted-foreground", "{h}" }
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
                if let Some(h) = hint {
                    p { class: "text-xs leading-snug text-muted-foreground", "{h}" }
                }
            }
            {control}
        }
    }
}

#[component]
fn NumberInput(path: String, value: f64, step: f64) -> Element {
    let path_for_input = path.clone();
    rsx! {
        Input {
            attributes: attributes!(input {
                r#type: "number",
                step: "{step}",
                value: "{value}",
                class: "w-24 text-right tabular-nums text-sm",
            }),
            oninput: move |e: FormEvent| {
                if let Ok(parsed) = e.value().parse::<f64>() {
                    emit_update(&path_for_input, serde_json::json!(parsed));
                }
            },
            placeholder: None::<String>,
            children: rsx! {},
        }
    }
}

#[component]
fn IntInput(path: String, value: u64) -> Element {
    let path_for_input = path.clone();
    rsx! {
        Input {
            attributes: attributes!(input {
                r#type: "number",
                step: "1",
                value: "{value}",
                class: "w-24 text-right tabular-nums text-sm",
            }),
            oninput: move |e: FormEvent| {
                if let Ok(parsed) = e.value().parse::<u64>() {
                    emit_update(&path_for_input, serde_json::json!(parsed));
                }
            },
            placeholder: None::<String>,
            children: rsx! {},
        }
    }
}

#[component]
fn TextInput(path: String, value: String, placeholder: Option<String>) -> Element {
    let path_for_input = path.clone();
    let placeholder_attr = placeholder.unwrap_or_default();
    rsx! {
        Input {
            attributes: attributes!(input {
                r#type: "text",
                value: "{value}",
                placeholder: "{placeholder_attr}",
                class: "w-full text-sm",
            }),
            oninput: move |e: FormEvent| {
                emit_update(&path_for_input, serde_json::json!(e.value()));
            },
            placeholder: None::<String>,
            children: rsx! {},
        }
    }
}

#[component]
fn Toggle(path: String, value: bool) -> Element {
    let path_for_input = path.clone();
    let track_class = if value {
        "relative h-6 w-10 cursor-pointer rounded-full bg-primary transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
    } else {
        "relative h-6 w-10 cursor-pointer rounded-full bg-muted shadow-[inset_0_0_0_1px_var(--border)] transition-colors hover:bg-muted/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
    };
    let thumb_class = if value {
        "absolute top-0.5 left-[1.125rem] h-5 w-5 rounded-full bg-background shadow-sm transition-transform"
    } else {
        "absolute top-0.5 left-0.5 h-5 w-5 rounded-full bg-foreground/80 shadow-sm transition-transform"
    };
    rsx! {
        button {
            r#type: "button",
            class: "{track_class}",
            "aria-pressed": if value { "true" } else { "false" },
            onclick: move |_| {
                emit_update(&path_for_input, serde_json::json!(!value));
            },
            span { class: "{thumb_class}" }
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

#[component]
fn BindingRow(index: usize, binding: Value) -> Element {
    let command = binding
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("(unknown)")
        .to_string();
    let chord = binding
        .get("binding")
        .map(format_binding)
        .unwrap_or_else(|| "(none)".to_string());
    let edit_path = binding.get("binding").and_then(|b| {
        if b.get("Direct").is_some() {
            Some(format!("shortcuts.bindings[{index}].binding.Direct"))
        } else if b.get("Leader").is_some() {
            Some(format!("shortcuts.bindings[{index}].binding.Leader"))
        } else {
            None
        }
    });
    rsx! {
        div { class: "flex items-center justify-between gap-4 rounded-md border border-border/60 bg-muted/30 px-3 py-2",
            span { class: "truncate font-mono text-xs text-foreground", "{command}" }
            if let Some(path) = edit_path {
                ChordEditor { path, text: chord }
            } else {
                Kbd { text: chord }
            }
        }
    }
}

#[component]
fn ChordEditor(path: String, text: String) -> Element {
    let mut recording = use_signal(|| false);
    let path_for_capture = path.clone();
    let mut feedback = use_signal(|| None::<String>);

    use_effect(move || {
        if !recording() {
            return;
        }
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("vmux-settings-key-capture"))
            && let Ok(html) = el.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = html.focus();
        }
    });

    if recording() {
        let preview = feedback().unwrap_or_else(|| "Press a key…".to_string());
        rsx! {
            button {
                r#type: "button",
                id: "vmux-settings-key-capture",
                tabindex: "0",
                class: "inline-flex animate-pulse items-center gap-2 rounded-md border border-primary bg-primary/15 px-3 py-1 font-mono text-[11px] text-foreground outline-none",
                onkeydown: move |e: KeyboardEvent| {
                    e.prevent_default();
                    e.stop_propagation();
                    let key = e.key();
                    if key == Key::Escape {
                        recording.set(false);
                        feedback.set(None);
                        return;
                    }
                    if matches!(key, Key::Control | Key::Shift | Key::Alt | Key::Meta) {
                        return;
                    }
                    let key_str = match key {
                        Key::Character(s) => s,
                        other => other.to_string(),
                    };
                    let mods = e.modifiers();
                    let combo = serde_json::json!({
                        "key": key_str,
                        "ctrl": mods.contains(Modifiers::CONTROL),
                        "shift": mods.contains(Modifiers::SHIFT),
                        "alt": mods.contains(Modifiers::ALT),
                        "super_key": mods.contains(Modifiers::META),
                    });
                    emit_update(&path_for_capture, combo);
                    feedback.set(Some("Saved".to_string()));
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
                title: "Click to record a new key combo",
                onclick: move |_| {
                    feedback.set(None);
                    recording.set(true);
                },
                "{text}"
            }
        }
    }
}

fn format_combo(combo: &Value) -> String {
    let mut parts = Vec::new();
    if combo.get("ctrl").and_then(Value::as_bool).unwrap_or(false) {
        parts.push("Ctrl".to_string());
    }
    if combo
        .get("super_key")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        parts.push("⌘".to_string());
    }
    if combo.get("alt").and_then(Value::as_bool).unwrap_or(false) {
        parts.push("Alt".to_string());
    }
    if combo.get("shift").and_then(Value::as_bool).unwrap_or(false) {
        parts.push("Shift".to_string());
    }
    if let Some(key) = combo.get("key").and_then(Value::as_str) {
        let pretty = match key {
            "ArrowLeft" => "←".to_string(),
            "ArrowRight" => "→".to_string(),
            "ArrowUp" => "↑".to_string(),
            "ArrowDown" => "↓".to_string(),
            "Tab" => "Tab".to_string(),
            "Enter" => "↵".to_string(),
            "Space" => "␣".to_string(),
            other if other.len() == 1 => other.to_uppercase(),
            other => other.to_string(),
        };
        parts.push(pretty);
    }
    if parts.is_empty() {
        "(none)".to_string()
    } else {
        parts.join(" + ")
    }
}

fn format_binding(binding: &Value) -> String {
    if let Some(direct) = binding.get("Direct") {
        return format_combo(direct);
    }
    if let Some(leader) = binding.get("Leader") {
        return format!("Leader → {}", format_combo(leader));
    }
    if let Some(arr) = binding.get("Chord").and_then(Value::as_array) {
        let combos: Vec<String> = arr.iter().map(format_combo).collect();
        return combos.join(" → ");
    }
    binding.to_string()
}

fn snake_to_title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut next_upper = true;
    for ch in s.chars() {
        if ch == '_' || ch == '-' {
            out.push(' ');
            next_upper = true;
        } else if next_upper {
            out.extend(ch.to_uppercase());
            next_upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn order_keys(obj: &Map<String, Value>, order: &[String]) -> Vec<String> {
    let mut out: Vec<String> = order
        .iter()
        .filter(|k| obj.contains_key(*k))
        .cloned()
        .collect();
    let mut rest: Vec<String> = obj
        .keys()
        .filter(|k| !order.iter().any(|o| o == *k))
        .cloned()
        .collect();
    rest.sort();
    out.extend(rest);
    out
}
