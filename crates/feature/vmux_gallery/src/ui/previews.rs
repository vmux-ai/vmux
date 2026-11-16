use crate::{
    GalleryAgentRequest, GalleryAttachRequest, GalleryBranchRequest, GalleryCheckedRequest,
    GalleryChoiceRequest, GalleryClickRequest, GalleryCursorRequest, GalleryDialogRequest,
    GalleryEffortRequest, GalleryMenuRequest, GalleryModelRequest, GalleryPasteRequest,
    GalleryPermissionRequest, GalleryProgressRequest, GalleryProjectRequest,
    GalleryRemoveAttachmentRequest, GalleryStatusRequest, GalleryStopRequest, GallerySubmitRequest,
    GalleryTextRequest, GalleryUiState, Story,
};
use dioxus::prelude::component;
use dioxus::prelude::*;
use std::collections::HashMap;
use vmux_api::address::PageAddressUiState;
use vmux_ui::back::{BackButton, PageBack};
use vmux_ui::components::address::{AddressControls, AddressTrail};
use vmux_ui::components::agent_menu::AgentMenu;
use vmux_ui::components::alert_dialog::*;
use vmux_ui::components::avatar::{AgentThumbnail, Avatar, Identicon, IdentityOrb};
use vmux_ui::components::badge::Badge;
use vmux_ui::components::breadcrumb::{
    BreadcrumbDropdown, BreadcrumbItem, BreadcrumbOption, BreadcrumbOptionIcon, BreadcrumbSelector,
    BreadcrumbSeparator, Breadcrumbs,
};
use vmux_ui::components::button::{Button, ButtonSize, ButtonVariant, IconButton};
use vmux_ui::components::card::{
    Card, CardContent, CardDescription, CardHeader, CardTitle, CardVariant,
};
use vmux_ui::components::checkbox::{Checkbox, CheckboxTone};
use vmux_ui::components::code::ShellCode;
use vmux_ui::components::composer::{ComposerFrame, PromptComposer, PromptComposerMode};
use vmux_ui::components::composer_bar::{
    AgentMenuData, BranchMenuData, ComposerBar, ComposerChip, ComposerChipVisual, ComposerMenuKind,
    ComposerMenus, EffortMenuData, ModelMenuData, PermissionMenuData, ProjectMenuData,
};
use vmux_ui::components::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use vmux_ui::components::dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle};
use vmux_ui::components::dropdown::{
    Dropdown, DropdownOption, DropdownPopup, DropdownSelector, DropdownToggleIndicator,
    DropdownTrigger,
};
use vmux_ui::components::effort_menu::EffortMenu;
use vmux_ui::components::error_state::ErrorState;
use vmux_ui::components::filter::{FilterMenu, FilterOption, FilterPill, FilterTrigger};
use vmux_ui::components::icon::Icon;
use vmux_ui::components::inline_edit::{EditableText, InlineEdit, InlineTextarea};
use vmux_ui::components::input::{ActionInput, Input};
use vmux_ui::components::manager::*;
use vmux_ui::components::mcp_menu::McpMenu;
use vmux_ui::components::model_menu::ModelMenu;
use vmux_ui::components::motion::{Collapse, Collapsible, ContentTransition};
use vmux_ui::components::notice::{Notice, NoticeKind};
use vmux_ui::components::page::{PageContent, PageHeader, PageSidebarToggle, PageToolbarRow};
use vmux_ui::components::page_address::{AddressSurface, PageToolbar, ToolbarControl};
use vmux_ui::components::permission_menu::PermissionMenu;
use vmux_ui::components::progress::{LoadingIcon, Progress, ProgressIndicator};
use vmux_ui::components::project_picker::{
    BranchPicker, ProjectPicker, ProjectSelectPopup, ProjectSelectTrigger,
};
use vmux_ui::components::prompt_box::{PromptBox, PromptPopup, PromptPopupPlacement};
use vmux_ui::components::prompt_media_options::PromptMediaOptions;
use vmux_ui::components::segmented_control::{SegmentedControl, SegmentedControlItem};
use vmux_ui::components::select::{
    Select, SelectGroup, SelectItemIndicator, SelectList, SelectOption, SelectTrigger, SelectValue,
};
use vmux_ui::components::sheet::{
    SheetHeaderAction, SheetHeaderChevron, SheetSelectorRow, SheetToggleRow,
};
use vmux_ui::components::skeleton::Skeleton;
use vmux_ui::components::start_hero::{StartBackdrop, StartHero};
use vmux_ui::components::switch::{Switch, SwitchThumb};
use vmux_ui::components::textarea::{Textarea, TextareaVariant};
use vmux_ui::components::tooltip::Tooltip;
use vmux_ui::components::tree_row::{
    FileTree, FileTreeChildren, FileTreeHeader, FileTreeRow, FileTreeRowGroup, FileTreeScroller,
    FileTreeSkeleton,
};
use vmux_ui::directory::{DirectoryEntryVisual, DirectoryNavigator};
use vmux_ui::favicon::{Favicon, GlobeIcon};
use vmux_ui::file_icon::TypeIcon;
use vmux_ui::hooks::{send, use_ui_state};
use vmux_ui::i18n::translate;
use vmux_ui::icon::{
    BuiltinIconView, GitIconView, GitSymbol, GitSymbolView, LineIcon, LineIconView, PageIconView,
};
use vmux_ui::matrix_rain::{MatrixLoader, MatrixRain};
use vmux_ui::prompt_ghost::PromptGhost;

