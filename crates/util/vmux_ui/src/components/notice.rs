use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::i18n::translate;
use crate::icon::{LineIcon, LineIconView};
use dioxus::prelude::*;

#[component]
pub fn Notice(
    message: String,
    #[props(default)] kind: NoticeKind,
    #[props(default)] floating: bool,
    #[props(default)] attached: bool,
    #[props(default = rsx! {})] actions: Element,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
) -> Element {
    let position = if floating {
        "fixed bottom-4 right-4 z-50 max-w-sm"
    } else {
        "w-full"
    };
    let surface = if attached {
        "flex items-center gap-3 px-3 py-2 text-sm [&_button]:text-inherit"
    } else {
        "flex items-start gap-3 rounded-xl bg-card px-4 py-3 text-sm"
    };
    rsx! {
        div {
            class: "{surface} {position}",
            role: if kind == NoticeKind::Error { "alert" } else { "status" },
            "data-slot": if attached { "composer-notice" } else { "notice" },
            if !attached {
            svg {
                class: "mt-0.5 size-4 shrink-0 {kind.color()}",
                view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
                stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: kind.path() }
            }
            }
            div { class: "min-w-0 flex-1 leading-relaxed", "{message}" }
            div { class: "flex shrink-0 items-center gap-1", {actions}
                if let Some(dismiss) = on_dismiss {
                    Button {
                        variant: ButtonVariant::Ghost, size: ButtonSize::Icon,
                        aria_label: translate("common-close"),
                        onclick: move |_| dismiss.call(()),
                        LineIconView { icon: LineIcon::X, class: "size-4".to_string() }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum NoticeKind {
    #[default]
    Info,
    Warning,
    Error,
}

impl NoticeKind {
    fn color(self) -> &'static str {
        match self {
            Self::Info => "text-muted-foreground",
            Self::Warning => "text-ansi-3",
            Self::Error => "text-ansi-1",
        }
    }

    fn path(self) -> &'static str {
        match self {
            Self::Info => "M12 8h.01M12 12v4M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z",
            Self::Warning => {
                "M12 9v4M12 17h.01M10.3 3.8 2 18a2 2 0 0 0 1.7 3h16.6a2 2 0 0 0 1.7-3L13.7 3.8a2 2 0 0 0-3.4 0Z"
            }
            Self::Error => "M12 8v5M12 16h.01M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0Z",
        }
    }
}
