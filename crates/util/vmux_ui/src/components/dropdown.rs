use dioxus::core::current_scope_id;
use dioxus::prelude::*;

use crate::cn::cn;

const DROPDOWN_BLUR_SCRIPT: &str = r#"
globalThis.__vmuxDropdownBlurCleanup?.();
let active = true;
const cleanup = () => {
  if (!active) return;
  active = false;
  window.removeEventListener("blur", dismiss);
  if (globalThis.__vmuxDropdownBlurCleanup === cleanup) {
    delete globalThis.__vmuxDropdownBlurCleanup;
  }
};
const dismiss = () => {
  cleanup();
  dioxus.send(true);
};
globalThis.__vmuxDropdownBlurCleanup = cleanup;
window.addEventListener("blur", dismiss);
"#;

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

const DROPDOWN_OUTSIDE_SCRIPT: &str = r#"
const owner = __VMUX_DROPDOWN_OWNER__;
const cleanups = globalThis.__vmuxDropdownOutsideCleanups ??= new Map();
cleanups.get(owner)?.();
let active = true;
const cleanup = () => {
  if (!active) return;
  active = false;
  document.removeEventListener('mousedown', outside, true);
  if (cleanups.get(owner) === cleanup) {
    cleanups.delete(owner);
  }
};
const outside = event => {
  const root = document.querySelector(`[data-vmux-dropdown-owner="${owner}"]`);
  if (!root || root.contains(event.target)) return;
  cleanup();
  dioxus.send(true);
};
cleanups.set(owner, cleanup);
document.addEventListener('mousedown', outside, true);
"#;

#[derive(Clone, Copy, PartialEq, Eq)]
struct DropdownOutsideOwner(usize);

impl DropdownOutsideOwner {
    fn install_script(self) -> String {
        DROPDOWN_OUTSIDE_SCRIPT.replace("__VMUX_DROPDOWN_OWNER__", &self.0.to_string())
    }

    fn cleanup_script(self) -> String {
        format!(
            "globalThis.__vmuxDropdownOutsideCleanups?.get({})?.();",
            self.0
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
    let root_class = cn(["relative", layer, class.as_str()]);
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
                DropdownBlurDismiss { on_dismiss }
                DropdownOutsideDismiss { owner: outside_owner, on_dismiss }
                if keyboard_navigation {
                    DropdownKeyboardNavigation {}
                }
            }
            {children}
        }
    }
}

#[component]
fn DropdownOutsideDismiss(owner: DropdownOutsideOwner, on_dismiss: EventHandler<()>) -> Element {
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
fn DropdownBlurDismiss(on_dismiss: EventHandler<()>) -> Element {
    use_future(move || async move {
        let mut eval = document::eval(DROPDOWN_BLUR_SCRIPT);
        if eval.recv::<bool>().await.is_ok_and(|dismiss| dismiss) {
            on_dismiss.call(());
        }
    });
    use_drop(move || {
        let _ = document::eval("globalThis.__vmuxDropdownBlurCleanup?.();");
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
