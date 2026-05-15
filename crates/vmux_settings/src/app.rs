#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_settings::event::{SETTINGS_LIST_EVENT, SettingsCommandEvent, SettingsListEvent};
use vmux_ui::hooks::{try_cef_bin_emit_rkyv, use_bin_event_listener, use_theme};

fn emit_update(path: &str, value: serde_json::Value) {
    let _ = try_cef_bin_emit_rkyv(&SettingsCommandEvent {
        path: path.to_string(),
        value: value.to_string(),
    });
}

#[component]
pub fn App() -> Element {
    use_theme();
    let mut snapshot = use_signal(|| serde_json::Value::Null);

    let _listener =
        use_bin_event_listener::<SettingsListEvent, _>(SETTINGS_LIST_EVENT, move |data| {
            let parsed: serde_json::Value =
                serde_json::from_str(&data.json).unwrap_or(serde_json::Value::Null);
            snapshot.set(parsed);
        });

    let s = snapshot.read().clone();
    if s.is_null() {
        return rsx! {
            div { class: "flex h-full items-center justify-center text-sm text-muted-foreground",
                "Loading settings..."
            }
        };
    }

    rsx! {
        div { class: "flex h-full min-h-0 flex-col overflow-y-auto bg-background text-foreground",
            div { class: "border-b border-border px-6 py-4",
                h1 { class: "text-lg font-semibold", "Settings" }
                p { class: "mt-1 text-xs text-muted-foreground",
                    "Stored in ~/Library/Application Support/Vmux/settings.ron"
                }
            }
            div { class: "flex flex-col gap-6 p-6",
                SectionGeneral { snapshot: s.clone() }
                SectionWindow { snapshot: s.clone() }
                SectionPane { snapshot: s.clone() }
                SectionSideSheet { snapshot: s.clone() }
                SectionFocusRing { snapshot: s.clone() }
                SectionShortcuts { snapshot: s.clone() }
                SectionTerminal { snapshot: s.clone() }
            }
        }
    }
}

fn pick_bool(snapshot: &serde_json::Value, path: &[&str], default: bool) -> bool {
    walk(snapshot, path)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(default)
}

fn pick_f64(snapshot: &serde_json::Value, path: &[&str], default: f64) -> f64 {
    walk(snapshot, path)
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(default)
}

fn pick_u64(snapshot: &serde_json::Value, path: &[&str], default: u64) -> u64 {
    walk(snapshot, path)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(default)
}

fn pick_string(snapshot: &serde_json::Value, path: &[&str]) -> String {
    walk(snapshot, path)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn walk<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    Some(cursor)
}

#[component]
fn Card(title: String, children: Element) -> Element {
    rsx! {
        div { class: "rounded-lg border border-border bg-card p-4",
            h2 { class: "mb-3 text-sm font-semibold uppercase tracking-wide text-muted-foreground", "{title}" }
            div { class: "flex flex-col gap-3", {children} }
        }
    }
}

#[component]
fn FieldNumber(label: String, path: String, value: f64, step: f64) -> Element {
    let path_for_input = path.clone();
    rsx! {
        label { class: "flex items-center justify-between gap-3 text-sm",
            span { class: "text-foreground", "{label}" }
            input {
                r#type: "number",
                class: "w-32 rounded border border-border bg-background px-2 py-1 text-right text-foreground",
                step: "{step}",
                value: "{value}",
                oninput: move |e| {
                    if let Ok(parsed) = e.value().parse::<f64>() {
                        emit_update(&path_for_input, serde_json::json!(parsed));
                    }
                },
            }
        }
    }
}

#[component]
fn FieldInt(label: String, path: String, value: u64) -> Element {
    let path_for_input = path.clone();
    rsx! {
        label { class: "flex items-center justify-between gap-3 text-sm",
            span { class: "text-foreground", "{label}" }
            input {
                r#type: "number",
                class: "w-32 rounded border border-border bg-background px-2 py-1 text-right text-foreground",
                step: "1",
                value: "{value}",
                oninput: move |e| {
                    if let Ok(parsed) = e.value().parse::<u64>() {
                        emit_update(&path_for_input, serde_json::json!(parsed));
                    }
                },
            }
        }
    }
}