#[component]
pub(crate) fn AuxiliaryPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    use_context_provider(move || {
        PageBack::new(EventHandler::new(move |_| {
            let _ = send(&GalleryClickRequest {
                revision,
                label: "BackButton".into(),
            });
        }))
    });
    let count = state.examples.files.len().max(1);
    rsx! {
        fieldset { disabled: state.disabled, class: "space-y-6",
            match state.story {
                Story::Favicon => rsx! {
                    div { class: "flex items-center gap-6",
                        Favicon { favicon_url: "assets/agent-logos/claude.png", url: "", class: "size-8" }
                        Favicon { favicon_url: "", url: "about:blank", class: "size-8", globe_class: "size-8" }
                        GlobeIcon { class: "size-8" }
                    }
                },
                Story::TypeIcon => rsx! {
                    div { class: "flex items-center gap-6",
                        for path in ["example.rs", "Cargo.toml", "src"] {
                            TypeIcon { path, is_dir: path == "src", class: "size-8" }
                        }
                    }
                },
                Story::PromptGhost => rsx! {
                    div { class: "max-w-xl space-y-8 overflow-hidden",
                        PromptGhost { accent_bg: "bg-primary/10", terminal: false }
                        PromptGhost { accent_bg: "bg-primary/10", terminal: true }
                    }
                },
                Story::MatrixRain => rsx! {
                    div { class: "relative h-56 overflow-hidden rounded-lg",
                        MatrixRain { accent_rgb: "var(--primary)", words: vec!["VMUX".into(), "UI".into()], columns: 24 }
                    }
                    MatrixLoader { label: translate("gallery-sample"), class: "h-56 w-full", columns: 24 }
                },
                Story::BackButton => rsx! { BackButton {} },
                Story::DirectoryNavigator => rsx! {
                    div { class: "flex h-80 overflow-auto rounded-lg bg-foreground/[0.025]",
                        DirectoryNavigator {
                            path: if state.text.is_empty() { "/example".into() } else { state.text.clone() },
                            parent_entries: state.examples.files.clone(),
                            entries: state.examples.files.clone(),
                            children: Some(state.examples.files.clone()),
                            selected: state.cursor,
                            thumbs: HashMap::new(),
                            preview: rsx! { {translate("gallery-sample")} },
                            on_select: move |(cursor, entry): (usize, vmux_ecs::event::FileDirEntry)| {
                                let _ = send(&GalleryCursorRequest { revision, cursor });
                                let _ = send(&GalleryClickRequest { revision, label: format!("DirectoryNavigator.select:{}", entry.name) });
                            },
                            on_ascend: move |text| { let _ = send(&GalleryTextRequest { revision, text }); },
                            on_descend: move |text| { let _ = send(&GalleryTextRequest { revision, text }); },
                            on_open: move |entry: vmux_ecs::event::FileDirEntry| { let _ = send(&GalleryClickRequest { revision, label: format!("DirectoryNavigator.open:{}", entry.name) }); },
                            on_next: move |_| { let _ = send(&GalleryCursorRequest { revision, cursor: (state.cursor + 1) % count }); },
                            on_previous: move |_| { let _ = send(&GalleryCursorRequest { revision, cursor: (state.cursor + count - 1) % count }); },
                            on_activate: move |_| { let _ = send(&GalleryClickRequest { revision, label: "DirectoryNavigator.activate".into() }); },
                            on_parent: move |_| { let _ = send(&GalleryTextRequest { revision, text: "/example".into() }); },
                            on_toggle_hidden: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                        }
                    }
                    div { class: "flex gap-4",
                        for entry in state.examples.files.clone() {
                            DirectoryEntryVisual { entry, thumb: None }
                        }
                    }
                },
                _ => rsx! {},
            }
        }
    }
}

#[component]
pub(crate) fn BasicPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    match state.story {
        Story::Button => rsx! { Buttons { state } },
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
                fieldset { disabled: state.disabled, class: "contents",
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
        _ => rsx! {},
    }
}

