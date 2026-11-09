use dioxus::core::Task;
use dioxus::core::current_scope_id;
use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

use crate::cn::cn;
use crate::components::popup::{Dismissal, Surface};
use crate::icon::{LineIcon, LineIconView};

const DROPDOWN_CONNECTION_WATCHER: &str = r#"
const watchConnection = globalThis.__vmuxDropdownWatchConnection ??= (() => {
  const entries = new Map();
  let observer = null;
  const stop = () => {
    if (entries.size || !observer) return;
    observer.disconnect();
    observer = null;
  };
  const inspect = () => {
    for (const [key, entry] of entries) {
      if (entry.active()) continue;
      entries.delete(key);
      for (const cleanup of entry.cleanups) cleanup();
    }
    stop();
  };
  const start = () => {
    if (observer) return;
    observer = new MutationObserver(inspect);
    observer.observe(document, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['data-vmux-dropdown-open'],
    });
  };
  return (key, element, cleanup, active = () => element.isConnected) => {
    let entry = entries.get(key);
    if (!entry) {
      entry = { element, active, cleanups: new Set() };
      entries.set(key, entry);
    }
    entry.element = element;
    entry.active = active;
    entry.cleanups.add(cleanup);
    start();
    return () => {
      const current = entries.get(key);
      if (!current) return;
      current.cleanups.delete(cleanup);
      if (!current.cleanups.size) entries.delete(key);
      stop();
    };
  };
})();
"#;

const DROPDOWN_KEYBOARD_SCRIPT: &str = r#"
__VMUX_POPUP_RUNTIME__
__VMUX_DROPDOWN_CONNECTION_WATCHER__
globalThis.__vmuxDropdownKeyboardCleanup?.();
let active = true;
const root = document.getElementById('__VMUX_DROPDOWN_ROOT__');
let unwatch = () => {};
const rootFor = event => event.target instanceof Element
  ? event.target.closest('[data-vmux-dropdown-keyboard="true"]')
  : null;
const optionsFor = root => [...root.querySelectorAll('[data-vmux-dropdown-option]:not(:disabled)')];
const clear = root => {
  for (const option of optionsFor(root)) {
    option.removeAttribute('data-vmux-dropdown-active');
    option.removeAttribute('aria-selected');
  }
};
const activate = (root, options, index) => {
  clear(root);
  const option = options[index];
  option.setAttribute('data-vmux-dropdown-active', 'true');
  option.setAttribute('aria-selected', 'true');
  const popup = option.closest('[data-vmux-dropdown-popup]');
  if (popup) globalThis.__vmuxPopupPresentation.reveal(popup, option);
};
const keydown = event => {
  if (event.isComposing) return;
  const root = rootFor(event);
  if (!root) return;
  const options = optionsFor(root);
  if (!options.length) return;
  const current = options.findIndex(option => option.getAttribute('data-vmux-dropdown-active') === 'true');
  const control = event.ctrlKey && !event.altKey && !event.metaKey;
  const key = event.key.toLowerCase();
  const forward = event.key === 'ArrowDown' || (control && key === 'n');
  const backward = event.key === 'ArrowUp' || (control && key === 'p');
  if (forward || backward) {
    event.preventDefault();
    event.stopPropagation();
    const index = forward
      ? (current < 0 ? 0 : (current + 1) % options.length)
      : (current < 0 ? options.length - 1 : (current - 1 + options.length) % options.length);
    activate(root, options, index);
    return;
  }
  if (event.key === 'Enter' && current >= 0) {
    event.preventDefault();
    event.stopPropagation();
    options[current].click();
  }
};
const input = event => {
  const root = rootFor(event);
  if (root) clear(root);
};
const cleanup = () => {
  if (!active) return;
  active = false;
  unwatch();
  document.removeEventListener('keydown', keydown, true);
  document.removeEventListener('input', input, true);
  if (globalThis.__vmuxDropdownKeyboardCleanup === cleanup) {
    delete globalThis.__vmuxDropdownKeyboardCleanup;
  }
};
globalThis.__vmuxDropdownKeyboardCleanup = cleanup;
document.addEventListener('keydown', keydown, true);
document.addEventListener('input', input, true);
if (root) {
  unwatch = watchConnection(
    `keyboard:${root.id}`,
    root,
    cleanup,
    () => root.isConnected && root.getAttribute('data-vmux-dropdown-open') === 'true',
  );
}
dioxus.send(true);
"#;