#[component]
fn FieldBool(label: String, path: String, value: bool) -> Element {
    let path_for_input = path.clone();
    rsx! {
        label { class: "flex items-center justify-between gap-3 text-sm",
            span { class: "text-foreground", "{label}" }
            input {
                r#type: "checkbox",
                class: "h-4 w-4",
                checked: value,
                onchange: move |e| {
                    let checked = e.value() == "true";
                    emit_update(&path_for_input, serde_json::json!(checked));
                },
            }
        }
    }
}

#[component]
fn FieldText(label: String, path: String, value: String) -> Element {
    let path_for_input = path.clone();
    rsx! {
        label { class: "flex flex-col gap-1 text-sm",
            span { class: "text-foreground", "{label}" }
            input {
                r#type: "text",
                class: "rounded border border-border bg-background px-2 py-1 text-foreground",
                value: "{value}",
                oninput: move |e| {
                    emit_update(&path_for_input, serde_json::json!(e.value()));
                },
            }
        }
    }
}

#[component]
fn SectionGeneral(snapshot: serde_json::Value) -> Element {
    let auto_update = pick_bool(&snapshot, &["auto_update"], true);
    let startup_url = pick_string(&snapshot, &["startup_url"]);
    rsx! {
        Card { title: "General".to_string(),
            FieldBool {
                label: "Auto-update".to_string(),
                path: "auto_update".to_string(),
                value: auto_update,
            }
            FieldText {
                label: "Startup URL (empty = vmux://vibe/)".to_string(),
                path: "startup_url".to_string(),
                value: startup_url,
            }
        }
    }
}

#[component]
fn SectionWindow(snapshot: serde_json::Value) -> Element {
    let padding = pick_f64(&snapshot, &["layout", "window", "padding"], 4.0);
    rsx! {
        Card { title: "Window".to_string(),
            FieldNumber {
                label: "Padding (px)".to_string(),
                path: "layout.window.padding".to_string(),
                value: padding,
                step: 1.0,
            }
        }
    }
}

#[component]
fn SectionPane(snapshot: serde_json::Value) -> Element {
    let gap = pick_f64(&snapshot, &["layout", "pane", "gap"], 8.0);
    let radius = pick_f64(&snapshot, &["layout", "pane", "radius"], 8.0);
    rsx! {
        Card { title: "Pane".to_string(),
            FieldNumber {
                label: "Gap (px)".to_string(),
                path: "layout.pane.gap".to_string(),
                value: gap,
                step: 1.0,
            }
            FieldNumber {
                label: "Corner radius (px)".to_string(),
                path: "layout.pane.radius".to_string(),
                value: radius,
                step: 1.0,
            }
        }
    }
}

#[component]
fn SectionSideSheet(snapshot: serde_json::Value) -> Element {
    let width = pick_f64(&snapshot, &["layout", "side_sheet", "width"], 280.0);
    rsx! {
        Card { title: "Side sheet".to_string(),
            FieldNumber {
                label: "Width (px)".to_string(),
                path: "layout.side_sheet.width".to_string(),
                value: width,
                step: 4.0,
            }
        }
    }
}

#[component]
fn SectionFocusRing(snapshot: serde_json::Value) -> Element {
    let width = pick_f64(&snapshot, &["layout", "focus_ring", "width"], 2.0);
    let glow_spread = pick_f64(&snapshot, &["layout", "focus_ring", "glow", "spread"], 8.0);
    let glow_intensity = pick_f64(
        &snapshot,
        &["layout", "focus_ring", "glow", "intensity"],
        0.45,
    );
    let gradient_enabled = pick_bool(
        &snapshot,
        &["layout", "focus_ring", "gradient", "enabled"],
        true,
    );
    let gradient_speed = pick_f64(
        &snapshot,
        &["layout", "focus_ring", "gradient", "speed"],
        0.6,
    );
    let gradient_cycles = pick_f64(
        &snapshot,
        &["layout", "focus_ring", "gradient", "cycles"],
        1.0,
    );
    rsx! {
        Card { title: "Focus ring".to_string(),
            FieldNumber {
                label: "Width (px)".to_string(),
                path: "layout.focus_ring.width".to_string(),
                value: width,
                step: 0.5,
            }
            FieldNumber {
                label: "Glow spread".to_string(),
                path: "layout.focus_ring.glow.spread".to_string(),
                value: glow_spread,
                step: 0.5,
            }
            FieldNumber {
                label: "Glow intensity".to_string(),
                path: "layout.focus_ring.glow.intensity".to_string(),
                value: glow_intensity,
                step: 0.05,
            }
            FieldBool {
                label: "Gradient enabled".to_string(),
                path: "layout.focus_ring.gradient.enabled".to_string(),
                value: gradient_enabled,
            }
            FieldNumber {
                label: "Gradient speed".to_string(),
                path: "layout.focus_ring.gradient.speed".to_string(),
                value: gradient_speed,
                step: 0.1,
            }
            FieldNumber {
                label: "Gradient cycles".to_string(),
                path: "layout.focus_ring.gradient.cycles".to_string(),
                value: gradient_cycles,
                step: 0.1,
            }
        }
    }
}

