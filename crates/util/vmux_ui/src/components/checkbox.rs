use dioxus::prelude::*;
use dioxus_primitives::checkbox::{self, CheckboxState};
use dioxus_primitives::icon;

const CHECKBOX: &str = "size-4 box-border cursor-pointer rounded border-0 bg-card p-0 text-muted-foreground shadow-[inset_0_0_0_1px_var(--primary)] data-[state=checked]:bg-primary data-[state=checked]:text-background data-[state=checked]:shadow-none data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50 focus-visible:shadow-[0_0_0_2px_var(--ring)]";

const CHECKBOX_INDICATOR: &str = "flex items-center justify-center";

#[component]
pub fn Checkbox(
    checked: bool,
    #[props(default)] disabled: bool,
    #[props(default)] on_checked_change: Option<EventHandler<bool>>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
) -> Element {
    let state = match checked {
        true => CheckboxState::Checked,
        false => CheckboxState::Unchecked,
    };
    rsx! {
        checkbox::Checkbox {
            class: CHECKBOX,
            checked: Some(state),
            disabled,
            on_checked_change: move |state| {
                if let Some(handler) = on_checked_change {
                    handler.call(bool::from(state));
                }
            },
            attributes,
            checkbox::CheckboxIndicator {
                class: CHECKBOX_INDICATOR,
                icon::Icon {
                    width: "1rem",
                    height: "1rem",
                    path { d: "M5 13l4 4L19 7" }
                }
            }
        }
    }
}