#[component]
fn Buttons(state: GalleryUiState) -> Element {
    let disabled = state.disabled;
    let revision = state.revision;
    if state.button_variant != "all" || state.button_size != "all" {
        return rsx! {
            div { class: "flex w-full flex-col gap-5",
                for (variant, name) in [
                    (ButtonVariant::Primary, "primary"),
                    (ButtonVariant::Secondary, "secondary"),
                    (ButtonVariant::Destructive, "destructive"),
                    (ButtonVariant::Outline, "outline"),
                    (ButtonVariant::Ghost, "ghost"),
                ] {
                    if state.button_variant == "all" || state.button_variant == name {
                        div { class: "flex flex-wrap items-center gap-3",
                            span { class: "w-20 font-mono text-xs text-muted-foreground", "{name}" }
                            for (size, label) in [
                                (ButtonSize::Xs, "xs"),
                                (ButtonSize::Sm, "sm"),
                                (ButtonSize::Md, "md"),
                                (ButtonSize::Icon, "icon"),
                                (ButtonSize::Block, "block"),
                            ] {
                                if state.button_size == "all" || state.button_size == label {
                                    Button { variant, size, disabled,
                                        onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: format!("{name}/{label}") }); },
                                        if state.text.is_empty() { "{label}" } else { "{state.text}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };
    }
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
                if state.button_variant == "all" || state.button_variant == name.to_ascii_lowercase() {
                div { class: "flex flex-wrap items-center gap-3",
                    span { class: "w-20 font-mono text-xs text-muted-foreground", "{name}" }
                    for (size, label) in [(ButtonSize::Xs, "Xs"), (ButtonSize::Sm, "Sm"), (ButtonSize::Md, "Md")] {
                        if state.button_size == "all" || state.button_size == label.to_ascii_lowercase() {
                        Button {
                            variant, size, disabled,
                            onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: format!("{name}/{label}") }); },
                            if state.text.is_empty() { "{label}" } else { "{state.text}" }
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

#[component]
pub(crate) fn ComposerPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    if state.story == Story::PromptBox {
        return rsx! {
            div { class: "mx-auto w-full max-w-xl space-y-6 pb-72",
                for glass in [true, false] {
                    div { class: "relative",
                        p { class: "mb-2 font-mono text-xs text-muted-foreground", "glass: {glass}" }
                        PromptBox { glass,
                            Input { value: state.text.clone(), disabled: state.disabled, class: "min-w-0 flex-1", placeholder: translate("gallery-sample"),
                                oninput: move |event: FormEvent| { let _ = send(&GalleryTextRequest { revision, text: event.value() }); },
                            }
                            Button { disabled: state.disabled, variant: ButtonVariant::Ghost,
                                onclick: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: if glass { "prompt".into() } else { "prompt-flat".into() } }); },
                                "+"
                            }
                            if state.menu == if glass { "prompt" } else { "prompt-flat" } {
                                PromptPopup { heading: "PromptPopup", on_dismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                                    Button { class: "w-full", onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "PromptPopup".into() }); }, {translate("gallery-sample")} }
                                }
                            }
                        }
                    }
                }
            }
        };
    }
    rsx! {
        div { class: "mx-auto w-full max-w-4xl space-y-6 py-16",
            div { class: "flex flex-wrap gap-2",
                Button { variant: ButtonVariant::Secondary,
                    onclick: move |_| { let _ = send(&GalleryTextRequest { revision, text: translate("gallery-example-review") }); },
                    {translate("gallery-sample")}
                }
                Button { variant: ButtonVariant::Ghost,
                    onclick: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                    "notice / completion: {state.checked}"
                }
            }
            if state.story == Story::ComposerBar {
                ComposerStrip { state }
            } else {
                ComposerFrame {
                    notice: if state.checked { Some(rsx! { Notice { message: translate("gallery-sample"), attached: true } }) } else { None },
                    PromptComposer {
                        value: state.text.clone(),
                        placeholder: translate("gallery-sample"),
                        accent_color: "var(--primary)",
                        accent_gradient: "from-primary to-primary/70",
                        disabled: state.disabled,
                        attachments: state.examples.attachments.clone(),
                        completion: if state.checked { format!(" {}", translate("gallery-completion")) } else { String::new() },
                        mode: if state.running { PromptComposerMode::Stop } else { PromptComposerMode::Send },
                        action_title: if state.running { translate("common-stop") } else { translate("command-send") },
                        action_enabled: state.running || !state.text.trim().is_empty(),
                        on_input: move |text| { let _ = send(&GalleryTextRequest { revision, text }); },
                        on_keydown: move |event: KeyboardEvent| {
                            if event.key() == Key::Enter && !event.modifiers().shift() {
                                event.prevent_default();
                                let _ = send(&GallerySubmitRequest { revision });
                            }
                        },
                        on_paste: move |_| { let _ = send(&GalleryPasteRequest { revision }); },
                        on_attach: move |_| { let _ = send(&GalleryAttachRequest { revision }); },
                        on_remove_attachment: move |index| { let _ = send(&GalleryRemoveAttachmentRequest { revision, index }); },
                        on_action: move |_| {
                            if state.running { let _ = send(&GalleryStopRequest { revision }); }
                            else { let _ = send(&GallerySubmitRequest { revision }); }
                        },
                        footer: Some(rsx! { ComposerStrip { state: state.clone() } }),
                    }
                }
            }
        }
    }
}

#[component]
fn ComposerStrip(state: GalleryUiState) -> Element {
    let revision = state.revision;
    let examples = crate::Examples::localized(&state.examples);
    let opened = match state.menu.as_str() {
        "agent" => Some(ComposerMenuKind::Agent),
        "model" => Some(ComposerMenuKind::Model),
        "effort" => Some(ComposerMenuKind::Effort),
        "permission" => Some(ComposerMenuKind::Permission),
        "project" => Some(ComposerMenuKind::Project),
        "branch" => Some(ComposerMenuKind::Branch),
        _ => None,
    };
    rsx! {
        div { class: "relative @container",
            ComposerBar {
                opened,
                agent: Some(ComposerChip { label: state.agent.clone(), title: "AgentMenu".into(), visual: ComposerChipVisual::Agent { icon: Default::default(), seed: state.agent.clone() }, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "agent".into() }); })) }),
                model: Some(ComposerChip { label: state.model.clone(), title: "ModelMenu".into(), visual: ComposerChipVisual::Kind, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "model".into() }); })) }),
                effort: Some(ComposerChip { label: state.effort.clone(), title: "EffortMenu".into(), visual: ComposerChipVisual::Kind, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "effort".into() }); })) }),
                permission: Some(ComposerChip { label: state.permission.clone(), title: "PermissionMenu".into(), visual: ComposerChipVisual::Kind, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "permission".into() }); })) }),
                project: Some(ComposerChip { label: state.project.clone(), title: "ProjectPicker".into(), visual: ComposerChipVisual::Identity { seed: state.project.clone() }, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "project".into() }); })) }),
                branch: Some(ComposerChip { label: state.branch.clone(), title: "BranchPicker".into(), visual: ComposerChipVisual::Kind, loading: false, disabled: state.disabled, on_open: Some(EventHandler::new(move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "branch".into() }); })) }),
                is_git_repo: true, workspace_known: true, uncommitted: 3, ahead: 2,
                status: state.status.clone(), active_subagents: 2, active_tasks: 1, queued_count: 3,
            }
            ComposerMenus {
                opened, cursor: state.cursor,
                placement: if state.story == Story::PromptComposer { PromptPopupPlacement::Upward } else { PromptPopupPlacement::Downward },
                on_hover: move |cursor| { let _ = send(&GalleryCursorRequest { revision, cursor }); },
                on_dismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                agent: Some(AgentMenuData { options: state.examples.agents.clone(), selected_url: state.agent.clone(), on_select: EventHandler::new(move |value| { let _ = send(&GalleryAgentRequest { revision, value }); }) }),
                model: Some(ModelMenuData { models: examples.models.clone(), current_model_id: state.model.clone(), on_select: EventHandler::new(move |entry: vmux_api::session::ModelOptionEntry| { let _ = send(&GalleryModelRequest { revision, value: entry.id }); }) }),
                effort: Some(EffortMenuData { levels: vec!["low".into(), "medium".into(), "high".into()], selected: state.effort.clone(), on_select: EventHandler::new(move |value| { let _ = send(&GalleryEffortRequest { revision, value }); }) }),
                permission: Some(PermissionMenuData { modes: examples.modes.clone(), current_mode_id: state.permission.clone(), on_select: EventHandler::new(move |entry: vmux_api::protocol::AcpModeOption| { let _ = send(&GalleryPermissionRequest { revision, value: entry.id }); }) }),
                project: Some(ProjectMenuData { projects: state.examples.projects.clone(), loaded: true, on_pick: EventHandler::new(move |pick: vmux_ui::components::project_picker::ProjectPick| { let _ = send(&GalleryProjectRequest { revision, value: pick.project }); }), on_choose_another: EventHandler::new(move |_| { let _ = send(&GalleryClickRequest { revision, label: "ProjectPicker.choose_another".into() }); }) }),
                branch: Some(BranchMenuData { project: state.project.clone(), branches: state.examples.branches.clone(), loaded: true, on_pick: EventHandler::new(move |pick: vmux_ui::components::project_picker::ProjectPick| { let _ = send(&GalleryBranchRequest { revision, value: pick.branch }); }) }),
            }
            div { class: "mt-4 flex flex-wrap gap-1",
                for status in ["idle", "streaming", "awaiting", "installing", "errored", "in_use"] {
                    Button { variant: ButtonVariant::Ghost,
                        onclick: move |_| { let _ = send(&GalleryStatusRequest { revision, status: status.into() }); },
                        "{status}"
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn EditingPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    let mut editing = use_signal(|| false);
    let draft = use_signal(|| state.text.clone());
    let mut active = use_signal(|| None::<usize>);
    let mut inline_draft = use_signal(|| state.text.clone());
    let mut multiline_draft = use_signal(|| state.text.clone());
    let inline_value = state.text.clone();
    let multiline_value = state.text.clone();
    rsx! {
        fieldset { disabled: state.disabled, class: "mx-auto max-w-xl space-y-8",
            div { class: "space-y-3",
                p { class: "font-mono text-xs text-muted-foreground", "EditableText" }
                EditableText {
                    value: state.text.clone(), editing, draft,
                    display_class: "block min-h-8 cursor-text rounded-md bg-foreground/5 p-2",
                    input_class: "w-full rounded-md bg-background p-2 outline-none",
                    title: "EditableText", empty_display: translate("gallery-sample"),
                    allow_empty: true,
                    on_active_change: Some(EventHandler::new(move |started: bool| {
                        if started { active.set(Some(0)); }
                        else if active() == Some(0) { active.set(None); }
                    })),
                    on_commit: move |text| {
                        if active() == Some(0) { let _ = send(&GalleryTextRequest { revision, text }); }
                    },
                }
            }
            div { class: "space-y-3",
                p { class: "font-mono text-xs text-muted-foreground", "InlineEdit" }
                Button { variant: ButtonVariant::Secondary, onclick: move |_| {
                    editing.set(false);
                    inline_draft.set(inline_value.clone());
                    active.set(if active() == Some(1) { None } else { Some(1) });
                }, "editing: {active() == Some(1)}" }
                if active() == Some(1) {
                    InlineEdit { draft: inline_draft, class: "w-full rounded-md bg-background p-2", aria_label: "InlineEdit", allow_empty: true,
                        on_commit: move |text| { if active() == Some(1) { active.set(None); let _ = send(&GalleryTextRequest { revision, text }); } },
                        on_cancel: move |_| { if active() == Some(1) { active.set(None); let _ = send(&GalleryClickRequest { revision, label: "InlineEdit.cancel".into() }); } },
                    }
                }
            }
            div { class: "space-y-3",
                p { class: "font-mono text-xs text-muted-foreground", "InlineTextarea" }
                Button { variant: ButtonVariant::Secondary, onclick: move |_| {
                    editing.set(false);
                    multiline_draft.set(multiline_value.clone());
                    active.set(if active() == Some(2) { None } else { Some(2) });
                }, "editing: {active() == Some(2)}" }
                if active() == Some(2) {
                    InlineTextarea { draft: multiline_draft, class: "w-full rounded-md bg-background p-2", aria_label: "InlineTextarea", allow_empty: true,
                        on_commit: move |text| { if active() == Some(2) { active.set(None); let _ = send(&GalleryTextRequest { revision, text }); } },
                        on_cancel: move |_| { if active() == Some(2) { active.set(None); let _ = send(&GalleryClickRequest { revision, label: "InlineTextarea.cancel".into() }); } },
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn ManagementPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    let view = if state.checked {
        ManagerView::Grid
    } else {
        ManagerView::List
    };
    rsx! {
        fieldset { disabled: state.disabled, class: "space-y-6",
            ManagerTabs {
                active: state.choice.clone(),
                tabs: vec![
                    ManagerTab { id: "one".into(), label: translate("gallery-option-one"), href: "one".into() },
                    ManagerTab { id: "two".into(), label: translate("gallery-option-two"), href: "two".into() },
                ],
                onselect: move |choice| { let _ = send(&GalleryChoiceRequest { revision, choice }); },
            }
            ManagerHeader { title: "ManagerHeader", count: 2, search_value: state.text.clone(), search_placeholder: translate("gallery-search"),
                onsearch: move |event: FormEvent| { let _ = send(&GalleryTextRequest { revision, text: event.value() }); },
                actions: rsx! {
                    ManagerAddButton { label: "ManagerAddButton", disabled: state.disabled,
                        onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ManagerAddButton".into() }); },
                    }
                    ManagerIconButton { label: "ManagerIconButton", disabled: state.disabled,
                        onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ManagerIconButton".into() }); },
                        LineIconView { icon: LineIcon::Plus, class: "size-4" }
                    }
                },
            }
            div { class: "flex h-96 flex-col overflow-hidden rounded-lg bg-foreground/[0.025]",
                WorkspaceLayout {
                    resizable: true,
                    sidebar: rsx! {
                        ManagerSidebarSection { title: "ManagerSidebarSection",
                            for (choice, label) in [("one", "vmux"), ("two", "library")] {
                                ManagerSidebarItem { label, count: 1, selected: state.choice == choice, icon: LineIcon::Plus,
                                    onclick: move |_| { let _ = send(&GalleryChoiceRequest { revision, choice: choice.into() }); },
                                }
                            }
                        }
                    },
                    search: rsx! { ManagerSearchRow {
                        ManagerSearch { value: state.text.clone(), placeholder: translate("gallery-search"),
                            onsearch: move |text| { let _ = send(&GalleryTextRequest { revision, text }); },
                        }
                    } },
                    toolbar: rsx! {
                        ManagerToolbar {
                            ManagerControls {
                                ManagerViewToggle { view, list_label: translate("tools-view-list"), grid_label: translate("tools-view-grid"),
                                    onchange: move |view| { let _ = send(&GalleryCheckedRequest { revision, checked: view == ManagerView::Grid }); },
                                }
                                ManagerSearchAction { value: state.text.clone(), placeholder: translate("gallery-search"),
                                    onsearch: move |text| { let _ = send(&GalleryTextRequest { revision, text }); },
                                }
                            }
                        }
                    },
                    ManagerList {
                        ManagerSection { label: "ManagerSection", count: 2,
                            ManagerCollection { view,
                                for (index, row) in state.examples.projects.iter().enumerate() {
                                    {
                                        let choice = if index == 0 { "one" } else { "two" };
                                        rsx! {
                                            if view == ManagerView::Grid {
                                                ManagerCard { title: row.label.clone(), subtitle: row.display_path.clone(), selected: state.choice == choice,
                                                    icon: rsx! { ManagerThumbnail { fallback: row.label.clone() } },
                                                    meta: rsx! { ManagerBadge { "ManagerBadge" } },
                                                    actions: rsx! { ManagerButton { variant: ManagerButtonVariant::Secondary,
                                                        onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ManagerCard.action".into() }); },
                                                        {translate("gallery-sample")}
                                                    } },
                                                    onselect: move |_| { let _ = send(&GalleryChoiceRequest { revision, choice: choice.into() }); },
                                                }
                                            } else {
                                                ManagerRow { title: row.label.clone(), subtitle: row.display_path.clone(), selected: state.choice == choice,
                                                    icon: rsx! { ManagerThumbnail { fallback: row.label.clone() } },
                                                    meta: rsx! { ManagerBadge { "ManagerBadge" } },
                                                    actions: rsx! { ManagerButton { variant: ManagerButtonVariant::Secondary,
                                                        onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ManagerRow.action".into() }); },
                                                        {translate("gallery-sample")}
                                                    } },
                                                    onselect: move |_| { let _ = send(&GalleryChoiceRequest { revision, choice: choice.into() }); },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "flex flex-wrap gap-3",
                for tone in [ManagerTone::Neutral, ManagerTone::Primary, ManagerTone::Green, ManagerTone::Amber] {
                    ManagerBadge { tone, "ManagerBadge" }
                }
                for variant in [ManagerButtonVariant::Primary, ManagerButtonVariant::Secondary, ManagerButtonVariant::Danger, ManagerButtonVariant::Ghost] {
                    ManagerButton { variant, disabled: state.disabled,
                        onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ManagerButton".into() }); },
                        "ManagerButton"
                    }
                }
            }
            ManagerSelect {
                value: Some(state.choice.clone()), placeholder: "ManagerSelect",
                items: vec![
                    ManagerSelectItem { value: "one".into(), label: translate("gallery-option-one") },
                    ManagerSelectItem { value: "two".into(), label: translate("gallery-option-two") },
                ],
                onselect: move |choice| { let _ = send(&GalleryChoiceRequest { revision, choice }); },
            }
            ManagerExpandableSearch { open: state.menu == "manager-search", active: !state.text.is_empty(), label: "ManagerExpandableSearch",
                ontoggle: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "manager-search".into() }); },
                ondismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                ManagerSearch { value: state.text.clone(), placeholder: translate("gallery-search"),
                    onsearch: move |text| { let _ = send(&GalleryTextRequest { revision, text }); },
                }
            }
            Button { variant: ButtonVariant::Secondary,
                onclick: move |_| { let _ = send(&GalleryDialogRequest { revision, open: true }); },
                "ManagerDetails"
            }
            if state.dialog_open {
                ManagerDetails { title: "ManagerDetails", subtitle: translate("gallery-sample"),
                    icon: rsx! { ManagerThumbnail { fallback: "vmux" } },
                    actions: rsx! {},
                    onclose: move |_| { let _ = send(&GalleryDialogRequest { revision, open: false }); },
                    {translate("gallery-description")}
                }
            }
            ManagerSpinner { detail: "ManagerSpinner" }
            ManagerEmpty { title: "ManagerEmpty", detail: translate("gallery-sample") }
            ManagerSkeleton {}
        }
    }
}

#[component]
pub(crate) fn MenuPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    if state.story == Story::PromptMediaOptions {
        return rsx! {
            div { class: "mx-auto max-w-md",
                PromptMediaOptions { items: state.examples.media, selected: state.cursor, loading: false,
                    on_hover: move |cursor| { let _ = send(&GalleryCursorRequest { revision, cursor }); },
                    on_select: move |index| { let _ = send(&GalleryClickRequest { revision, label: format!("PromptMediaOptions:{index}") }); },
                }
            }
        };
    }
    let menu = match state.story {
        Story::AgentMenu => "agent",
        Story::ModelMenu => "model",
        Story::EffortMenu => "effort",
        Story::PermissionMenu => "permission",
        Story::ProjectPicker => "project",
        Story::McpMenu => "mcp",
        _ => "",
    };
    rsx! {
        fieldset { disabled: state.disabled, class: "mx-auto w-full max-w-lg space-y-6",
            div { class: "relative min-h-80",
                Button { variant: ButtonVariant::Secondary,
                    onclick: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: menu.into() }); },
                    "{state.story.name()}"
                }
                if state.menu == menu {
                    MenuPopup { state: state.clone() }
                }
            }
            if state.story == Story::ProjectPicker {
                div { class: "relative min-h-80",
                    ProjectSelectTrigger { label: state.project.clone(), title: "ProjectSelectTrigger", open: state.menu == "project-select",
                        onclick: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "project-select".into() }); },
                    }
                    if state.menu == "project-select" {
                        ProjectSelectPopup {
                            projects: state.examples.projects.clone(), selected_path: state.project.clone(),
                            on_pick: move |row: vmux_api::space::ProjectRow| { let _ = send(&GalleryProjectRequest { revision, value: row.path }); },
                            on_add: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ProjectSelectPopup.add".into() }); },
                        }
                    }
                }
                div { class: "relative min-h-80",
                    Button { onclick: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "branch".into() }); }, "BranchPicker" }
                    if state.menu == "branch" {
                        BranchPicker { project: state.project.clone(), branches: state.examples.branches.clone(), loaded: true,
                            on_hover: move |cursor| { let _ = send(&GalleryCursorRequest { revision, cursor }); },
                            on_pick: move |pick: vmux_ui::components::project_picker::ProjectPick| { let _ = send(&GalleryBranchRequest { revision, value: pick.branch }); },
                            on_dismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MenuPopup(state: GalleryUiState) -> Element {
    let revision = state.revision;
    let examples = crate::Examples::localized(&state.examples);
    let hover = EventHandler::new(move |cursor| {
        let _ = send(&GalleryCursorRequest { revision, cursor });
    });
    let dismiss = EventHandler::new(move |_| {
        let _ = send(&GalleryMenuRequest {
            revision,
            menu: String::new(),
        });
    });
    match state.story {
        Story::AgentMenu => rsx! { AgentMenu {
            options: state.examples.agents, selected_url: state.agent, cursor: state.cursor,
            on_hover: hover, on_dismiss: dismiss,
            on_select: move |value| { let _ = send(&GalleryAgentRequest { revision, value }); },
        } },
        Story::ModelMenu => rsx! { ModelMenu {
            models: examples.models, current_model_id: state.model, selected: state.cursor, on_hover: hover, on_dismiss: dismiss,
            on_select: move |(_, entry): (usize, vmux_api::session::ModelOptionEntry)| { let _ = send(&GalleryModelRequest { revision, value: entry.id }); },
        } },
        Story::EffortMenu => rsx! { EffortMenu {
            levels: vec!["low".into(), "medium".into(), "high".into()], selected: state.effort, cursor: state.cursor, on_hover: hover, on_dismiss: dismiss,
            on_select: move |value| { let _ = send(&GalleryEffortRequest { revision, value }); },
        } },
        Story::PermissionMenu => rsx! { PermissionMenu {
            modes: examples.modes, current_mode_id: state.permission, selected: state.cursor, on_hover: hover, on_dismiss: dismiss,
            on_select: move |entry: vmux_api::protocol::AcpModeOption| { let _ = send(&GalleryPermissionRequest { revision, value: entry.id }); },
        } },
        Story::ProjectPicker => rsx! { ProjectPicker {
            projects: state.examples.projects, loaded: true, cursor: state.cursor, on_hover: hover, on_dismiss: dismiss,
            on_pick: move |pick: vmux_ui::components::project_picker::ProjectPick| { let _ = send(&GalleryProjectRequest { revision, value: pick.project }); },
            on_choose_another: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ProjectPicker.choose_another".into() }); },
        } },
        Story::McpMenu => rsx! { McpMenu {
            entries: state.examples.mcp.servers.clone(), state: state.examples.mcp, selected: state.cursor, on_hover: hover, on_dismiss: dismiss,
            on_select: move |index| { let _ = send(&GalleryClickRequest { revision, label: format!("McpMenu:{index}") }); },
        } },
        _ => rsx! {},
    }
}

#[component]
pub(crate) fn NavigationPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    let dropdown_open = state.menu == "dropdown";
    let address = use_ui_state::<PageAddressUiState>().state;
    let address = address().address.unwrap_or_else(|| state.address.clone());
    rsx! {
        fieldset { disabled: state.disabled, class: "mx-auto w-full max-w-3xl space-y-6",
            match state.story {
                Story::Address | Story::PageAddress => rsx! {
                    AddressSurface { url: address.url.clone(),
                        onactivate: move |_| { let _ = send(&GalleryClickRequest { revision, label: "AddressSurface".into() }); },
                        AddressTrail { state: address.clone() }
                        AddressControls { state: address }
                    }
                    PageToolbar { controls: vec![
                        ToolbarControl::badge("ToolbarControl"),
                        ToolbarControl::button("Button", move |_| { let _ = send(&GalleryClickRequest { revision, label: "PageToolbar.button".into() }); }),
                        ToolbarControl::search(state.text.clone(), translate("gallery-search"), move |text| { let _ = send(&GalleryTextRequest { revision, text }); }),
                    ] }
                },
                Story::Breadcrumbs => rsx! {
                    Breadcrumbs {
                        BreadcrumbItem { label: "vmux", onclick: move |_| { let _ = send(&GalleryClickRequest { revision, label: "BreadcrumbItem".into() }); } }
                        BreadcrumbSeparator {}
                        BreadcrumbDropdown { label: "BreadcrumbDropdown", open: state.menu == "breadcrumb",
                            on_open_change: move |open| { let _ = send(&GalleryMenuRequest { revision, menu: if open { "breadcrumb".into() } else { String::new() } }); },
                            DropdownOption { label: translate("gallery-option-one"), selected: state.choice == "one",
                                onclick: move |_| { let _ = send(&GalleryChoiceRequest { revision, choice: "one".into() }); },
                            }
                        }
                        BreadcrumbSeparator {}
                        BreadcrumbSelector {
                            label: state.choice.clone(), value: state.choice.clone(),
                            options: vec![
                                BreadcrumbOption { label: translate("gallery-option-one"), value: "one".into(), identity_seed: "vmux".into(), icon_url: String::new(), icon: Some(vmux_api::BuiltinIcon::Layers) },
                                BreadcrumbOption { label: translate("gallery-option-two"), value: "two".into(), identity_seed: "library".into(), icon_url: String::new(), icon: None },
                            ],
                            onselect: move |choice| { let _ = send(&GalleryChoiceRequest { revision, choice }); },
                        }
                    }
                    BreadcrumbOptionIcon { class: "size-6", option: BreadcrumbOption { label: "vmux".into(), value: "one".into(), identity_seed: "vmux".into(), icon_url: String::new(), icon: Some(vmux_api::BuiltinIcon::Layers) } }
                },
                Story::Dropdown => rsx! {
                    div { class: "grid items-start gap-6 @lg/gallery:grid-cols-2",
                    div { class: "relative min-w-0 space-y-3",
                        p { class: "font-mono text-[11px] text-muted-foreground", "DropdownTrigger / DropdownPopup" }
                        Dropdown { open: state.menu == "dropdown", on_dismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                            DropdownTrigger { label: state.choice.clone(), class: "inline-flex min-w-36 justify-between rounded-md bg-foreground/5 px-3 py-2", open: dropdown_open, disabled: state.disabled,
                                onclick: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: if dropdown_open { String::new() } else { "dropdown".into() } }); },
                                trailing: rsx! { DropdownToggleIndicator { open: state.menu == "dropdown" } },
                            }
                            if state.menu == "dropdown" {
                                DropdownPopup {
                                    for choice in ["one", "two", "three"] {
                                        DropdownOption { label: translate(&format!("gallery-option-{choice}")), selected: state.choice == choice,
                                            onclick: move |_| { let _ = send(&GalleryChoiceRequest { revision, choice: choice.into() }); },
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "relative min-w-0 space-y-3",
                        p { class: "font-mono text-[11px] text-muted-foreground", "DropdownSelector" }
                        DropdownSelector { label: state.choice.clone(), trigger_class: "inline-flex min-w-36 justify-between rounded-md bg-foreground/5 px-3 py-2", open: state.menu == "dropdown-selector", disabled: state.disabled,
                            on_open_change: move |open| { let _ = send(&GalleryMenuRequest { revision, menu: if open { "dropdown-selector".into() } else { String::new() } }); },
                            on_dismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                            DropdownOption { label: translate("gallery-option-one"), selected: state.choice == "one",
                                onclick: move |_| { let _ = send(&GalleryChoiceRequest { revision, choice: "one".into() }); },
                            }
                        }
                    }
                    }
                },
                Story::SegmentedControl => rsx! {
                    SegmentedControl { active: state.choice.clone(),
                        items: vec![
                            SegmentedControlItem { id: "one".into(), label: translate("gallery-option-one") },
                            SegmentedControlItem { id: "two".into(), label: translate("gallery-option-two") },
                            SegmentedControlItem { id: "three".into(), label: translate("gallery-option-three") },
                        ],
                        onselect: move |choice| { let _ = send(&GalleryChoiceRequest { revision, choice }); },
                    }
                },
                Story::Sheet => rsx! {
                    div { class: "flex gap-2",
                        SheetHeaderAction { label: "SheetHeaderAction", active: state.checked,
                            onclick: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); }, "+"
                        }
                        SheetHeaderChevron { label: "SheetHeaderChevron", expanded: state.checked,
                            onclick: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                        }
                    }
                    SheetToggleRow { label: "SheetToggleRow", expanded: state.checked,
                        onclick: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                    }
                    SheetSelectorRow { label: "SheetSelectorRow", open: state.checked,
                        onclick: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                    }
                },
                Story::FileTree => rsx! {
                    FileTree {
                        FileTreeHeader { "FileTreeHeader" }
                        FileTreeScroller { class: "h-56",
                            FileTreeRowGroup {
                                FileTreeRow { path: "/example/src", label: "src", is_dir: true, expanded: state.checked, show_dir_icon: true,
                                    on_activate: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                                }
                                FileTreeChildren { expanded: state.checked,
                                    for (choice, path) in [("one", "example.rs"), ("two", "main.rs")] {
                                        FileTreeRow { path, label: path, is_dir: false, depth: 1, active: state.choice == choice,
                                            on_activate: move |_| { let _ = send(&GalleryChoiceRequest { revision, choice: choice.into() }); },
                                        }
                                    }
                                }
                            }
                        }
                    }
                    FileTreeSkeleton { rows: 4 }
                },
                _ => rsx! {},
            }
        }
    }
}

#[component]
pub(crate) fn SurfacePreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
    rsx! {
        fieldset { disabled: state.disabled, class: "mx-auto max-w-3xl space-y-6",
            match state.story {
                Story::AlertDialog => rsx! {
                    Button { onclick: move |_| { let _ = send(&GalleryDialogRequest { revision, open: true }); }, {translate("gallery-open-dialog")} }
                    AlertDialogRoot { open: Some(state.dialog_open),
                        on_open_change: move |open| { let _ = send(&GalleryDialogRequest { revision, open }); },
                        AlertDialogContent {
                            AlertDialogTitle { {translate("gallery-sample")} }
                            AlertDialogDescription { {translate("gallery-description")} }
                            AlertDialogActions {
                                AlertDialogCancel { on_click: move |_| { let _ = send(&GalleryDialogRequest { revision, open: false }); }, {translate("gallery-close")} }
                                AlertDialogAction { on_click: move |_| { let _ = send(&GalleryClickRequest { revision, label: "AlertDialogAction".into() }); }, {translate("gallery-sample")} }
                            }
                        }
                    }
                },
                Story::ContextMenu => rsx! {
                    ContextMenu { disabled: state.disabled, open: Some(state.dialog_open),
                        on_open_change: move |open| { let _ = send(&GalleryDialogRequest { revision, open }); },
                        ContextMenuTrigger { class: "flex h-40 items-center justify-center rounded-lg bg-foreground/5", "ContextMenuTrigger" }
                        ContextMenuContent {
                            for (index, choice) in ["one", "two", "three"].into_iter().enumerate() {
                                ContextMenuItem { index, value: choice.to_string(),
                                    on_select: move |_| { let _ = send(&GalleryChoiceRequest { revision, choice: choice.into() }); },
                                    {translate(&format!("gallery-option-{choice}"))}
                                }
                            }
                        }
                    }
                },
                Story::Badge => rsx! {
                    div { class: "flex flex-wrap items-center gap-3",
                        Badge { "Badge" }
                        Badge { class: "rounded-full bg-primary/10 px-2 py-1 text-primary", {translate("gallery-sample")} }
                        Badge { class: "rounded-full bg-success/10 px-2 py-1 text-success", {translate("gallery-sample")} }
                    }
                },
                Story::ShellCode => rsx! {
                    ShellCode { source: "vmux build ui-gallery\ngit status --short" }
                    ShellCode { source: "vmux://ui/composer", inline: true }
                },
                Story::ErrorState => rsx! {
                    ErrorState { message: translate("gallery-sample"),
                        on_retry: move |_| { let _ = send(&GalleryClickRequest { revision, label: "ErrorState.retry".into() }); },
                    }
                },
                Story::Filter => rsx! {
                    FilterTrigger { active: state.checked, label: "FilterTrigger",
                        onclick: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                    }
                    FilterPill { label: "FilterPill", dot: "bg-success", clear_label: translate("gallery-close"),
                        onclear: move |_| { let _ = send(&GalleryClickRequest { revision, label: "FilterPill.clear".into() }); },
                    }
                    FilterMenu { title: "FilterMenu",
                        for choice in ["one", "two", "three"] {
                            FilterOption { label: translate(&format!("gallery-option-{choice}")), detail: "FilterOption", next: choice == "three",
                                onclick: move |_| { let _ = send(&GalleryChoiceRequest { revision, choice: choice.into() }); },
                            }
                        }
                    }
                },
                Story::Icon => rsx! {
                    div { class: "flex flex-wrap items-center gap-6",
                        Icon { class: "size-6", path { d: "M12 5v14M5 12h14" } }
                        LineIconView { icon: LineIcon::Plus, class: "size-6" }
                        LineIconView { icon: LineIcon::Copy, class: "size-6" }
                        LineIconView { icon: LineIcon::Upload, class: "size-6" }
                        BuiltinIconView { icon: vmux_api::BuiltinIcon::Layers, class: "size-6" }
                        GitIconView { class: "size-6" }
                        GitSymbolView { symbol: GitSymbol::Branch, class: "size-6" }
                        PageIconView { icon: Default::default(), url: "vmux://ui/", img_class: "size-6", icon_class: "size-6" }
                    }
                },
                Story::Motion => rsx! {
                    Button { variant: ButtonVariant::Secondary,
                        onclick: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                        "expanded: {state.checked}"
                    }
                    ContentTransition { class: "rounded-lg bg-foreground/5 p-4", "ContentTransition" }
                    Collapsible { expanded: state.checked, content_class: "rounded-lg bg-foreground/5 p-4", "Collapsible" }
                    Collapse { leaving: !state.checked, "Collapse" }
                },
                Story::Notice => rsx! {
                    for kind in [NoticeKind::Info, NoticeKind::Warning, NoticeKind::Error] {
                        Notice { kind, message: translate("gallery-sample"),
                            on_dismiss: move |_| { let _ = send(&GalleryClickRequest { revision, label: "Notice.dismiss".into() }); },
                        }
                    }
                    Button { variant: ButtonVariant::Secondary,
                        onclick: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                        "floating: {state.checked}"
                    }
                    if state.checked {
                        Notice { floating: true, message: translate("gallery-sample"),
                            on_dismiss: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: false }); },
                        }
                    }
                },
                Story::Page => rsx! {
                    div { class: "flex h-80 flex-col rounded-lg bg-foreground/5",
                        PageToolbarRow {
                            PageSidebarToggle { open: state.checked, label: "PageSidebarToggle",
                                ontoggle: move |_| { let _ = send(&GalleryCheckedRequest { revision, checked: !state.checked }); },
                            }
                            "PageToolbarRow"
                        }
                        PageHeader { "PageHeader" }
                        PageContent { "PageContent" }
                    }
                },
                Story::StartHero => rsx! {
                    div { class: "relative isolate overflow-hidden rounded-lg p-8",
                        StartBackdrop {}
                        StartHero { {translate("gallery-sample")} }
                    }
                },
                Story::Tooltip => rsx! {
                    Tooltip { label: translate("gallery-sample"),
                        Button { variant: ButtonVariant::Secondary, "Tooltip" }
                    }
                },
                _ => rsx! {},
            }
        }
    }
}
