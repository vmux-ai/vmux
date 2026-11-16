use crate::{ArgsPatch, GalleryConfigureRequest, GalleryPresetRequest, GalleryUiState};
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::input::Input;
use vmux_ui::components::switch::{Switch, SwitchThumb};
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[component]
pub(super) fn StoryControls(state: GalleryUiState) -> Element {
    let revision = state.revision;
    let fields = state.story.control_names();
    rsx! {
        details { class: "rounded-xl bg-foreground/[0.025] p-4 text-xs", open: true,
            summary { class: "cursor-pointer text-sm font-medium", {translate("gallery-controls")} }
            h2 { class: "mt-5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60", {translate("gallery-presets")} }
            div { class: "mt-2 flex flex-wrap gap-1.5",
                for (index, preset) in state.presets.iter().enumerate() {
                    Button { variant: ButtonVariant::Secondary, size: ButtonSize::Xs, class: "rounded-md",
                        onclick: move |_| { let _ = send(&GalleryPresetRequest { revision, index }); },
                        {translate(&crate::StoryPreset::label_key(preset))}
                    }
                }
            }
            h2 { class: "mb-2 mt-5 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60", {translate("gallery-properties")} }
            div { class: "grid grid-cols-[repeat(auto-fit,minmax(160px,1fr))] items-end gap-3",
                if fields.contains(&"choice") { ChoiceControl { label: "choice", value: state.choice.clone(), options: vec!["one".into(), "two".into(), "three".into(), "list".into(), "grid".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { choice: Some(value), ..Default::default() } }); } } }
                if fields.contains(&"button_variant") { ChoiceControl { label: "button_variant", value: state.button_variant.clone(), options: vec!["all".into(), "primary".into(), "secondary".into(), "destructive".into(), "outline".into(), "ghost".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { button_variant: Some(value), ..Default::default() } }); } } }
                if fields.contains(&"button_size") { ChoiceControl { label: "button_size", value: state.button_size.clone(), options: vec!["all".into(), "xs".into(), "sm".into(), "md".into(), "icon".into(), "block".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { button_size: Some(value), ..Default::default() } }); } } }

                if fields.contains(&"disabled") {
            BoolControl { label: "disabled", value: state.disabled, onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { disabled: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"checked") {
            BoolControl { label: "checked", value: state.checked, onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { checked: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"dialog_open") {
            BoolControl { label: "dialog_open", value: state.dialog_open, onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { dialog_open: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"running") {
            BoolControl { label: "running", value: state.running, onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { running: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"has_attachment") {
            BoolControl { label: "has_attachment", value: state.has_attachment, onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { has_attachment: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"agent") {
            ChoiceControl { label: "agent", value: state.agent.clone(), options: vec!["codex".into(), "claude".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { agent: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"model") {
            ChoiceControl { label: "model", value: state.model.clone(), options: vec!["balanced".into(), "fast".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { model: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"effort") {
            ChoiceControl { label: "effort", value: state.effort.clone(), options: vec!["".into(), "low".into(), "medium".into(), "high".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { effort: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"permission") {
            ChoiceControl { label: "permission", value: state.permission.clone(), options: vec!["default".into(), "plan".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { permission: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"project") {
            ChoiceControl { label: "project", value: state.project.clone(), options: vec!["/example/vmux".into(), "/example/library".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { project: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"branch") {
            ChoiceControl { label: "branch", value: state.branch.clone(), options: vec!["main".into(), "feature/ui".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { branch: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"status") {
            ChoiceControl { label: "status", value: state.status.clone(), options: vec!["idle".into(), "streaming".into(), "awaiting".into(), "installing".into(), "errored".into(), "in_use".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { status: Some(value), ..Default::default() } }); } }
                }
                if fields.contains(&"menu") {
            ChoiceControl { label: "menu", value: state.menu.clone(), options: vec!["".into(), "agent".into(), "model".into(), "effort".into(), "permission".into(), "project".into(), "branch".into(), "dropdown".into(), "dropdown-selector".into(), "breadcrumb".into(), "prompt".into(), "prompt-flat".into(), "mcp".into(), "project-select".into(), "manager-search".into()], onchange: move |value| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { menu: Some(value), ..Default::default() } }); } }
                }

                if fields.contains(&"text") {
                label { class: "flex flex-col gap-1.5 font-mono text-[11px] text-muted-foreground", "text",
                    Input { value: state.text.clone(), aria_label: "text", class: "h-8 bg-foreground/[0.035] text-xs",
                        oninput: move |event: FormEvent| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { text: Some(event.value()), ..Default::default() } }); },
                    }
                }
                }
                if fields.contains(&"progress") {
                label { class: "flex flex-col gap-1.5 font-mono text-[11px] text-muted-foreground", "progress",
                    Input { r#type: "number", min: "0", max: "100", value: "{state.progress}", aria_label: "progress", class: "h-8 bg-foreground/[0.035] text-xs",
                        oninput: move |event: FormEvent| { if let Ok(value) = event.value().parse() { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { progress: Some(value), ..Default::default() } }); } },
                    }
                }
                }
                if fields.contains(&"cursor") {
                label { class: "flex flex-col gap-1.5 font-mono text-[11px] text-muted-foreground", "cursor",
                    Input { r#type: "number", min: "0", max: "16", value: "{state.cursor}", aria_label: "cursor", class: "h-8 bg-foreground/[0.035] text-xs",
                        oninput: move |event: FormEvent| { if let Ok(value) = event.value().parse() { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { cursor: Some(value), ..Default::default() } }); } },
                    }
                }
                }
            }
        }
    }
}

#[component]
fn BoolControl(label: String, value: bool, onchange: EventHandler<bool>) -> Element {
    rsx! { label { class: "flex flex-col gap-1.5",
        span { class: "font-mono text-[11px] text-muted-foreground", "{label}" }
        div { class: "flex h-8 items-center justify-between gap-3 rounded-md bg-foreground/[0.035] px-3",
            code { class: "text-xs text-foreground", "{value}" }
            Switch { checked: Some(value), on_checked_change: onchange, SwitchThumb {} }
        }
    } }
}

#[component]
fn ChoiceControl(
    label: String,
    value: String,
    options: Vec<String>,
    onchange: EventHandler<String>,
) -> Element {
    rsx! { label { class: "flex flex-col gap-1.5",
        span { class: "font-mono text-[11px] text-muted-foreground", "{label}" }
        select { class: "h-8 w-full rounded-md bg-foreground/[0.035] px-2 text-xs text-foreground outline-none focus:ring-1 focus:ring-primary/40", value,
            onchange: move |event| onchange.call(event.value()),
            for option in options { option { value: option.clone(), "{option}" } }
        }
    } }
}
