use dioxus::prelude::*;

#[component]
pub fn Tooltip(label: String, children: Element) -> Element {
    rsx! {
        span { class: "group/tooltip relative inline-flex",
            {children}
            span {
                role: "tooltip",
                class: "pointer-events-none absolute left-1/2 top-full z-[100] mt-2 -translate-x-1/2 whitespace-nowrap rounded-[var(--radius)] bg-foreground px-2.5 py-1.5 text-[11px] font-medium text-background opacity-0 shadow-lg transition-opacity delay-500 duration-100 group-hover/tooltip:opacity-100 group-focus-within/tooltip:opacity-100",
                "{label}"
            }
        }
    }
}
