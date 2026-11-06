use crate::components::popup::Surface;
use dioxus::prelude::*;
use dioxus_primitives::context_menu::{
    ContextMenuContentProps, ContextMenuItemProps, ContextMenuProps, ContextMenuTriggerProps,
};

#[derive(Clone, Copy)]
struct ContextMenuState {
    open: Signal<bool>,
    position: Signal<(i32, i32)>,
    activation: Signal<u64>,
    focused: Signal<Option<usize>>,
    max_index: Signal<usize>,
    disabled: ReadSignal<bool>,
    on_open_change: Callback<bool>,
}

impl ContextMenuState {
    fn open_at(mut self, position: (i32, i32)) {
        self.position.set(position);
        let activation = *self.activation.peek();
        self.activation.set(activation.wrapping_add(1));
        self.focused.set(None);
        self.set_open(true);
    }

    fn set_open(mut self, open: bool) {
        self.open.set(open);
        if !open {
            self.focused.set(None);
        }
        self.on_open_change.call(open);
    }
}

#[derive(Clone)]
struct ContextMenuOverlay(String);

impl ContextMenuOverlay {
    fn next() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        Self(format!("vmux-context-menu-{serial}"))
    }

    fn id(&self) -> String {
        self.0.clone()
    }
}

#[component]
pub fn ContextMenu(props: ContextMenuProps) -> Element {
    let controlled = props.open;
    let mut open = use_signal(|| controlled().unwrap_or(props.default_open));
    let position = use_signal(|| (0, 0));
    let activation = use_signal(|| 0u64);
    let focused = use_signal(|| None);
    let max_index = use_signal(|| 0usize);
    let state = use_context_provider(|| ContextMenuState {
        open,
        position,
        activation,
        focused,
        max_index,
        disabled: props.disabled,
        on_open_change: props.on_open_change,
    });

    use_effect(move || {
        if let Some(value) = controlled() {
            open.set(value);
        }
    });
    use_drop(move || {
        if *open.peek() {
            state.on_open_change.call(false);
        }
    });

    rsx! {
        div {
            "data-state": if (state.open)() { "open" } else { "closed" },
            "data-disabled": (state.disabled)(),
            ..props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn ContextMenuTrigger(props: ContextMenuTriggerProps) -> Element {
    let state: ContextMenuState = use_context();

    rsx! {
        div {
            role: "button",
            aria_haspopup: "menu",
            aria_expanded: (state.open)(),
            user_select: "none",
            oncontextmenu: move |event: Event<MouseData>| {
                if !(state.disabled)() {
                    state.open_at((
                        event.data().client_coordinates().x as i32,
                        event.data().client_coordinates().y as i32,
                    ));
                    event.prevent_default();
                    event.stop_propagation();
                }
            },
            ..props.attributes,
            {props.children}
        }
    }
}

#[component]
pub fn ContextMenuContent(props: ContextMenuContentProps) -> Element {
    let mut state: ContextMenuState = use_context();
    let overlay = use_hook(ContextMenuOverlay::next);
    let (x, y) = (state.position)();
    let animation = Surface::ENTER;
    let motion_style = Surface::style();

    if !(state.open)() {
        return rsx! {};
    }

    rsx! {
        div {
            id: overlay.id(),
            popover: "manual",
            class: "fixed inset-0 z-[1000] m-0 h-screen w-screen max-h-none max-w-none border-0 bg-transparent p-0 outline-none",
            style: motion_style,
            "data-vmux-popup-kind": Surface::Context.kind(),
            "data-vmux-popup-x": "{x}",
            "data-vmux-popup-y": "{y}",
            "data-vmux-popup-activation": "{(state.activation)()}",
            tabindex: "-1",
            onmounted: move |_| {
                crate::transport::Host::run_script(Surface::runtime());
            },
            onpointerdown: move |event| {
                event.prevent_default();
                state.set_open(false);
            },
            oncontextmenu: move |event| {
                event.prevent_default();
                state.set_open(false);
            },
            onkeydown: move |event: Event<KeyboardData>| {
                match event.key() {
                    Key::Escape => state.set_open(false),
                    Key::ArrowDown => {
                        let next = (state.focused)()
                            .map(|index| (index + 1).min((state.max_index)()))
                            .unwrap_or(0);
                        state.focused.set(Some(next));
                    }
                    Key::ArrowUp => {
                        let next = (state.focused)()
                            .map(|index| index.saturating_sub(1))
                            .unwrap_or((state.max_index)());
                        state.focused.set(Some(next));
                    }
                    Key::Home => state.focused.set(Some(0)),
                    Key::End => state.focused.set(Some((state.max_index)())),
                    _ => return,
                }
                event.prevent_default();
            },
            div {
                id: props.id,
                role: "menu",
                aria_orientation: "vertical",
                class: "fixed z-[1000] min-w-[132px] max-w-[calc(100vw-12px)] max-h-[calc(100vh-12px)] overflow-y-auto rounded-md border border-border bg-background p-0.5 dark:bg-muted {animation}",
                style: "left:clamp(6px, {x}px, calc(100vw - 140px));top:clamp(6px, {y}px, calc(100vh - 32px));",
                onpointerdown: move |event| event.stop_propagation(),
                oncontextmenu: move |event| {
                    event.prevent_default();
                    event.stop_propagation();
                },
                ..props.attributes,
                {props.children}
            }
        }
    }
}

#[component]
pub fn ContextMenuItem(props: ContextMenuItemProps) -> Element {
    let mut state: ContextMenuState = use_context();
    let index = (props.index)();
    let disabled = (props.disabled)() || (state.disabled)();
    let mut item_ref: Signal<Option<std::rc::Rc<MountedData>>> = use_signal(|| None);

    use_effect(move || {
        if index > (state.max_index)() {
            state.max_index.set(index);
        }
    });
    use_effect(move || {
        if (state.open)()
            && (state.focused)() == Some(index)
            && let Some(item) = item_ref()
        {
            spawn(async move {
                if let Err(error) = item.set_focus(true).await {
                    dioxus::logger::tracing::warn!(
                        "focusing a context menu item failed: {error:?}"
                    );
                }
            });
        }
    });

    rsx! {
        div {
            role: "menuitem",
            tabindex: if (state.focused)() == Some(index) { "0" } else { "-1" },
            aria_disabled: disabled,
            "data-disabled": disabled,
            class: "flex min-h-6 max-w-full cursor-pointer select-none items-center overflow-hidden text-ellipsis whitespace-nowrap rounded px-2 py-0.5 text-xs leading-4 text-muted-foreground outline-none transition-colors data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50 hover:bg-accent hover:text-foreground focus:bg-accent focus:text-foreground dark:hover:bg-primary dark:hover:text-foreground dark:focus:bg-primary dark:focus:text-foreground",
            onmounted: move |event| item_ref.set(Some(event.data())),
            onpointerenter: move |_| state.focused.set(Some(index)),
            onpointerdown: move |event| {
                if !disabled {
                    props.on_select.call((props.value)());
                    state.set_open(false);
                }
                event.prevent_default();
                event.stop_propagation();
            },
            onkeydown: move |event: Event<KeyboardData>| {
                let select = match event.key() {
                    Key::Enter => true,
                    Key::Character(value) => value == " ",
                    _ => false,
                };
                if select {
                    if !disabled {
                        props.on_select.call((props.value)());
                        state.set_open(false);
                    }
                    event.prevent_default();
                    event.stop_propagation();
                }
            },
            ..props.attributes,
            {props.children}
        }
    }
}

#[cfg(test)]
mod tests {
    use dioxus::core::{AttributeValue, Mutation, VirtualDom};
    use dioxus_html::{
        Code, EventData, Location, Modifiers, SerializedHtmlEventConverter, SerializedKeyboardData,
        SerializedMouseData,
    };
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Clone, Default)]
    struct MenuProbe {
        opened: Rc<RefCell<Vec<bool>>>,
        selected: Rc<RefCell<Vec<String>>>,
    }

    #[component]
    fn GestureMenu() -> Element {
        let probe = use_context::<MenuProbe>();
        let opened = probe.opened;
        let selected = probe.selected;
        let value = use_signal(|| "archive".to_string());
        rsx! {
            ContextMenu {
                attributes: vec![],
                on_open_change: move |open| opened.borrow_mut().push(open),
                ContextMenuTrigger { attributes: vec![], "row" }
                ContextMenuContent { attributes: vec![],
                    ContextMenuItem {
                        attributes: vec![], index: 0usize,
                        value: Into::<ReadSignal<String>>::into(value),
                        on_select: move |value| selected.borrow_mut().push(value),
                        "Archive"
                    }
                }
            }
        }
    }

    #[test]
    fn right_click_declares_a_popup_in_the_first_frame_and_selection_closes_immediately() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let probe = MenuProbe::default();
        let mut dom = VirtualDom::new(GestureMenu);
        dom.provide_root_context(probe.clone());
        let initial = dom.rebuild_to_vec();
        let mut trigger = None;
        for edit in initial.edits {
            if let Mutation::NewEventListener { name, id } = edit
                && name == "contextmenu"
            {
                trigger = Some(id);
            }
        }
        let data = EventData::Mouse(SerializedMouseData::default());
        dom.runtime().handle_event(
            "contextmenu",
            Event::new(data.into_any(), true),
            trigger.unwrap(),
        );
        let opened = dom.render_immediate_to_vec();
        let mut declared = false;
        let mut select = None;
        for edit in opened.edits {
            match edit {
                Mutation::SetAttribute {
                    name: "data-vmux-popup-kind",
                    value: AttributeValue::Text(value),
                    ..
                } if value == "context" => declared = true,
                Mutation::NewEventListener { name, id } if name == "keydown" => select = Some(id),
                _ => {}
            }
        }
        assert!(declared);
        assert_eq!(*probe.opened.borrow(), [true]);
        let data = EventData::Keyboard(SerializedKeyboardData::new(
            Key::Enter,
            Code::Enter,
            Location::Standard,
            false,
            Modifiers::empty(),
            false,
        ));
        dom.runtime().handle_event(
            "keydown",
            Event::new(data.into_any(), true),
            select.unwrap(),
        );
        dom.render_immediate_to_vec();
        assert_eq!(*probe.selected.borrow(), ["archive"]);
        assert_eq!(*probe.opened.borrow(), [true, false]);
        assert!(!dioxus_ssr::render(&dom).contains("role=\"menu\""));
    }
}
