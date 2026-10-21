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

#[component]
pub fn Dropdown(
    open: bool,
    #[props(default)] class: String,
    #[props(default)] keyboard_navigation: bool,
    on_dismiss: EventHandler<()>,
    children: Element,
) -> Element {
    let root_class = cn(["relative", class.as_str()]);
    rsx! {
        div {
            class: root_class,
            "data-vmux-dropdown-root": "",
            "data-vmux-dropdown-keyboard": if keyboard_navigation { "true" } else { "false" },
            onkeydown: move |event: KeyboardEvent| {
                if open && event.key() == Key::Escape {
                    event.prevent_default();
                    event.stop_propagation();
                    on_dismiss.call(());
                    return;
                }
            },
            if open {
                DropdownBlurDismiss { on_dismiss }
                if keyboard_navigation {
                    DropdownKeyboardNavigation {}
                }
                div {
                    class: "fixed inset-0 z-20",
                    aria_hidden: "true",
                    onmousedown: move |event: MouseEvent| {
                        event.prevent_default();
                        event.stop_propagation();
                        on_dismiss.call(());
                    },
                }
            }
            {children}
        }
    }
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