const DROPDOWN_POPUP_SCRIPT: &str = r#"
__VMUX_POPUP_RUNTIME__
__VMUX_DROPDOWN_CONNECTION_WATCHER__
const id = '__VMUX_DROPDOWN_POPUP__';
const entries = globalThis.__vmuxDropdownPopupEntries ??= new Map();
entries.get(id)?.cleanup();
const popup = document.getElementById(id);
const root = popup?.closest('[data-vmux-dropdown-root]');
if (popup && root) {
  for (const other of document.querySelectorAll(
    '[data-vmux-dropdown-popup]:popover-open'
  )) {
    if (other !== popup) other.hidePopover();
  }
  let active = true;
  let frame = 0;
  const anchor = root.querySelector('[data-vmux-dropdown-trigger]') ?? root;
  let unwatch = () => {};
  const place = () => {
    frame = 0;
    if (!active || !popup.isConnected || !popup.matches(':popover-open')) return;
    globalThis.__vmuxPopupPresentation.place(popup);
  };
  const schedule = () => {
    if (!active || frame) return;
    frame = requestAnimationFrame(place);
  };
  const toggle = event => {
    if (event.newState === 'open') schedule();
  };
  const observer = new ResizeObserver(schedule);
  observer.observe(anchor);
  observer.observe(popup);
  popup.addEventListener('toggle', toggle);
  window.addEventListener('resize', schedule, true);
  window.addEventListener('scroll', schedule, true);
  const cleanup = () => {
    if (!active) return;
    active = false;
    if (frame) cancelAnimationFrame(frame);
    observer.disconnect();
    unwatch();
    popup.removeEventListener('toggle', toggle);
    window.removeEventListener('resize', schedule, true);
    window.removeEventListener('scroll', schedule, true);
    if (popup.matches(':popover-open')) {
      try { popup.hidePopover(); } catch (error) {}
    }
    if (entries.get(id)?.cleanup === cleanup) entries.delete(id);
  };
  unwatch = watchConnection(id, popup, cleanup);
  entries.set(id, { cleanup });
  place();
}
"#;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DropdownAlign {
    #[default]
    Start,
    End,
}

