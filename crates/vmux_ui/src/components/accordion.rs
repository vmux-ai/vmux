use dioxus::prelude::*;
use dioxus_primitives::accordion::{
    self, AccordionContentProps, AccordionItemProps, AccordionProps, AccordionTriggerProps,
};
use dioxus_primitives::icon;

#[component]
pub fn Accordion(props: AccordionProps) -> Element {
    rsx! {
        Fragment {
            style { "{ACCORDION_KEYFRAMES}" }
            accordion::Accordion {
                class: "[contain:inline-size]",
                width: "15rem",
                id: props.id,
                allow_multiple_open: props.allow_multiple_open,
                disabled: props.disabled,
                collapsible: props.collapsible,
                horizontal: props.horizontal,
                attributes: props.attributes,
                {props.children}
            }
        }
    }
}

#[component]
pub fn AccordionItem(props: AccordionItemProps) -> Element {
    rsx! {
        accordion::AccordionItem {
            class: "group mt-px box-border overflow-hidden border-b border-border first:mt-0 last:border-b-0",
            disabled: props.disabled,
            default_open: props.default_open,
            on_change: props.on_change,
            on_trigger_click: props.on_trigger_click,
            index: props.index,
            attributes: props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn AccordionTrigger(props: AccordionTriggerProps) -> Element {
    rsx! {
        accordion::AccordionTrigger {
            class: "flex w-full cursor-pointer flex-row items-center justify-between border-0 bg-transparent py-4 text-left text-muted-foreground outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
            id: props.id,
            attributes: props.attributes,
            {props.children}
            icon::Icon {
                class: "size-5 shrink-0 text-muted-foreground transition-transform duration-300 ease-out group-data-[open=true]:rotate-180",
                stroke: "currentColor",
                polyline { points: "6 9 12 15 18 9" }
            }
        }
    }
}

#[component]
pub fn AccordionContent(props: AccordionContentProps) -> Element {
    rsx! {
        accordion::AccordionContent {
            class: "grid data-[open=true]:animate-[accordion-open_300ms_cubic-bezier(0.87,0,0.13,1)_forwards] data-[open=false]:animate-[accordion-close_300ms_cubic-bezier(0.87,0,0.13,1)_forwards] [&>*]:min-h-0 [&>*]:overflow-hidden",
            id: props.id,
            attributes: props.attributes,
            {props.children}
        }
    }
}

const ACCORDION_KEYFRAMES: &str = r#"
@keyframes accordion-open {
  from {
    grid-template-rows: 0fr;
  }
  to {
    grid-template-rows: 1fr;
  }
}
@keyframes accordion-close {
  from {
    grid-template-rows: 1fr;
  }
  to {
    grid-template-rows: 0fr;
  }
}
"#;
