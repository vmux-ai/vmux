use crate::{
    ArgsPatch, GalleryConfigureRequest, GalleryPresetRequest, GalleryResetRequest, GalleryUiState,
};
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::checkbox::Checkbox;
use vmux_ui::components::input::Input;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;

#[component]
pub(super) fn StoryControls(state: GalleryUiState) -> Element {
    let revision = state.revision;
    let fields = state.story.control_names();
    rsx! {
        details { class: "mb-5 rounded-lg bg-foreground/[0.025] p-3 text-xs", open: true,
            summary { class: "cursor-pointer font-mono font-medium", {translate("gallery-controls")} }
            div { class: "mt-3 flex flex-wrap gap-2",
                Button { variant: ButtonVariant::Secondary, onclick: move |_| { let _ = send(&GalleryResetRequest); }, {translate("gallery-reset")} }
                for (index, preset) in state.presets.iter().enumerate() {
                    Button { variant: ButtonVariant::Secondary,
                        onclick: move |_| { let _ = send(&GalleryPresetRequest { revision, index }); },
                        "{preset.name}"
                    }
                }
            }
            div { class: "mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3",
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
                label { class: "flex flex-col gap-1 font-mono", "text",
                    Input { value: state.text.clone(), aria_label: "text", class: "text-xs",
                        oninput: move |event: FormEvent| { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { text: Some(event.value()), ..Default::default() } }); },
                    }
                }
                }
                if fields.contains(&"progress") {
                label { class: "flex flex-col gap-1 font-mono", "progress",
                    Input { r#type: "number", min: "0", max: "100", value: "{state.progress}", aria_label: "progress", class: "text-xs",
                        oninput: move |event: FormEvent| { if let Ok(value) = event.value().parse() { let _ = send(&GalleryConfigureRequest { revision, args: ArgsPatch { progress: Some(value), ..Default::default() } }); } },
                    }
                }
                }
                if fields.contains(&"cursor") {
                label { class: "flex flex-col gap-1 font-mono", "cursor",
                    Input { r#type: "number", min: "0", max: "16", value: "{state.cursor}", aria_label: "cursor", class: "text-xs",
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
    rsx! { label { class: "flex items-center gap-2 font-mono",
        Checkbox { checked: value, on_checked_change: onchange }
        "{label}"
    } }
}

#[component]
fn ChoiceControl(
    label: String,
    value: String,
    options: Vec<String>,
    onchange: EventHandler<String>,
) -> Element {
    rsx! { label { class: "flex flex-col gap-1 font-mono", "{label}",
        select { class: "h-8 rounded-md bg-background px-2 text-xs text-foreground", value,
            onchange: move |event| onchange.call(event.value()),
            for option in options { option { value: option.clone(), "{option}" } }
        }
    } }
}
