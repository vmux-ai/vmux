use dioxus::core::current_scope_id;
use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;

use crate::cn::cn;

const DROPDOWN_KEYBOARD_SCRIPT: &str = r#"
globalThis.__vmuxDropdownKeyboardCleanup?.();
let active = true;
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
  option.scrollIntoView({ block: 'nearest' });
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
  document.removeEventListener('keydown', keydown, true);
  document.removeEventListener('input', input, true);
  if (globalThis.__vmuxDropdownKeyboardCleanup === cleanup) {
    delete globalThis.__vmuxDropdownKeyboardCleanup;
  }
  dioxus.send(true);
};
globalThis.__vmuxDropdownKeyboardCleanup = cleanup;
document.addEventListener('keydown', keydown, true);
document.addEventListener('input', input, true);
"#;

const DROPDOWN_DISMISS_SCRIPT: &str = r#"
const owner = __VMUX_DROPDOWN_OWNER__;
const entries = globalThis.__vmuxDropdownDismissEntries ??= new Map();
for (const [other, entry] of [...entries]) {
  if (other !== owner) entry.dismiss();
}
entries.get(owner)?.cleanup();
let active = true;
const cleanup = () => {
  if (!active) return;
  active = false;
  document.removeEventListener('pointerdown', outside, true);
  window.removeEventListener('pointerdown', outside, true);
  window.removeEventListener('blur', dismiss, true);
  if (entries.get(owner)?.cleanup === cleanup) {
    entries.delete(owner);
  }
};
const dismiss = () => {
  if (!active) return;
  cleanup();
  dioxus.send(true);
};
const outside = event => {
  const root = document.querySelector(`[data-vmux-dropdown-owner="${owner}"]`);
  if (root && event.composedPath().includes(root)) return;
  dismiss();
};
entries.set(owner, { cleanup, dismiss });
document.addEventListener('pointerdown', outside, true);
window.addEventListener('pointerdown', outside, true);
window.addEventListener('blur', dismiss, true);
"#;

const DROPDOWN_POPUP_SCRIPT: &str = r#"
const id = '__VMUX_DROPDOWN_POPUP__';
const entries = globalThis.__vmuxDropdownPopupEntries ??= new Map();
entries.get(id)?.cleanup();
const popup = document.getElementById(id);
const root = popup?.closest('[data-vmux-dropdown-root]');
if (!popup || !root) {
  dioxus.send(false);
} else {
  let active = true;
  let frame = 0;
  const align = '__VMUX_DROPDOWN_ALIGN__';
  const placement = '__VMUX_DROPDOWN_PLACEMENT__';
  const matchWidth = __VMUX_DROPDOWN_MATCH_WIDTH__;
  const inset = __VMUX_DROPDOWN_INSET__;
  const gap = __VMUX_DROPDOWN_GAP__;
  const margin = 6;
  const baseWidth = popup.style.width;
  const baseMinWidth = popup.style.minWidth;
  const baseMaxHeight = popup.style.maxHeight;
  const baseOverflowY = popup.style.overflowY;
  const place = () => {
    frame = 0;
    if (!active || !popup.isConnected || !root.isConnected) return;
    if (!popup.matches(':popover-open')) {
      try { popup.showPopover(); } catch (error) { return; }
    }
    const anchor = root.getBoundingClientRect();
    popup.style.width = baseWidth;
    popup.style.minWidth = baseMinWidth;
    popup.style.maxHeight = baseMaxHeight;
    popup.style.overflowY = baseOverflowY;
    if (matchWidth) popup.style.width = `${Math.max(0, anchor.width - inset * 2)}px`;
    const availableWidth = Math.max(0, window.innerWidth - margin * 2);
    const availableHeight = Math.max(0, window.innerHeight - margin * 2);
    popup.style.maxWidth = `${availableWidth}px`;
    let bounds = popup.getBoundingClientRect();
    if (bounds.width > availableWidth) {
      popup.style.minWidth = '0px';
      popup.style.width = `${availableWidth}px`;
      bounds = popup.getBoundingClientRect();
    }
    if (bounds.height > availableHeight) {
      popup.style.maxHeight = `${availableHeight}px`;
      popup.style.overflowY = 'auto';
      bounds = popup.getBoundingClientRect();
    }
    const width = bounds.width;
    const height = bounds.height;
    let x = align === 'end'
      ? anchor.right - inset - width
      : anchor.left + inset;
    let y = placement === 'top'
      ? anchor.top - gap - height
      : anchor.bottom + gap;
    const below = anchor.bottom + gap;
    const above = anchor.top - gap - height;
    if (placement === 'bottom' && y + height > window.innerHeight - margin && above >= margin) {
      y = above;
    } else if (placement === 'top' && y < margin && below + height <= window.innerHeight - margin) {
      y = below;
    }
    x = Math.min(Math.max(margin, x), Math.max(margin, window.innerWidth - width - margin));
    y = Math.min(Math.max(margin, y), Math.max(margin, window.innerHeight - height - margin));
    popup.style.left = `${x}px`;
    popup.style.top = `${y}px`;
    popup.style.right = 'auto';
    popup.style.bottom = 'auto';
    popup.style.visibility = 'visible';
  };
  const schedule = () => {
    if (!active || frame) return;
    frame = requestAnimationFrame(place);
  };
  const observer = new ResizeObserver(schedule);
  observer.observe(root);
  observer.observe(popup);
  window.addEventListener('resize', schedule, true);
  window.addEventListener('scroll', schedule, true);
  const cleanup = () => {
    if (!active) return;
    active = false;
    if (frame) cancelAnimationFrame(frame);
    observer.disconnect();
    window.removeEventListener('resize', schedule, true);
    window.removeEventListener('scroll', schedule, true);
    if (popup.matches(':popover-open')) {
      try { popup.hidePopover(); } catch (error) {}
    }
    if (entries.get(id)?.cleanup === cleanup) entries.delete(id);
    dioxus.send(true);
  };
  entries.set(id, { cleanup });
  schedule();
}
"#;

