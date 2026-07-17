use dioxus::prelude::*;
use dioxus_primitives::context_menu::{
    ContextMenuContentProps, ContextMenuItemProps, ContextMenuProps, ContextMenuTriggerProps,
};

#[derive(Clone, Copy)]
struct ContextMenuState {
    open: Signal<bool>,
    position: Signal<(i32, i32)>,
    focused: Signal<Option<usize>>,
    max_index: Signal<usize>,
    disabled: ReadSignal<bool>,
    on_open_change: Callback<bool>,
}

impl ContextMenuState {
    fn set_open(mut self, open: bool) {
        self.open.set(open);
        if !open {
            self.focused.set(None);
        }
        self.on_open_change.call(open);
    }
}

#[component]
pub fn ContextMenu(props: ContextMenuProps) -> Element {
    let controlled = props.open;
    let mut open = use_signal(|| controlled().unwrap_or(props.default_open));
    let position = use_signal(|| (0, 0));
    let focused = use_signal(|| None);
    let max_index = use_signal(|| 0usize);
    let state = use_context_provider(|| ContextMenuState {
        open,
        position,
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
    let mut state: ContextMenuState = use_context();

    rsx! {
        div {
            role: "button",
            aria_haspopup: "menu",
            aria_expanded: (state.open)(),
            user_select: "none",
            oncontextmenu: move |event: Event<MouseData>| {
                if !(state.disabled)() {
                    state.position.set((
                        event.data().client_coordinates().x as i32,
                        event.data().client_coordinates().y as i32,
                    ));
                    state.set_open(true);
                    event.prevent_default();
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
    let (x, y) = (state.position)();

    if !(state.open)() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "fixed inset-0 z-[1000]",
            tabindex: "-1",
            onmounted: move |event| {
                let data = event.data();
                spawn(async move {
                    let _ = data.set_focus(true).await;
                });
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
                position: "fixed",
                left: "clamp(8px, {x}px, calc(100vw - 176px))",
                top: "clamp(8px, {y}px, calc(100vh - 48px))",
                class: "z-[1000] min-w-[168px] max-w-[calc(100vw-16px)] max-h-[calc(100vh-16px)] overflow-y-auto rounded-md bg-background p-1 shadow-[inset_0_0_0_1px_var(--border),0_8px_24px_rgba(0,0,0,0.24)] dark:bg-muted dark:shadow-[inset_0_0_0_1px_var(--primary),0_8px_24px_rgba(0,0,0,0.4)]",
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
                let _ = item.set_focus(true).await;
            });
        }
    });

    rsx! {
        div {
            role: "menuitem",
            tabindex: if (state.focused)() == Some(index) { "0" } else { "-1" },
            aria_disabled: disabled,
            "data-disabled": disabled,
            class: "flex min-h-8 max-w-full cursor-pointer select-none items-center overflow-hidden text-ellipsis whitespace-nowrap rounded px-2.5 py-1.5 text-[13px] leading-5 text-muted-foreground outline-none transition-colors data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50 hover:bg-accent hover:text-foreground focus:bg-accent focus:text-foreground dark:hover:bg-primary dark:hover:text-foreground dark:focus:bg-primary dark:focus:text-foreground",
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