impl DropdownAlign {
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::End => "end",
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DropdownPlacement {
    #[default]
    Bottom,
    Top,
    Right,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DropdownOpenMode {
    #[default]
    Click,
    Hover,
}

#[component]
pub fn DropdownToggleIndicator(open: bool) -> Element {
    rsx! {
        span {
            "data-vmux-dropdown-indicator": "",
            "data-state": if open { "open" } else { "closed" },
            class: "inline-flex shrink-0 text-muted-foreground/60 opacity-0 transition-[opacity,transform] data-[state=open]:rotate-180 data-[state=open]:opacity-100 group-hover:opacity-100 group-focus-visible:opacity-100",
        LineIconView {
            icon: LineIcon::ChevronDown,
            class: "size-3".to_string(),
        }
        }
    }
}

#[component]
pub fn DropdownTrigger(
    label: String,
    open: bool,
    onclick: EventHandler<()>,
    #[props(default)] on_hover_open: Option<EventHandler<()>>,
    #[props(default)] title: String,
    #[props(default)] active: bool,
    #[props(default)] disabled: bool,
    #[props(default)] open_mode: DropdownOpenMode,
    #[props(default = 120)] hover_delay_ms: u32,
    #[props(default)] class: String,
    #[props(default)] label_class: String,
    #[props(default = true)] truncate: bool,
    #[props(default = rsx! {})] leading: Element,
    #[props(default = rsx! {})] trailing: Element,
) -> Element {
    let mut hover_task = use_signal(|| None::<Task>);
    let mut hover_preview = use_signal(|| false);
    let context = try_consume_context::<DropdownContext>();
    use_effect(use_reactive!(|(open, open_mode)| {
        let _ = (open, open_mode);
        if !open {
            hover_preview.set(false);
        }
        if let Some(task) = hover_task.write().take() {
            task.cancel();
        }
    }));
    use_drop(move || {
        if let Some(task) = hover_task.write().take() {
            task.cancel();
        }
    });
    let tone = if active {
        "font-medium text-foreground"
    } else {
        "text-muted-foreground hover:text-foreground"
    };
    rsx! {
        button {
            r#type: "button",
            "data-vmux-dropdown-trigger": "",
            class: cn([
                "group flex h-7 w-fit max-w-full min-w-0 items-center gap-1.5 rounded-md px-1.5 text-xs transition-colors hover:bg-foreground/[0.05]",
                tone,
                class.as_str(),
            ]),
            title,
            aria_expanded: open,
            disabled,
            onmouseenter: move |_| {
                if open_mode == DropdownOpenMode::Hover
                    && let Some(on_hover_open) = on_hover_open
                {
                    if hover_delay_ms == 0 {
                        if let Some(context) = context {
                            context.activate.call(());
                        }
                        hover_preview.set(true);
                        on_hover_open.call(());
                        return;
                    }
                    if let Some(task) = hover_task.write().take() {
                        task.cancel();
                    }
                    hover_task.set(Some(spawn(async move {
                        crate::platform::Platform::sleep(hover_delay_ms).await;
                        if let Some(context) = context {
                            context.activate.call(());
                        }
                        hover_preview.set(true);
                        on_hover_open.call(());
                    })));
                }
            },
            onmouseleave: move |_| {
                if open_mode == DropdownOpenMode::Hover
                    && let Some(task) = hover_task.write().take() {
                        task.cancel();
                    }
            },
            onclick: move |_| {
                if open_mode == DropdownOpenMode::Hover
                    && let Some(task) = hover_task.write().take() {
                        task.cancel();
                }
                if open && *hover_preview.peek() {
                    hover_preview.set(false);
                    return;
                }
                hover_preview.set(false);
                if !open && let Some(context) = context {
                    context.activate.call(());
                }
                onclick.call(());
            },
            {leading}
            span { class: cn([if truncate { "min-w-0 truncate" } else { "shrink-0 whitespace-nowrap" }, label_class.as_str()]), "{label}" }
            {trailing}
        }
    }
}

#[component]
pub fn DropdownOption(
    label: String,
    selected: bool,
    onclick: EventHandler<MouseEvent>,
    #[props(default)] disabled: bool,
    #[props(default)] title: String,
    #[props(default)] secondary: String,
    #[props(default)] class: String,
    #[props(default = rsx! {})] leading: Element,
    #[props(default = rsx! {})] trailing: Element,
) -> Element {
    let tone = if disabled {
        "cursor-not-allowed text-muted-foreground/45"
    } else if selected {
        "bg-foreground/[0.075] text-foreground"
    } else {
        "text-muted-foreground hover:bg-foreground/[0.055] hover:text-foreground"
    };
    rsx! {
        button {
            r#type: "button",
            disabled,
            "data-vmux-dropdown-option": "",
            "data-vmux-dropdown-selected": if selected { "true" } else { "false" },
            class: cn([
                "flex min-h-8 w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition-colors data-[vmux-dropdown-active=true]:bg-foreground/[0.07] data-[vmux-dropdown-active=true]:text-foreground",
                tone,
                class.as_str(),
            ]),
            title,
            onclick: move |event| onclick.call(event),
            {leading}
            div { class: "min-w-0 flex-1",
                div { class: "truncate font-medium", "{label}" }
                if !secondary.is_empty() {
                    div { class: "truncate font-mono text-[10px] font-normal text-muted-foreground", "{secondary}" }
                }
            }
            {trailing}
        }
    }
}

#[component]
pub fn DropdownSelector(
    label: String,
    open: bool,
    on_open_change: EventHandler<bool>,
    on_dismiss: EventHandler<()>,
    children: Element,
    #[props(default)] title: String,
    #[props(default)] active: bool,
    #[props(default)] disabled: bool,
    #[props(default)] class: String,
    #[props(default)] trigger_class: String,
    #[props(default = true)] truncate: bool,
    #[props(default)] popup_class: String,
    #[props(default)] align: DropdownAlign,
    #[props(default)] placement: DropdownPlacement,
    #[props(default)] open_mode: DropdownOpenMode,
    #[props(default = 120)] hover_delay_ms: u32,
    #[props(default)] match_width: bool,
    #[props(default)] horizontal_inset: f64,
    #[props(default = 4.0)] gap: f64,
    #[props(default = rsx! {})] leading: Element,
    #[props(default = rsx! {})] trailing: Element,
) -> Element {
    rsx! {
        Dropdown {
            open,
            keyboard_navigation: true,
            dismiss_on_mouseleave: open_mode == DropdownOpenMode::Hover,
            class,
            on_dismiss: move |_| on_dismiss.call(()),
            DropdownTrigger {
                label,
                truncate,
                title,
                open,
                active,
                disabled,
                open_mode,
                hover_delay_ms,
                class: trigger_class,
                leading,
                trailing: rsx! {
                    {trailing}
                    if open_mode == DropdownOpenMode::Click {
                        DropdownToggleIndicator { open }
                    }
                },
                on_hover_open: move |_| {
                    on_open_change.call(true);
                },
                onclick: move |_| {
                    on_open_change.call(!open);
                },
            }
            if open {
                DropdownPopup {
                    align,
                    placement,
                    match_width,
                    horizontal_inset,
                    gap,
                    class: popup_class,
                    {children}
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct DropdownContext {
    dismiss: EventHandler<()>,
    hover_enter: EventHandler<()>,
    hover_leave: EventHandler<()>,
    manual: bool,
    activation: Signal<u64>,
    activate: Callback<()>,
}

impl DropdownPlacement {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Right => "right",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct DropdownPopupOwner(usize);

impl DropdownPopupOwner {
    fn id(self) -> String {
        format!("vmux-dropdown-popup-{}", self.0)
    }

    fn install_script(self) -> String {
        DROPDOWN_POPUP_SCRIPT
            .replace("__VMUX_POPUP_RUNTIME__", &Surface::runtime())
            .replace(
                "__VMUX_DROPDOWN_CONNECTION_WATCHER__",
                DROPDOWN_CONNECTION_WATCHER,
            )
            .replace("__VMUX_DROPDOWN_POPUP__", &self.id())
    }

    fn dismiss_script(self, activation: u64) -> String {
        format!(
            r#"
__VMUX_DROPDOWN_CONNECTION_WATCHER__
const id = '{}';
const activation = {};
const popup = document.getElementById(id);
if (popup) {{
  let opened = popup.matches(':popover-open');
  const root = popup.closest('[data-vmux-dropdown-root]');
  const manual = popup.getAttribute('popover') === 'manual';
  const entries = globalThis.__vmuxDropdownToggleEntries ??= new Map();
  entries.get(id)?.cleanup();
  let active = true;
  let unwatch = () => {{}};
  const cleanup = () => {{
    if (!active) return;
    active = false;
    unwatch();
    popup.removeEventListener('beforetoggle', beforetoggle);
    document.removeEventListener('pointerdown', pointerdown, true);
    document.removeEventListener('keydown', keydown, true);
    if (entries.get(id)?.cleanup === cleanup) entries.delete(id);
  }};
  const dismiss = () => {{
    if (!active) return;
    cleanup();
    if (Number(root?.dataset.vmuxDropdownActivation ?? 0) !== activation) return;
    dioxus.send('dismissed');
  }};
  const beforetoggle = event => {{
    if (event.newState === 'open') opened = true;
    if (event.newState === 'closed' && opened) dismiss();
  }};
  const pointerdown = event => {{
    const path = event.composedPath();
    if (path.includes(popup) || (root && path.includes(root))) return;
    dismiss();
  }};
  const keydown = event => {{
    if (event.key !== 'Escape' || popup.getAttribute('popover') !== 'manual') return;
    event.preventDefault();
    event.stopPropagation();
    dismiss();
  }};
  entries.set(id, {{ cleanup }});
  unwatch = watchConnection(id, popup, cleanup);
  popup.addEventListener('beforetoggle', beforetoggle);
  if (manual) {{
    document.addEventListener('pointerdown', pointerdown, true);
    document.addEventListener('keydown', keydown, true);
  }}
}}
"#,
            self.id(),
            activation
        )
        .replace(
            "__VMUX_DROPDOWN_CONNECTION_WATCHER__",
            DROPDOWN_CONNECTION_WATCHER,
        )
    }

    fn mount(self) {
        crate::transport::Host::show_dropdown_popover(&self.id());
        crate::transport::Host::run_script(self.install_script());
    }
}

#[component]
pub fn Dropdown(
    open: bool,
    #[props(default)] class: String,
    #[props(default)] keyboard_navigation: bool,
    #[props(default)] dismiss_on_mouseleave: bool,
    on_dismiss: EventHandler<()>,
    children: Element,
) -> Element {
    let mut leave_task = use_signal(|| None::<Task>);
    let mut dismissing = use_signal(|| false);
    let mut activation = use_signal(|| 0u64);
    let activate = use_callback(move |()| {
        let next = activation.peek().wrapping_add(1);
        activation.set(next);
    });
    use_effect(use_reactive!(|(open, dismiss_on_mouseleave)| {
        let _ = (open, dismiss_on_mouseleave);
        if let Some(task) = leave_task.write().take() {
            task.cancel();
        }
        if open && *dismissing.peek() {
            dismissing.set(false);
        }
    }));
    use_drop(move || {
        if let Some(task) = leave_task.write().take() {
            task.cancel();
        }
    });
    let dismiss = use_callback(move |()| {
        if *dismissing.peek() {
            return;
        }
        dismissing.set(true);
        if let Some(task) = leave_task.write().take() {
            task.cancel();
        }
        on_dismiss.call(());
    });
    Dismissal::use_listener(open, dismiss);
    let hover_enter = use_callback(move |()| {
        if !dismiss_on_mouseleave {
            return;
        }
        if let Some(task) = leave_task.write().take() {
            task.cancel();
        }
    });
    let hover_leave = use_callback(move |()| {
        if !dismiss_on_mouseleave || !open {
            return;
        }
        if let Some(task) = leave_task.write().take() {
            task.cancel();
        }
        leave_task.set(Some(spawn(async move {
            crate::platform::Platform::sleep(120).await;
            dismiss.call(());
        })));
    });
    use_context_provider(|| DropdownContext {
        dismiss,
        hover_enter,
        hover_leave,
        manual: dismiss_on_mouseleave,
        activation,
        activate,
    });
    let root_id = format!("vmux-dropdown-root-{}", current_scope_id().0);
    let layer = if open { "z-[100]" } else { "z-auto" };
    let duration = crate::motion::Motion::FEEDBACK_MS;
    let root_class = cn([
        "relative [&_button]:transition-[background-color,color,opacity,transform] [&_button]:duration-[var(--vmux-motion-ms)] [&_button]:ease-out [&_svg]:transition-transform [&_svg]:duration-[var(--vmux-motion-ms)] motion-reduce:[&_button]:transition-none motion-reduce:[&_svg]:transition-none",
        layer,
        class.as_str(),
    ]);
    rsx! {
        div {
            id: root_id.clone(),
            class: root_class,
            style: "--vmux-motion-ms:{duration}ms;",
            "data-vmux-dropdown-root": "",
            "data-vmux-dropdown-open": if open { "true" } else { "false" },
            "data-vmux-dropdown-activation": activation().to_string(),
            "data-vmux-dropdown-keyboard": if keyboard_navigation { "true" } else { "false" },
            onmouseenter: move |_| hover_enter.call(()),
            onmouseleave: move |_| hover_leave.call(()),
            onkeydown: move |event: KeyboardEvent| {
                if open && event.key() == Key::Escape {
                    event.prevent_default();
                    event.stop_propagation();
                    dismiss.call(());
                }
            },
            if open {
                if keyboard_navigation {
                    DropdownKeyboardNavigation { root_id: root_id.clone() }
                }
            }
            {children}
        }
    }
}

#[component]
pub fn DropdownPopup(
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(default)] align: DropdownAlign,
    #[props(default)] placement: DropdownPlacement,
    #[props(default)] match_width: bool,
    #[props(default)] horizontal_inset: f64,
    #[props(default = 4.0)] gap: f64,
    children: Element,
) -> Element {
    let owner = DropdownPopupOwner(current_scope_id().0);
    let context = use_context::<DropdownContext>();
    let activation = (context.activation)();
    let mut registered = use_signal(|| None::<u64>);
    let mut listener = use_signal(|| None::<Task>);
    let install = use_callback(move |activation: u64| {
        if let Some(task) = listener.write().take() {
            task.cancel();
        }
        registered.set(Some(activation));
        let mut eval = document::eval(&owner.dismiss_script(activation));
        owner.mount();
        listener.set(Some(spawn(async move {
            if eval
                .recv::<String>()
                .await
                .is_ok_and(|message| message == "dismissed")
                && *context.activation.peek() == activation
            {
                context.dismiss.call(());
            }
        })));
    });
    use_effect(use_reactive!(|activation| {
        let previous = *registered.peek();
        if previous.is_some_and(|previous| previous != activation) {
            install.call(activation);
        }
    }));
    use_drop(move || {
        if let Some(task) = listener.write().take() {
            task.cancel();
        }
    });
    let duration = crate::motion::Motion::CONTENT_MS;
    let animation = Surface::ENTER;
    let base = attributes!(div {
        id: owner.id(),
        popover: if context.manual { "manual" } else { "auto" },
        class: "invisible fixed left-0 top-0 z-[1000] isolate m-0 origin-top rounded-lg bg-popover shadow-[0_12px_32px_rgba(0,0,0,0.28)] [backdrop-filter:none] [-webkit-backdrop-filter:none] {animation} will-change-opacity",
        style: "--vmux-motion-ms:{duration}ms;",
        "data-vmux-dropdown-popup": "",
        "data-vmux-popup-kind": Surface::Dropdown.kind(),
        "data-vmux-popup-activation": "{activation}",
        "data-vmux-popup-align": align.as_str(),
        "data-vmux-popup-placement": placement.as_str(),
        "data-vmux-popup-match-width": if match_width { "true" } else { "false" },
        "data-vmux-popup-inset": "{horizontal_inset}",
        "data-vmux-popup-gap": "{gap}",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div {
            onmounted: move |_| {
                install.call(*context.activation.peek());
            },
            onmouseenter: move |_| context.hover_enter.call(()),
            onmouseleave: move |_| context.hover_leave.call(()),
            onclick: move |event| {
                if let Some(handler) = onclick {
                    handler.call(event);
                }
            },
            ..merged,
            {children}
        }
    }
}

#[component]
fn DropdownKeyboardNavigation(root_id: String) -> Element {
    use_future(move || {
        let root_id = root_id.clone();
        async move {
            let script = DROPDOWN_KEYBOARD_SCRIPT
                .replace("__VMUX_POPUP_RUNTIME__", &Surface::runtime())
                .replace(
                    "__VMUX_DROPDOWN_CONNECTION_WATCHER__",
                    DROPDOWN_CONNECTION_WATCHER,
                )
                .replace("__VMUX_DROPDOWN_ROOT__", &root_id);
            let mut eval = document::eval(&script);
            let _ = eval.recv::<bool>().await;
        }
    });
    rsx! {}
}

#[cfg(test)]
mod tests {
    use super::{
        DROPDOWN_CONNECTION_WATCHER, DROPDOWN_KEYBOARD_SCRIPT, DropdownOpenMode,
        DropdownPopupOwner, DropdownSelector, DropdownTrigger,
    };
    use crate::components::popup::Surface;
    use dioxus::core::{AttributeValue, Mutation};
    use dioxus::prelude::*;
    use dioxus_html::{EventData, SerializedHtmlEventConverter, SerializedMouseData};
    use std::io::Write;
    use std::process::{Command, Stdio};

    struct PlacementProbe;

    impl PlacementProbe {
        fn verify() {
            let runtime = Surface::runtime();
            let keyboard = DROPDOWN_KEYBOARD_SCRIPT
                .replace("__VMUX_POPUP_RUNTIME__", &runtime)
                .replace(
                    "__VMUX_DROPDOWN_CONNECTION_WATCHER__",
                    DROPDOWN_CONNECTION_WATCHER,
                )
                .replace("__VMUX_DROPDOWN_ROOT__", "test-dropdown-root");
            let source = serde_json::json!({
                "runtime": runtime,
                "mount": DropdownPopupOwner(0).install_script(),
                "keyboard": keyboard,
                "dismiss": DropdownPopupOwner(0).dismiss_script(0).replace("__VMUX_DROPDOWN_CONNECTION_WATCHER__", DROPDOWN_CONNECTION_WATCHER),
            });
            let mut child = Command::new("node")
                .args(["-e", include_str!("dropdown_placement_test.js")])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Node is required for the dropdown runtime test");
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.to_string().as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn dropdown_selection_is_placed_before_paint_and_preserves_fresh_reading_input() {
        PlacementProbe::verify();
    }

    #[component]
    fn HoverSelector() -> Element {
        let mut open = use_signal(|| false);
        rsx! {
            DropdownSelector {
                label: "Branches".to_string(),
                open: open(),
                open_mode: DropdownOpenMode::Hover,
                hover_delay_ms: 0,
                on_open_change: move |next| open.set(next),
                on_dismiss: move |_| open.set(false),
                span { "main" }
            }
        }
    }

    #[test]
    fn hover_opens_immediately_and_the_first_click_keeps_the_menu_open() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(HoverSelector);
        let initial = dom.rebuild_to_vec();
        let trigger = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-expanded",
                    id,
                    ..
                } => Some(*id),
                _ => None,
            })
            .unwrap();
        for (event, expanded) in [("mouseenter", true), ("click", true), ("click", false)] {
            dom.runtime().handle_event(
                event,
                Event::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                trigger,
            );
            dom.render_immediate_to_vec();
            let html = dioxus_ssr::render(&dom);
            assert!(
                html.contains(&format!("aria-expanded={expanded}"))
                    || html.contains(&format!("aria-expanded=\"{expanded}\"")),
                "after {event}: {html}"
            );
        }
    }

    #[component]
    fn HoverTrigger() -> Element {
        let mut open = use_signal(|| true);
        rsx! {
            DropdownTrigger {
                label: "Branches".to_string(),
                open: open(),
                open_mode: DropdownOpenMode::Hover,
                onclick: move |_| open.set(!open()),
            }
        }
    }

    #[test]
    fn hover_open_trigger_click_toggles_without_waiting_for_mouseleave() {
        dioxus_html::set_event_converter(Box::new(SerializedHtmlEventConverter));
        let mut dom = VirtualDom::new(HoverTrigger);
        let initial = dom.rebuild_to_vec();
        let trigger = initial
            .edits
            .iter()
            .find_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "aria-expanded",
                    id,
                    ..
                } => Some(*id),
                _ => None,
            })
            .expect("dropdown trigger must render");
        for expanded in [false, true] {
            dom.runtime().handle_event(
                "click",
                Event::new(
                    EventData::Mouse(SerializedMouseData::default()).into_any(),
                    true,
                ),
                trigger,
            );
            let changed = dom.render_immediate_to_vec();
            assert!(
                changed.edits.iter().any(|edit| match edit {
                    Mutation::SetAttribute {
                        name: "aria-expanded",
                        value,
                        ..
                    } => match value {
                        AttributeValue::Bool(value) => *value == expanded,
                        AttributeValue::Text(value) => value == &expanded.to_string(),
                        _ => false,
                    },
                    _ => false,
                }),
                "{:?}",
                changed.edits
            );
        }
    }
}
