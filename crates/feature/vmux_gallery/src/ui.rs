#![allow(non_snake_case)]

use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_ui::components::avatar::{AgentThumbnail, Avatar, Identicon, IdentityOrb};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant, IconButton};
use vmux_ui::components::card::{
    Card, CardContent, CardDescription, CardHeader, CardTitle, CardVariant,
};
use vmux_ui::components::checkbox::{Checkbox, CheckboxTone};
use vmux_ui::components::dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle};
use vmux_ui::components::input::{ActionInput, Input};
use vmux_ui::components::manager::{
    ManagerPage, ManagerSearch, ManagerSearchRow, ManagerSidebarItem, WorkspaceLayout,
};
use vmux_ui::components::page::PageContent;
use vmux_ui::components::page_address::{PageBreadcrumbs, PageToolbar, ToolbarControl};
use vmux_ui::components::progress::{LoadingIcon, Progress, ProgressIndicator};
use vmux_ui::components::select::{
    Select, SelectGroup, SelectItemIndicator, SelectList, SelectOption, SelectTrigger, SelectValue,
};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::switch::{Switch, SwitchThumb};
use vmux_ui::components::textarea::{Textarea, TextareaVariant};
use vmux_ui::hooks::{send, use_theme, use_ui_state};
use vmux_ui::i18n::translate;
use vmux_ui::navigation::PageNavigation;

mod auxiliary;
mod composer;
mod editing;
mod management;
mod menus;
mod navigation;
mod surfaces;

use auxiliary::AuxiliaryPreview;
use composer::ComposerPreview;
use editing::EditingPreview;
use management::ManagementPreview;
use menus::MenuPreview;
use navigation::NavigationPreview;
use surfaces::SurfacePreview;

use crate::{
    GalleryCheckedRequest, GalleryChoiceRequest, GalleryClickRequest, GalleryDialogRequest,
    GalleryDisabledRequest, GalleryProgressRequest, GalleryResetRequest, GalleryTextRequest,
    GalleryUiState, Story,
};

#[vmux_page::page(
    component = Page,
    states = ["PageAddressUiState", "ThemeUiState", "GalleryUiState"],
    subtree
)]
pub(crate) struct GalleryPage;

