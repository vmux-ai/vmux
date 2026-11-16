#![allow(non_snake_case)]

use dioxus::prelude::*;
use vmux_ui::components::avatar::Avatar;
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant};
use vmux_ui::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use vmux_ui::components::checkbox::{Checkbox, CheckboxTone};
use vmux_ui::components::dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle};
use vmux_ui::components::input::Input;
use vmux_ui::components::progress::{LoadingIcon, Progress, ProgressIndicator};
use vmux_ui::components::select::{Select, SelectList, SelectOption, SelectTrigger, SelectValue};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::switch::{Switch, SwitchThumb};
use vmux_ui::components::textarea::{Textarea, TextareaVariant};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;

use crate::{
    GalleryCheckedRequest, GalleryChoiceRequest, GalleryClickRequest, GalleryDialogRequest,
    GalleryDisabledRequest, GalleryProgressRequest, GalleryResetRequest, GallerySelectRequest,
    GalleryTextRequest, GalleryUiState, Story,
};

#[vmux_page::page(
    component = Page,
    states = ["PageAddressUiState", "ThemeUiState", "GalleryUiState"]
)]
pub(crate) struct GalleryPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let binding = use_ui_state::<GalleryUiState>();
    let state = binding.state;
    let mut filter = use_signal(String::new);
    let current = state();
    rsx! {
        div { class: "flex h-full min-h-0 flex-col bg-background font-sans text-foreground",
            header { class: "flex flex-wrap items-center justify-between gap-3 px-5 py-4",
                div {
                    h1 { class: "text-base font-semibold", {translate("gallery-title")} }
                    p { class: "mt-1 text-xs text-muted-foreground", {translate("gallery-description")} }
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    size: ButtonSize::Sm,
                    onclick: move |_| { let _ = send(&GalleryResetRequest); },
                    {translate("gallery-reset")}
                }
            }
            div { class: "flex min-h-0 flex-1",
                nav { class: "flex w-32 shrink-0 flex-col gap-2 overflow-y-auto bg-foreground/[0.025] p-3 sm:w-44",
                    aria_label: translate("gallery-title"),
                    Input {
                        class: "w-full text-xs",
                        value: filter(),
                        placeholder: translate("gallery-search"),
                        aria_label: translate("gallery-search"),
                        oninput: move |event: FormEvent| filter.set(event.value()),
                    }
                    for story in Story::ALL {
                        if story.name().to_ascii_lowercase().contains(&filter().to_ascii_lowercase()) {
                            Button {
                                key: "{story.name()}",
                                size: ButtonSize::Block,
                                variant: if story == current.story { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                                aria_current: if story == current.story { "true" } else { "false" },
                                onclick: move |_| { let _ = send(&GallerySelectRequest { story }); },
                                "{story.name()}"
                            }
                        }
                    }
                }
                main { class: "flex min-w-0 flex-1 flex-col gap-6 overflow-y-auto p-3 sm:p-6",
                    div { class: "flex flex-wrap items-center justify-between gap-4",
                        h2 { class: "font-mono text-lg font-semibold", "{current.story.name()}" }
                        label { class: "flex items-center gap-2 text-xs text-muted-foreground",
                            Checkbox {
                                checked: current.disabled,
                                on_checked_change: move |disabled| { let _ = send(&GalleryDisabledRequest { disabled }); },
                            }
                            {translate("gallery-disabled")}
                        }
                    }
                    section { class: "flex min-h-64 items-center justify-center rounded-xl bg-card p-3 sm:p-6",
                        aria_label: translate("gallery-preview"),
                        Preview { key: "{current.revision}", state: current.clone() }
                    }
                    section { class: "space-y-3 text-sm",
                        h3 { class: "font-medium", {translate("gallery-events")} }
                        p { class: "text-xs text-muted-foreground", {translate("gallery-events-description")} }
                        if let Some(error) = (binding.error)() {
                            pre { role: "alert", class: "whitespace-pre-wrap text-xs text-destructive", "{error}" }
                        }
                        output { class: "block overflow-x-auto rounded-lg bg-foreground/[0.035] p-4 font-mono text-xs", aria_live: "polite",
                            if current.interaction_count == 0 {
                                {translate("gallery-no-events")}
                            } else {
                                "#{current.interaction_count} {current.last_event}"
                            }
                        }
                        pre { class: "overflow-x-auto whitespace-pre-wrap font-mono text-xs text-muted-foreground",
                            "disabled: {current.disabled}\nchecked: {current.checked}\ntext: {current.text:?}\nchoice: {current.choice:?}\ndialog_open: {current.dialog_open}\nprogress: {current.progress}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Preview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    match state.story {
        Story::Button => rsx! { Buttons { disabled: state.disabled, revision } },
        Story::Input => rsx! {
            Input {
                class: "w-full max-w-sm",
                value: state.text,
                disabled: state.disabled,
                placeholder: translate("gallery-sample"),
                aria_label: "Input",
                oninput: move |event: FormEvent| { let _ = send(&GalleryTextRequest { revision, text: event.value() }); },
            }
        },
        Story::Textarea => rsx! { Textareas { state } },
        Story::Checkbox => rsx! {
            div { class: "flex flex-col gap-4",
                for tone in [CheckboxTone::Primary, CheckboxTone::Neutral] {
                    label { class: "flex items-center gap-3 text-sm",
                        Checkbox {
                            tone,
                            checked: state.checked,
                            disabled: state.disabled,
                            on_checked_change: move |checked| { let _ = send(&GalleryCheckedRequest { revision, checked }); },
                        }
                        {translate("gallery-sample")}
                    }
                }
            }
        },
        Story::Switch => rsx! {
            label { class: "flex items-center gap-3 text-sm",
                Switch {
                    checked: Some(state.checked),
                    disabled: state.disabled,
                    on_checked_change: move |checked| { let _ = send(&GalleryCheckedRequest { revision, checked }); },
                    SwitchThumb {}
                }
                {translate("gallery-sample")}
            }
        },
        Story::Select => rsx! {
            Select::<String> {
                value: Some(state.choice),
                disabled: state.disabled,
                on_value_change: move |choice: Option<String>| {
                    if let Some(choice) = choice { let _ = send(&GalleryChoiceRequest { revision, choice }); }
                },
                SelectTrigger { aria_label: "Select", SelectValue {} }
                SelectList {
                    for (index, choice) in ["one", "two", "three"].into_iter().enumerate() {
                        SelectOption::<String> {
                            value: choice.to_string(),
                            index,
                            text_value: translate(&format!("gallery-option-{choice}")),
                            {translate(&format!("gallery-option-{choice}"))}
                        }
                    }
                }
            }
        },
        Story::Dialog => rsx! { DialogPreview { state } },
        Story::Card => rsx! {
            Card { class: "w-full max-w-sm",
                CardHeader {
                    CardTitle { {translate("gallery-sample")} }
                    CardDescription { {translate("gallery-description")} }
                }
                CardContent {
                    Button {
                        disabled: state.disabled,
                        size: ButtonSize::Sm,
                        onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "Card".to_string() }); },
                        {translate("gallery-sample")}
                    }
                }
            }
        },
        Story::Avatar => rsx! {
            div { class: "flex flex-col gap-6",
                Input {
                    value: state.text.clone(),
                    disabled: state.disabled,
                    aria_label: translate("gallery-sample"),
                    placeholder: translate("gallery-sample"),
                    oninput: move |event: FormEvent| { let _ = send(&GalleryTextRequest { revision, text: event.value() }); },
                }
                div { class: "flex items-center gap-4",
                    for class in ["size-6", "size-10", "size-16"] {
                        Avatar { seed: state.text.clone(), background: "", alt: translate("gallery-sample"), class }
                    }
                }
            }
        },
        Story::Progress => rsx! {
            div { class: "flex w-full max-w-sm flex-col gap-6",
                Progress { value: state.progress, max: 100.0, aria_label: "Progress", ProgressIndicator {} }
                input {
                    r#type: "range", min: "0", max: "100", value: "{state.progress}",
                    disabled: state.disabled,
                    aria_label: "Progress",
                    oninput: move |event| {
                        if let Ok(value) = event.value().parse() { let _ = send(&GalleryProgressRequest { revision, value }); }
                    },
                }
                Progress { value: None, aria_label: "Progress", ProgressIndicator {} }
                LoadingIcon {}
            }
        },
        Story::Skeleton => rsx! {
            div { class: "flex w-full max-w-sm items-center gap-4",
                Skeleton { class: "size-12 rounded-full" }
                div { class: "flex flex-1 flex-col gap-3",
                    Skeleton { class: "h-4 w-full" }
                    Skeleton { class: "h-4 w-2/3" }
                }
            }
        },
    }
}

#[component]
fn Buttons(disabled: bool, revision: u64) -> Element {
    rsx! {
        div { class: "flex w-full flex-col gap-5",
            for (variant, name) in [
                (ButtonVariant::Primary, "Primary"),
                (ButtonVariant::Secondary, "Secondary"),
                (ButtonVariant::Destructive, "Destructive"),
                (ButtonVariant::Outline, "Outline"),
                (ButtonVariant::Ghost, "Ghost"),
            ] {
                div { class: "flex flex-wrap items-center gap-3",
                    span { class: "w-20 font-mono text-xs text-muted-foreground", "{name}" }
                    for (size, label) in [(ButtonSize::Xs, "Xs"), (ButtonSize::Sm, "Sm"), (ButtonSize::Md, "Md")] {
                        Button {
                            variant, size, disabled,
                            onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: format!("{name}/{label}") }); },
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Textareas(state: GalleryUiState) -> Element {
    let revision = state.revision;
    rsx! {
        div { class: "flex w-full max-w-md flex-col gap-4",
            for variant in [TextareaVariant::Default, TextareaVariant::Fade, TextareaVariant::Outline, TextareaVariant::Ghost] {
                label { class: "flex flex-col gap-2",
                    span { class: "font-mono text-xs text-muted-foreground", "{variant.class()}" }
                    Textarea {
                        variant,
                        value: state.text.clone(),
                        disabled: state.disabled,
                        placeholder: translate("gallery-sample"),
                        oninput: move |event: FormEvent| { let _ = send(&GalleryTextRequest { revision, text: event.value() }); },
                    }
                }
            }
        }
    }
}

#[component]
fn DialogPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    rsx! {
        Button {
            disabled: state.disabled,
            onclick: move |_| { let _ = send(&GalleryDialogRequest { revision, open: true }); },
            {translate("gallery-open-dialog")}
        }
        DialogRoot {
            open: Some(state.dialog_open),
            on_open_change: move |open| { let _ = send(&GalleryDialogRequest { revision, open }); },
            DialogContent {
                DialogTitle { {translate("gallery-sample")} }
                DialogDescription { {translate("gallery-description")} }
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| { let _ = send(&GalleryDialogRequest { revision, open: false }); },
                    {translate("gallery-close")}
                }
            }
        }
    }
}
