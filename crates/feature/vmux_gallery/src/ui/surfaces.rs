use crate::{
    GalleryCheckedRequest, GalleryChoiceRequest, GalleryClickRequest, GalleryDialogRequest,
    GalleryUiState, Story,
};
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_ui::components::alert_dialog::*;
use vmux_ui::components::badge::Badge;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::code::ShellCode;
use vmux_ui::components::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger,
};
use vmux_ui::components::error_state::ErrorState;
use vmux_ui::components::filter::{FilterMenu, FilterOption, FilterPill, FilterTrigger};
use vmux_ui::components::icon::Icon;
use vmux_ui::components::motion::{Collapse, Collapsible, ContentTransition};
use vmux_ui::components::notice::{Notice, NoticeKind};
use vmux_ui::components::page::{PageContent, PageHeader, PageSidebarToggle, PageToolbarRow};
use vmux_ui::components::start_hero::{StartBackdrop, StartHero};
use vmux_ui::components::tooltip::Tooltip;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{
    BuiltinIconView, GitIconView, GitSymbol, GitSymbolView, LineIcon, LineIconView, PageIconView,
};

#[component]
pub(super) fn SurfacePreview(state: GalleryUiState) -> Element {
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
