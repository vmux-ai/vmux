use crate::{
    GalleryCheckedRequest, GalleryChoiceRequest, GalleryClickRequest, GalleryMenuRequest,
    GalleryTextRequest, GalleryUiState, Story,
};
use dioxus::prelude::component;
use dioxus::prelude::*;
use vmux_api::address::PageAddressUiState;
use vmux_ui::components::address::{AddressControls, AddressTrail};
use vmux_ui::components::breadcrumb::{
    BreadcrumbDropdown, BreadcrumbItem, BreadcrumbOption, BreadcrumbOptionIcon, BreadcrumbSelector,
    BreadcrumbSeparator, Breadcrumbs,
};
use vmux_ui::components::dropdown::{
    Dropdown, DropdownOption, DropdownPopup, DropdownSelector, DropdownToggleIndicator,
    DropdownTrigger,
};
use vmux_ui::components::page_address::{AddressSurface, PageToolbar, ToolbarControl};
use vmux_ui::components::segmented_control::{SegmentedControl, SegmentedControlItem};
use vmux_ui::components::sheet::{
    SheetHeaderAction, SheetHeaderChevron, SheetSelectorRow, SheetToggleRow,
};
use vmux_ui::components::tree_row::{
    FileTree, FileTreeChildren, FileTreeHeader, FileTreeRow, FileTreeRowGroup, FileTreeScroller,
    FileTreeSkeleton,
};
use vmux_ui::hooks::{send, use_ui_state};
use vmux_ui::i18n::translate;

#[component]
pub(super) fn NavigationPreview(state: GalleryUiState) -> Element {
    let revision = state.revision;
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
                    div { class: "relative min-h-80",
                        Dropdown { open: state.menu == "dropdown", on_dismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                            DropdownTrigger { label: "DropdownTrigger", open: state.menu == "dropdown", disabled: state.disabled,
                                onclick: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: "dropdown".into() }); },
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
                    div { class: "relative min-h-80",
                        DropdownSelector { label: "DropdownSelector", open: state.menu == "dropdown-selector", disabled: state.disabled,
                            on_open_change: move |open| { let _ = send(&GalleryMenuRequest { revision, menu: if open { "dropdown-selector".into() } else { String::new() } }); },
                            on_dismiss: move |_| { let _ = send(&GalleryMenuRequest { revision, menu: String::new() }); },
                            DropdownOption { label: translate("gallery-option-one"), selected: state.choice == "one",
                                onclick: move |_| { let _ = send(&GalleryChoiceRequest { revision, choice: "one".into() }); },
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
