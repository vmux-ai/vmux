use dioxus::prelude::*;

use crate::cn::cn;
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::i18n::translate;
use crate::icon::{LineIcon, LineIconView};

#[derive(Clone, PartialEq, Props)]
pub struct ErrorStateProps {
    pub message: String,
    #[props(default)]
    pub class: String,
    #[props(default)]
    pub on_retry: Option<EventHandler<MouseEvent>>,
}

#[component]
pub fn ErrorState(props: ErrorStateProps) -> Element {
    let class = cn([
        "flex min-h-32 flex-col items-center justify-center gap-2 px-5 py-6 text-center",
        props.class.as_str(),
    ]);
    rsx! {
        div { class,
            LineIconView { icon: LineIcon::AlertCircle, class: "size-5 text-muted-foreground" }
            div { class: "text-xs font-medium text-foreground", {translate("common-error")} }
            p { class: "m-0 max-w-md text-[11px] leading-5 text-muted-foreground", "{props.message}" }
            if let Some(on_retry) = props.on_retry {
                Button {
                    variant: ButtonVariant::Secondary,
                    size: ButtonSize::Xs,
                    class: "mt-1 gap-1.5",
                    onclick: move |event| on_retry.call(event),
                    LineIconView { icon: LineIcon::RefreshCw, class: "size-3" }
                    {translate("common-retry")}
                }
            }
        }
    }
}
