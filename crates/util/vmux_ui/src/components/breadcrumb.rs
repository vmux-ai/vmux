use crate::cn::cn;
use crate::components::avatar::IdentityOrb;
use crate::components::dropdown::{
    DropdownAlign, DropdownOpenMode, DropdownOption, DropdownSelector,
};
use crate::i18n::translate;
use crate::icon::BuiltinIconView;
use dioxus::prelude::*;
use vmux_api::BuiltinIcon;

#[derive(Clone, PartialEq, Eq)]
pub struct BreadcrumbOption {
    pub label: String,
    pub value: String,
    pub identity_seed: String,
    pub icon_url: String,
    pub icon: Option<BuiltinIcon>,
}

#[component]
pub fn Breadcrumbs(
    children: Element,
    #[props(default)] class: String,
    #[props(default)] label: String,
) -> Element {
    let class = cn(["flex min-w-0 items-center gap-1", class.as_str()]);
    let label = if label.is_empty() {
        translate("editor-breadcrumb")
    } else {
        label
    };
    rsx! {
        nav { class, aria_label: label, {children} }
    }
}

#[component]
pub fn BreadcrumbItem(
    label: String,
    #[props(default)] title: String,
    #[props(default)] active: bool,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(default = rsx! {})] leading: Element,
) -> Element {
    let title = if title.is_empty() {
        label.clone()
    } else {
        title
    };
    let class = if active {
        "flex h-7 min-w-0 items-center gap-1.5 rounded-md px-1.5 text-xs font-medium text-foreground"
    } else {
        "flex h-7 min-w-0 items-center gap-1.5 rounded-md px-1.5 text-xs text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
    };
    match onclick {
        Some(handler) => rsx! {
            button {
                "data-vmux-address-item": "",
                r#type: "button",
                class,
                title,
                onclick: move |event| handler.call(event),
                {leading}
                span { class: "truncate", "{label}" }
            }
        },
        None => rsx! {
            span { "data-vmux-address-item": "", class, title, aria_current: if active { "page" } else { "false" },
                {leading}
                span { class: "truncate", "{label}" }
            }
        },
    }
}

#[component]
pub fn BreadcrumbSeparator() -> Element {
    rsx! {
        span { class: "shrink-0 text-xs text-muted-foreground/45", "/" }
    }
}

#[component]
pub fn BreadcrumbDropdown(
    label: String,
    open: bool,
    on_open_change: EventHandler<bool>,
    children: Element,
    #[props(default)] title: String,
    #[props(default)] class: String,
    #[props(default)] active: bool,
    #[props(default = rsx! {})] leading: Element,
    #[props(default)] popup_class: String,
    #[props(default = true)] truncate: bool,
) -> Element {
    let root_class = cn(["w-fit min-w-0 max-w-full", class.as_str()]);
    let popup_class = cn([
        "max-h-80 min-w-48 overflow-y-auto p-1",
        popup_class.as_str(),
    ]);
    rsx! {
        DropdownSelector {
            label,
            truncate,
            title,
            open,
            class: root_class,
            active,
            open_mode: DropdownOpenMode::Hover,
            leading,
            align: DropdownAlign::Start,
            match_width: true,
            popup_class,
            on_open_change: move |next| on_open_change.call(next),
            on_dismiss: move |_| on_open_change.call(false),
            {children}
        }
    }
}

#[component]
pub fn BreadcrumbSelector(
    label: String,
    options: Vec<BreadcrumbOption>,
    onselect: EventHandler<String>,
    #[props(default)] value: String,
    #[props(default)] class: String,
    #[props(default)] active: bool,
    #[props(default)] on_open_change: Option<EventHandler<bool>>,
    #[props(default = rsx! {})] leading: Element,
) -> Element {
    let mut open = use_signal(|| false);
    let selected_value = value.clone();
    let selected_label = label.clone();
    rsx! {
        BreadcrumbDropdown {
            label,
            open: open(),
            class,
            active,
            leading,
            on_open_change: move |next| {
                open.set(next);
                if let Some(handler) = on_open_change {
                    handler.call(next);
                }
            },
            for option in options {
                {
                    let selected = if selected_value.is_empty() {
                        option.label == selected_label
                    } else {
                        option.value == selected_value
                    };
                    let option_value = option.value.clone();
                    rsx! {
                        DropdownOption {
                            key: "{option.value}",
                            label: option.label.clone(),
                            selected,
                            onclick: move |_| {
                                open.set(false);
                                if let Some(handler) = on_open_change {
                                    handler.call(false);
                                }
                                onselect.call(option_value.clone());
                            },
                            leading: rsx! {
                                BreadcrumbOptionIcon {
                                    option: option.clone(),
                                    class: "size-4 shrink-0".to_string(),
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn BreadcrumbOptionIcon(option: BreadcrumbOption, class: String) -> Element {
    if !option.identity_seed.is_empty() {
        return rsx! {
            IdentityOrb {
                seed: option.identity_seed,
                alt: option.label,
                class,
            }
        };
    }
    if !option.icon_url.is_empty() {
        return rsx! {
            img {
                src: option.icon_url,
                alt: option.label,
                class: cn([class.as_str(), "object-contain"]),
            }
        };
    }
    let Some(icon) = option.icon else {
        return rsx! {};
    };
    rsx! {
        BuiltinIconView {
            icon,
            class: cn([class.as_str(), "text-muted-foreground"]),
        }
    }
}