#[derive(Clone, Copy, PartialEq, Eq)]
struct DropdownOutsideOwner(usize);

impl DropdownOutsideOwner {
    fn install_script(self) -> String {
        DROPDOWN_DISMISS_SCRIPT.replace("__VMUX_DROPDOWN_OWNER__", &self.0.to_string())
    }

    fn cleanup_script(self) -> String {
        format!(
            "globalThis.__vmuxDropdownDismissEntries?.get({})?.cleanup();",
            self.0
        )
    }
}

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
}

impl DropdownPlacement {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Top => "top",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct DropdownPopupOwner(usize);

impl DropdownPopupOwner {
    fn id(self) -> String {
        format!("vmux-dropdown-popup-{}", self.0)
    }

    fn install_script(
        self,
        align: DropdownAlign,
        placement: DropdownPlacement,
        match_width: bool,
        horizontal_inset: f64,
        gap: f64,
    ) -> String {
        DROPDOWN_POPUP_SCRIPT
            .replace("__VMUX_DROPDOWN_POPUP__", &self.id())
            .replace("__VMUX_DROPDOWN_ALIGN__", align.as_str())
            .replace("__VMUX_DROPDOWN_PLACEMENT__", placement.as_str())
            .replace(
                "__VMUX_DROPDOWN_MATCH_WIDTH__",
                if match_width { "true" } else { "false" },
            )
            .replace("__VMUX_DROPDOWN_INSET__", &horizontal_inset.to_string())
            .replace("__VMUX_DROPDOWN_GAP__", &gap.to_string())
    }

    fn cleanup_script(self) -> String {
        format!(
            "globalThis.__vmuxDropdownPopupEntries?.get('{}')?.cleanup();",
            self.id()
        )
    }
}

#[component]
pub fn Dropdown(
    open: bool,
    #[props(default)] class: String,
    #[props(default)] keyboard_navigation: bool,
    on_dismiss: EventHandler<()>,
    children: Element,
) -> Element {
    let outside_owner = DropdownOutsideOwner(current_scope_id().0);
    let layer = if open { "z-[100]" } else { "z-auto" };
    let root_class = cn([
        "relative [&_button]:transition-[background-color,color,opacity,transform] [&_button]:duration-150 [&_button]:ease-out [&_svg]:transition-transform [&_svg]:duration-150 motion-reduce:[&_button]:transition-none motion-reduce:[&_svg]:transition-none",
        layer,
        class.as_str(),
    ]);
    rsx! {
        div {
            class: root_class,
            "data-vmux-dropdown-root": "",
            "data-vmux-dropdown-open": if open { "true" } else { "false" },
            "data-vmux-dropdown-owner": outside_owner.0,
            "data-vmux-dropdown-keyboard": if keyboard_navigation { "true" } else { "false" },
            onkeydown: move |event: KeyboardEvent| {
                if open && event.key() == Key::Escape {
                    event.prevent_default();
                    event.stop_propagation();
                    on_dismiss.call(());
                }
            },
            if open {
                DropdownDismiss { owner: outside_owner, on_dismiss }
                if keyboard_navigation {
                    DropdownKeyboardNavigation {}
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
    let base = attributes!(div {
        id: owner.id(),
        popover: "manual",
        class: "invisible fixed left-0 top-0 z-[1000] m-0 origin-top rounded-lg bg-popover shadow-[0_16px_48px_rgba(0,0,0,0.32)] animate-[dx-fade-zoom-in_120ms_ease-out_forwards] will-change-[transform,opacity] motion-reduce:animate-none",
        "data-vmux-dropdown-popup": "",
    });
    let merged = merge_attributes(vec![base, attributes]);
    rsx! {
        div {
            onclick: move |event| {
                if let Some(handler) = onclick {
                    handler.call(event);
                }
            },
            ..merged,
            {children}
            DropdownPopupMount {
                owner,
                align,
                placement,
                match_width,
                horizontal_inset,
                gap,
            }
        }
    }
}

#[component]
fn DropdownPopupMount(
    owner: DropdownPopupOwner,
    align: DropdownAlign,
    placement: DropdownPlacement,
    match_width: bool,
    horizontal_inset: f64,
    gap: f64,
) -> Element {
    use_future(move || async move {
        let script = owner.install_script(align, placement, match_width, horizontal_inset, gap);
        let mut eval = document::eval(&script);
        let _ = eval.recv::<bool>().await;
    });
    use_drop(move || {
        let _ = document::eval(&owner.cleanup_script());
    });
    rsx! {}
}

#[component]
fn DropdownDismiss(owner: DropdownOutsideOwner, on_dismiss: EventHandler<()>) -> Element {
    use_future(move || async move {
        let script = owner.install_script();
        let mut eval = document::eval(&script);
        if eval.recv::<bool>().await.is_ok_and(|dismiss| dismiss) {
            on_dismiss.call(());
        }
    });
    use_drop(move || {
        let _ = document::eval(&owner.cleanup_script());
    });
    rsx! {}
}

#[component]
fn DropdownKeyboardNavigation() -> Element {
    use_future(move || async move {
        let mut eval = document::eval(DROPDOWN_KEYBOARD_SCRIPT);
        let _ = eval.recv::<bool>().await;
    });
    use_drop(move || {
        let _ = document::eval("globalThis.__vmuxDropdownKeyboardCleanup?.();");
    });
    rsx! {}
}