#[component]
fn SectionShortcuts(snapshot: serde_json::Value) -> Element {
    let timeout = pick_u64(&snapshot, &["shortcuts", "chord_timeout_ms"], 1000);
    let leader = walk(&snapshot, &["shortcuts", "leader"])
        .map(|v| v.to_string())
        .unwrap_or_else(|| "(none)".to_string());
    let bindings = walk(&snapshot, &["shortcuts", "bindings"])
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {
        Card { title: "Shortcuts".to_string(),
            FieldInt {
                label: "Chord timeout (ms)".to_string(),
                path: "shortcuts.chord_timeout_ms".to_string(),
                value: timeout,
            }
            div { class: "text-xs text-muted-foreground",
                "Leader: " span { class: "font-mono text-foreground", "{leader}" }
            }
            div { class: "rounded border border-border bg-background p-2 text-xs",
                div { class: "mb-2 font-medium text-muted-foreground", "Bindings (read-only)" }
                for (i, binding) in bindings.iter().enumerate() {
                    div { key: "{i}", class: "font-mono", "{binding}" }
                }
            }
        }
    }
}

#[component]
fn SectionTerminal(snapshot: serde_json::Value) -> Element {
    let confirm_close = pick_bool(&snapshot, &["terminal", "confirm_close"], true);
    let default_theme = pick_string(&snapshot, &["terminal", "default_theme"]);
    let themes = walk(&snapshot, &["terminal", "themes"])
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {
        Card { title: "Terminal".to_string(),
            FieldBool {
                label: "Confirm close".to_string(),
                path: "terminal.confirm_close".to_string(),
                value: confirm_close,
            }
            FieldText {
                label: "Default theme name".to_string(),
                path: "terminal.default_theme".to_string(),
                value: default_theme,
            }
            for (i, theme) in themes.iter().enumerate() {
                ThemeSubcard { index: i, theme: theme.clone() }
            }
        }
    }
}

#[component]
fn ThemeSubcard(index: usize, theme: serde_json::Value) -> Element {
    let name = theme
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("(unnamed)")
        .to_string();
    let font_family = theme
        .get("font_family")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let font_size = theme
        .get("font_size")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(14.0);
    let line_height = theme
        .get("line_height")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(1.2);
    let padding = theme
        .get("padding")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(4.0);
    let cursor_blink = theme
        .get("cursor_blink")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let shell = theme
        .get("shell")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    rsx! {
        div { class: "rounded border border-border bg-background p-3",
            div { class: "mb-2 text-xs font-semibold text-muted-foreground", "Theme: {name}" }
            FieldText {
                label: "Font family".to_string(),
                path: format!("terminal.themes[{index}].font_family"),
                value: font_family,
            }
            FieldNumber {
                label: "Font size".to_string(),
                path: format!("terminal.themes[{index}].font_size"),
                value: font_size,
                step: 0.5,
            }
            FieldNumber {
                label: "Line height".to_string(),
                path: format!("terminal.themes[{index}].line_height"),
                value: line_height,
                step: 0.05,
            }
            FieldNumber {
                label: "Padding".to_string(),
                path: format!("terminal.themes[{index}].padding"),
                value: padding,
                step: 0.5,
            }
            FieldBool {
                label: "Cursor blink".to_string(),
                path: format!("terminal.themes[{index}].cursor_blink"),
                value: cursor_blink,
            }
            FieldText {
                label: "Shell".to_string(),
                path: format!("terminal.themes[{index}].shell"),
                value: shell,
            }
        }
    }
}
