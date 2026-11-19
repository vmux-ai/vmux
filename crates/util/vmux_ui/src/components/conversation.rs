use dioxus::prelude::*;

#[component]
pub fn Page(
    accent: String,
    #[props(default)] on_keydown: Option<EventHandler<KeyboardEvent>>,
    #[props(default)] stylesheet: String,
    children: Element,
) -> Element {
    rsx! {
        main {
            class: "session-chat-page relative isolate flex h-dvh flex-col overflow-hidden bg-background text-foreground outline-none",
            style: "--agent-accent:{accent};",
            tabindex: "-1",
            onkeydown: move |event| {
                if let Some(handler) = on_keydown { handler.call(event); }
            },
            if !stylesheet.is_empty() { style { dangerous_inner_html: stylesheet } }
            {children}
        }
    }
}

#[component]
pub fn Transcript(
    #[props(default = "chat-scroll".to_string())] id: String,
    #[props(default)] initial_scroll: String,
    #[props(default)] placement_pending: bool,
    #[props(default = true)] at_bottom: bool,
    #[props(default)] filled: bool,
    #[props(default)] on_mounted: Option<EventHandler<MountedEvent>>,
    #[props(default)] on_scroll: Option<EventHandler<ScrollEvent>>,
    #[props(default)] on_wheel: Option<EventHandler<WheelEvent>>,
    #[props(default)] on_touchmove: Option<EventHandler<TouchEvent>>,
    #[props(default)] on_resize: Option<EventHandler<ResizeEvent>>,
    children: Element,
) -> Element {
    rsx! {
        div {
            id,
            "data-vmux-initial-scroll": initial_scroll,
            "data-vmux-presentation-pending": placement_pending.then_some(""),
            "data-vmux-follow-scroll": at_bottom.to_string(),
            "data-vmux-preserve-scroll": "auto",
            class: "relative z-10 min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 pb-20 pt-3 outline-none sm:px-4 md:px-6",
            tabindex: "0",
            onmounted: move |event| { if let Some(handler) = on_mounted { handler.call(event); } },
            onscroll: move |event| { if let Some(handler) = on_scroll { handler.call(event); } },
            onwheel: move |event| { if let Some(handler) = on_wheel { handler.call(event); } },
            ontouchmove: move |event| { if let Some(handler) = on_touchmove { handler.call(event); } },
            div {
                class: if filled { "mx-auto flex h-full min-h-full w-full max-w-3xl flex-col gap-5" } else { "mx-auto flex min-h-full w-full max-w-3xl flex-col gap-5" },
                onresize: move |event| { if let Some(handler) = on_resize { handler.call(event); } },
                {children}
            }
        }
    }
}

#[component]
pub fn Dock(children: Element) -> Element {
    rsx! {
        div { class: "relative z-20 px-4 pb-[calc(0.75rem+env(safe-area-inset-bottom))] pt-2",
            div { class: "session-chat-prompt-shell relative mx-auto flex max-w-3xl flex-col gap-2", {children} }
        }
    }
}
