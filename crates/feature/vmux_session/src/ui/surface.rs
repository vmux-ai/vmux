use dioxus::prelude::*;
use vmux_ui::components::message::WelcomeLayout;
use vmux_ui::motion::Motion;

#[component]
pub(super) fn Page(
    accent: String,
    #[props(default)] setup: bool,
    #[props(default)] onboarding_entry: bool,
    #[props(default)] on_keydown: Option<EventHandler<KeyboardEvent>>,
    children: Element,
) -> Element {
    use_hook(vmux_ui::components::motion::Disclosure::install);
    let file_placement = super::FilePlacement::provide();
    rsx! {
        super::item::ToolFileMenu {
        main {
            class: "session-chat-page relative isolate flex h-dvh flex-col overflow-hidden bg-background text-foreground outline-none",
            style: "--agent-accent:{accent};",
            onresize: move |event: ResizeEvent| file_placement.resize(&event.data()),
            "data-vmux-setup-motion": setup.then_some(""),
            "data-vmux-onboarding-entry": onboarding_entry.then_some(""),
            tabindex: "-1",
            onkeydown: move |event| {
                if let Some(handler) = on_keydown { handler.call(event); }
            },
            style { dangerous_inner_html: super::item::MD_CSS }
            style { dangerous_inner_html: Entrance::css() }
            {children}
        }
        }
    }
}

struct Entrance;

impl Entrance {
    fn css() -> String {
        Self::STYLE
            .replace("__VMUX_ENTRY_MS__", &Motion::MAX_MS.to_string())
            .replace(
                "__VMUX_DOCK_OFFSET__",
                &WelcomeLayout::dock_offset().to_string(),
            )
    }

    const STYLE: &str = r#"
.session-chat-page{--vmux-entry-ms:__VMUX_ENTRY_MS__ms;--vmux-dock-ms:280ms}
[data-vmux-onboarding-entry] .onboarding-origin{animation:vmux-guide-dock var(--vmux-dock-ms) cubic-bezier(.22,.1,.2,1) backwards;animation-play-state:paused}
[data-vmux-onboarding-entry] .onboarding-origin .chat-message-enter{animation:none}
[data-vmux-onboarding-entry] .session-chat-prompt-shell{animation:vmux-composer-dock var(--vmux-dock-ms) cubic-bezier(.22,.1,.2,1) backwards;animation-play-state:paused}
[data-vmux-onboarding-entry] .setup-notice{min-height:48px;animation:vmux-choice-open 200ms ease-out 70ms backwards;animation-play-state:paused}
[data-vmux-onboarding-entry] .onboarding-detail,
[data-vmux-onboarding-entry] .composer-notice-content{animation:vmux-setup-detail var(--vmux-entry-ms) ease-out 100ms backwards;animation-play-state:paused}
[data-vmux-onboarding-entry] .chat-message-enter{animation-play-state:paused}
body:not([inert]) [data-vmux-onboarding-entry] .onboarding-origin,
body:not([inert]) [data-vmux-onboarding-entry] .session-chat-prompt-shell,
body:not([inert]) [data-vmux-onboarding-entry] .setup-notice,
body:not([inert]) [data-vmux-onboarding-entry] .onboarding-detail,
body:not([inert]) [data-vmux-onboarding-entry] .composer-notice-content,
body:not([inert]) [data-vmux-onboarding-entry] .chat-message-enter{animation-play-state:running}
[data-vmux-setup-motion] [role="option"]{transition:background-color 80ms ease-out}
@keyframes vmux-guide-dock{from{transform:translateY(calc(50dvh - __VMUX_DOCK_OFFSET__px))}to{transform:translateY(0)}}
@keyframes vmux-composer-dock{from{transform:translateY(calc(__VMUX_DOCK_OFFSET__px - 50dvh))}to{transform:translateY(0)}}
@keyframes vmux-choice-open{from{grid-template-rows:0fr}to{grid-template-rows:1fr}}
@keyframes vmux-setup-detail{from{opacity:0;transform:translateY(6px)}to{opacity:1;transform:translateY(0)}}
@media(max-width:1px),(max-height:1px){.session-chat-page .onboarding-origin,.session-chat-page .chat-message-enter,.session-chat-page .session-chat-prompt-shell,.session-chat-page .setup-notice,.session-chat-page .onboarding-detail,.session-chat-page .composer-notice-content{animation-play-state:paused!important}}
@media(prefers-reduced-motion:reduce){.session-chat-page .onboarding-origin,.session-chat-page .chat-message-enter,.session-chat-page .session-chat-prompt-shell,.session-chat-page .setup-notice,.session-chat-page .onboarding-detail,.session-chat-page .composer-notice-content,.session-chat-page [role="option"]{animation:none!important;transition:none!important}}
"#;
}

#[component]
pub(super) fn Transcript(
    #[props(default = "chat-scroll".to_string())] id: String,
    #[props(default)] initial_scroll: String,
    #[props(default)] placement_pending: bool,
    #[props(default = true)] at_bottom: bool,
    #[props(default)] filled: bool,
    #[props(default)] busy: bool,
    #[props(default)] reserve_details_toggle: bool,
    #[props(default)] on_mounted: Option<EventHandler<MountedEvent>>,
    #[props(default)] on_scroll: Option<EventHandler<ScrollEvent>>,
    #[props(default)] on_scrollend: Option<EventHandler<ScrollEvent>>,
    #[props(default)] on_wheel: Option<EventHandler<WheelEvent>>,
    #[props(default)] on_touchmove: Option<EventHandler<TouchEvent>>,
    #[props(default)] on_resize: Option<EventHandler<ResizeEvent>>,
    children: Element,
) -> Element {
    let bottom_padding = if busy { "pb-12" } else { "pb-20" };
    let toggle_space = if reserve_details_toggle {
        "[&_.chat-user-bubble]:pr-10"
    } else {
        ""
    };
    rsx! {
        div {
            id,
            "data-vmux-sticky-container": "",
            "data-vmux-initial-scroll": initial_scroll,
            "data-vmux-presentation-pending": placement_pending.then_some(""),
            "data-vmux-follow-scroll": at_bottom.to_string(),
            "data-vmux-preserve-scroll": "auto",
            class: "relative z-10 min-h-0 flex-1 overflow-y-auto overscroll-contain scroll-pt-12 px-3 {bottom_padding} {toggle_space} outline-none sm:px-4 md:px-6",
            tabindex: "0",
            onmounted: move |event| { if let Some(handler) = on_mounted { handler.call(event); } },
            onscroll: move |event| { if let Some(handler) = on_scroll { handler.call(event); } },
            onscrollend: move |event| { if let Some(handler) = on_scrollend { handler.call(event); } },
            onwheel: move |event| { if let Some(handler) = on_wheel { handler.call(event); } },
            ontouchmove: move |event| { if let Some(handler) = on_touchmove { handler.call(event); } },
            div {
                class: "mx-auto flex min-h-full w-full max-w-3xl flex-col gap-5 pt-3",
                style: filled.then_some("height:100%;"),
                onresize: move |event| { if let Some(handler) = on_resize { handler.call(event); } },
                {children}
            }
        }
    }
}

#[component]
pub(super) fn Dock(children: Element) -> Element {
    rsx! {
        div { class: "relative z-20 px-4 pb-[calc(0.75rem+env(safe-area-inset-bottom))] pt-2",
            div { class: "session-chat-prompt-shell relative mx-auto flex max-w-3xl flex-col gap-2", {children} }
        }
    }
}
