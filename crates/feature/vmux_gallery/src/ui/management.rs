use crate::{
    GalleryCheckedRequest, GalleryChoiceRequest, GalleryClickRequest, GalleryDialogRequest,
    GalleryMenuRequest, GalleryTextRequest, GalleryUiState,
};
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_ui::components::button::{Button, ButtonVariant};
use vmux_ui::components::manager::*;
use vmux_ui::hooks::send;
use vmux_ui::i18n::translate;
use vmux_ui::icon::{LineIcon, LineIconView};

#[component]
pub(super) fn ManagementPreview(state: GalleryUiState) -> Element {
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
                                ManagerViewToggle { view, list_label: "List", grid_label: "Grid",
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
