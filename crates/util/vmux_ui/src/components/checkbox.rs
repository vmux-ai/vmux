use dioxus::prelude::*;
use dioxus_primitives::checkbox::{self, CheckboxState};
use dioxus_primitives::icon;

const CHECKBOX_PRIMARY: &str = "size-4 box-border cursor-pointer rounded border-0 bg-card p-0 text-muted-foreground shadow-[inset_0_0_0_1px_var(--primary)] data-[state=checked]:bg-primary data-[state=checked]:text-background data-[state=checked]:shadow-none data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50 focus-visible:shadow-[0_0_0_2px_var(--ring)]";
const CHECKBOX_NEUTRAL: &str = "size-4 box-border cursor-pointer rounded border-0 bg-card p-0 text-muted-foreground ring-1 ring-inset ring-foreground/25 data-[state=checked]:bg-foreground/75 data-[state=checked]:text-background data-[state=checked]:ring-transparent data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50 focus-visible:ring-2 focus-visible:ring-ring";

const CHECKBOX_INDICATOR: &str = "flex items-center justify-center";

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum CheckboxTone {
    #[default]
    Primary,
    Neutral,
}

impl CheckboxTone {
    fn classes(self) -> &'static str {
        match self {
            Self::Primary => CHECKBOX_PRIMARY,
            Self::Neutral => CHECKBOX_NEUTRAL,
        }
    }
}

#[component]
pub fn Checkbox(
    checked: bool,
    #[props(default)] tone: CheckboxTone,
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
            class: tone.classes(),
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
