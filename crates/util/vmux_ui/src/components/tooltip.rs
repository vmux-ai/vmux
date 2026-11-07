use crate::components::popup::Dismissal;
use dioxus::prelude::*;

#[component]
pub fn Tooltip(label: String, children: Element) -> Element {
    let mut dismissed = use_signal(|| false);
    let mut active = use_signal(|| false);
    Dismissal::use_listener(
        active(),
        EventHandler::new(move |()| {
            dismissed.set(true);
            active.set(false);
        }),
    );
    rsx! {
        span {
            class: "group/tooltip relative inline-flex",
            onpointerenter: move |_| {
                dismissed.set(false);
                active.set(true);
            },
            onpointerleave: move |_| active.set(false),
            onfocusin: move |_| {
                dismissed.set(false);
                active.set(true);
            },
            onfocusout: move |_| active.set(false),
            onpointerdown: move |_| {
                dismissed.set(true);
                let _ = document::eval("document.activeElement?.blur();");
            },
            {children}
            if !dismissed() {
                span {
                    role: "tooltip",
                    class: "pointer-events-none absolute left-1/2 top-full z-[100] mt-2 -translate-x-1/2 whitespace-nowrap rounded-[var(--radius)] bg-foreground px-2.5 py-1.5 text-[11px] font-medium text-background opacity-0 shadow-lg transition-opacity delay-500 duration-100 group-hover/tooltip:opacity-100 group-focus-within/tooltip:opacity-100",
                    "{label}"
                }
            }
        }
    }
}