#[component]
pub fn Page() -> Element {
    use_theme();
    let binding = use_ui_state::<GalleryUiState>();
    let state = binding.state;
    let mut filter = use_signal(String::new);
    let current = state();
    let mut toolbar = Vec::new();
    toolbar.push(ToolbarControl::button(
        translate("gallery-reset"),
        move |_| {
            let _ = send(&GalleryResetRequest);
        },
    ));
    let disabled = current.disabled;
    toolbar.push(
        ToolbarControl::button(translate("gallery-disabled"), move |_| {
            let _ = send(&GalleryDisabledRequest {
                disabled: !disabled,
            });
        })
        .active(disabled),
    );
    rsx! {
        document::Title { "{current.story.name()}" }
        ManagerPage {
            PageBreadcrumbs { state: current.address.clone() }
            WorkspaceLayout {
                resizable: true,
                search: rsx! { ManagerSearchRow {
                    ManagerSearch {
                        value: filter(), placeholder: translate("gallery-search"),
                        onsearch: move |query| filter.set(query),
                    }
                } },
                sidebar: rsx! {
                    nav { class: "flex flex-col gap-0.5",
                        for story in Story::ALL {
                            if story.component_names().to_ascii_lowercase().contains(&filter().to_ascii_lowercase()) {
                                ManagerSidebarItem {
                                    key: "{story.slug()}",
                                    label: story.name().to_string(),
                                    count: 0, show_count: false,
                                    selected: story == current.story,
                                    onclick: move |event| PageNavigation::click(story.url(), &event),
                                }
                            }
                        }
                    }
                },
                toolbar: rsx! { PageToolbar { controls: toolbar } },
                PageContent { content_class: "mx-auto w-full max-w-5xl",
                    div { class: "mb-5 flex flex-wrap items-center justify-between gap-3",
                        h1 { class: "font-mono text-base font-semibold", "{current.story.name()}" }
                        span { class: "text-xs text-muted-foreground", {translate("gallery-description")} }
                    }
                    p { class: "mb-3 font-mono text-[11px] text-muted-foreground", {current.story.component_names()} }
                    section { class: "min-h-64 rounded-lg bg-foreground/[0.025] p-4",
                        aria_label: translate("gallery-preview"),
                        Preview { key: "{current.revision}", state: current.clone() }
                    }
                    details { class: "mt-6 text-xs text-muted-foreground", open: true,
                        summary { class: "mb-3 cursor-pointer font-medium", {translate("gallery-events")} }
                        p { class: "mb-3", {translate("gallery-events-description")} }
                        if let Some(error) = (binding.error)() {
                            pre { role: "alert", class: "whitespace-pre-wrap text-destructive", "{error}" }
                        }
                        output { class: "block overflow-x-auto rounded-lg bg-foreground/[0.035] p-3 font-mono", aria_live: "polite",
                            if current.interaction_count == 0 { {translate("gallery-no-events")} }
                            else { "#{current.interaction_count} {current.last_event}" }
                        }
                        pre { class: "mt-3 overflow-x-auto whitespace-pre-wrap font-mono",
                            "disabled: {current.disabled}\nchecked: {current.checked}\ntext: {current.text:?}\nchoice: {current.choice:?}\nmenu: {current.menu:?}\nagent: {current.agent:?}\nmodel: {current.model:?}\neffort: {current.effort:?}\npermission: {current.permission:?}\nproject: {current.project:?}\nbranch: {current.branch:?}\nrunning: {current.running}\nsubmitted: {current.submitted:?}"
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
        Story::Favicon
        | Story::TypeIcon
        | Story::PromptGhost
        | Story::DirectoryNavigator
        | Story::MatrixRain
        | Story::BackButton => rsx! { AuxiliaryPreview { state } },
        Story::PromptComposer | Story::ComposerBar | Story::PromptBox => {
            rsx! { ComposerPreview { state } }
        }
        Story::AgentMenu
        | Story::ModelMenu
        | Story::EffortMenu
        | Story::PermissionMenu
        | Story::ProjectPicker
        | Story::McpMenu
        | Story::PromptMediaOptions => rsx! { MenuPreview { state } },
        Story::Address
        | Story::Breadcrumbs
        | Story::Dropdown
        | Story::PageAddress
        | Story::SegmentedControl
        | Story::Sheet
        | Story::FileTree => rsx! { NavigationPreview { state } },
        Story::AlertDialog
        | Story::ContextMenu
        | Story::Badge
        | Story::ShellCode
        | Story::ErrorState
        | Story::Filter
        | Story::Icon
        | Story::Motion
        | Story::Notice
        | Story::Page
        | Story::StartHero
        | Story::Tooltip => rsx! { SurfacePreview { state } },
        Story::Manager => rsx! { ManagementPreview { state } },
        Story::InlineEdit => rsx! { EditingPreview { state } },
        Story::Button => rsx! { Buttons { disabled: state.disabled, revision } },
        Story::Input => rsx! {
            div { class: "mx-auto w-full max-w-xl space-y-6",
            Input {
                class: "w-full max-w-sm",
                value: state.text.clone(),
                disabled: state.disabled,
                placeholder: translate("gallery-sample"),
                aria_label: "Input",
                oninput: move |event: FormEvent| { let _ = send(&GalleryTextRequest { revision, text: event.value() }); },
            }
            ActionInput {
                value: state.text.clone(), placeholder: translate("gallery-sample"),
                aria_label: "ActionInput", action_label: translate("gallery-sample"),
                class: "relative flex w-full items-center", input_class: "w-full rounded-md bg-background p-2",
                oninput: move |event: FormEvent| { let _ = send(&GalleryTextRequest { revision, text: event.value() }); },
                onkeydown: move |event: KeyboardEvent| { if event.key() == Key::Enter { let _ = send(&GalleryClickRequest { revision, label: "ActionInput.enter".into() }); } },
                onblur: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ActionInput.blur".into() }); },
                onaction: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ActionInput.action".into() }); },
                "+"
            }
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
                SelectList { SelectGroup {
                    for (index, choice) in ["one", "two", "three"].into_iter().enumerate() {
                        SelectOption::<String> {
                            value: choice.to_string(),
                            index,
                            text_value: translate(&format!("gallery-option-{choice}")),
                            {translate(&format!("gallery-option-{choice}"))}
                            SelectItemIndicator {}
                        }
                    }
                } }
            }
        },
        Story::Dialog => rsx! { DialogPreview { state } },
        Story::Card => rsx! {
            div { class: "flex flex-wrap gap-6",
                for variant in [CardVariant::Default, CardVariant::Panel] {
                    Card { variant, class: "w-full max-w-sm",
                        CardHeader {
                            CardTitle { {translate("gallery-sample")} }
                            CardDescription { {translate("gallery-description")} }
                        }
                        CardContent {
                            Button { disabled: state.disabled, size: ButtonSize::Sm,
                                onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "Card".into() }); },
                                {translate("gallery-sample")}
                            }
                        }
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
                div { class: "flex items-center gap-4",
                    AgentThumbnail { seed: state.text.clone(), background: "", alt: "AgentThumbnail", class: "size-10" }
                    Identicon { seed: state.text.clone(), alt: "Identicon", class: "size-10" }
                    IdentityOrb { seed: state.text.clone(), alt: "IdentityOrb", class: "size-10" }
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
            div { class: "flex flex-wrap items-center gap-3",
                IconButton { label: "IconButton", paths: vec!["M12 5v14M5 12h14".into()], disabled,
                    onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "IconButton".into() }); },
                }
                Button { size: ButtonSize::Icon, disabled, "I" }
                Button { size: ButtonSize::Block, disabled,
                    onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "Button/Block".into() }); },
                    "Block"
                }
            }
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
