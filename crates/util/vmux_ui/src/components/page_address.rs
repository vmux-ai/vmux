use crate::clipboard::Clipboard;
use crate::components::icon::Icon;
use crate::hooks::{send, use_ui_state};
use crate::i18n::translate;
use dioxus::prelude::*;
use vmux_api::address::{AddressIntent, PageAddress, PageAddressPublish, PageAddressUiState};

#[component]
pub fn PageBreadcrumbs(
    state: PageAddress,
    #[props(default)] onintent: Option<EventHandler<AddressIntent>>,
) -> Element {
    use_effect(use_reactive!(|(state,)| {
        let _ = send(&PageAddressPublish {
            address: state.clone(),
        });
    }));
    let source = use_ui_state::<PageAddressUiState>().state;
    let mut observed = use_signal(move || {
        source
            .peek()
            .effect
            .as_ref()
            .map_or(0, |effect| effect.revision)
    });
    use_effect(move || {
        let effect = source.read().effect.clone();
        let Some(effect) = effect else {
            return;
        };
        if effect.revision <= *observed.peek() {
            return;
        }
        observed.set(effect.revision);
        if source
            .read()
            .address
            .as_ref()
            .is_none_or(|address| !effect.matches(address))
        {
            return;
        }
        if let Some(onintent) = onintent {
            onintent.call(effect.intent);
        }
    });
    rsx! {}
}

#[component]
pub fn PageToolbar(
    children: Element,
    #[props(default = rsx! {})] actions: Element,
    #[props(default = rsx! {})] view_controls: Element,
) -> Element {
    rsx! { div { class: "flex shrink-0 flex-wrap items-center justify-between gap-2 px-3 py-2",
        div { class: "flex min-w-0 flex-wrap items-center gap-2", {children} }
        div { class: "flex shrink-0 flex-wrap items-center gap-2", {view_controls}, {actions} }
    } }
}

#[component]
pub fn AddressSurface(url: String, onactivate: EventHandler<()>, children: Element) -> Element {
    let copy_url = url.clone();
    rsx! {
        div { class: "group/address relative flex h-8 min-w-0 flex-1 items-center",
            button {
                r#type: "button",
                "data-vmux-url-edit": "",
                class: "absolute inset-0 appearance-none rounded-md border-0 bg-transparent outline-none cursor-text focus-visible:bg-foreground/[0.04]",
                title: url.clone(),
                aria_label: url,
                onclick: move |_| onactivate.call(()),
            }
            div {
                class: "pointer-events-none relative flex h-full min-w-0 flex-1 items-center px-1 [&_button]:pointer-events-auto [&_a]:pointer-events-auto [&_input]:pointer-events-auto [&_textarea]:pointer-events-auto [&_select]:pointer-events-auto [&_[role=button]]:pointer-events-auto [&_[role=tab]]:pointer-events-auto [&_[popover]]:pointer-events-auto [&_[contenteditable]]:pointer-events-auto [&_[data-vmux-address-item]]:pointer-events-auto [&_[data-vmux-dropdown-root]]:pointer-events-auto",
                {children}
            }
            if !copy_url.is_empty() {
                button {
                    r#type: "button",
                    class: "relative z-10 ml-1 flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover/address:opacity-100 focus-visible:opacity-100 hover:bg-foreground/[0.05] hover:text-foreground",
                    title: translate("common-copy-url"),
                    aria_label: translate("common-copy-url"),
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        Clipboard::write(&copy_url);
                    },
                    Icon { class: "size-3.5",
                        rect { x: "9", y: "9", width: "13", height: "13", rx: "2" }
                        path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                    }
                }
            }
        }
    }
}
