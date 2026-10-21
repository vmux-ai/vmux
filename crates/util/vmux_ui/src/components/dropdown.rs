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

const DROPDOWN_FOCUS_NEXT_SCRIPT: &str = r#"
const root = document.activeElement?.closest('[data-vmux-dropdown-root]');
const options = root ? [...root.querySelectorAll('[data-vmux-dropdown-option]:not(:disabled)')] : [];
if (options.length) {
  const current = options.indexOf(document.activeElement);
  options[current < 0 ? 0 : (current + 1) % options.length].focus();
}
"#;

const DROPDOWN_FOCUS_PREVIOUS_SCRIPT: &str = r#"
const root = document.activeElement?.closest('[data-vmux-dropdown-root]');
const options = root ? [...root.querySelectorAll('[data-vmux-dropdown-option]:not(:disabled)')] : [];
if (options.length) {
  const current = options.indexOf(document.activeElement);
  options[current < 0 ? options.length - 1 : (current - 1 + options.length) % options.length].focus();
}
"#;

struct DropdownKeyboard;

impl DropdownKeyboard {
    fn direction(event: &KeyboardEvent) -> Option<bool> {
        match event.key() {
            Key::ArrowDown => Some(true),
            Key::ArrowUp => Some(false),
            Key::Character(value)
                if event.modifiers().ctrl() && value.eq_ignore_ascii_case("n") =>
            {
                Some(true)
            }
            Key::Character(value)
                if event.modifiers().ctrl() && value.eq_ignore_ascii_case("p") =>
            {
                Some(false)
            }
            _ => None,
        }
    }

    fn focus(forward: bool) {
        let script = if forward {
            DROPDOWN_FOCUS_NEXT_SCRIPT
        } else {
            DROPDOWN_FOCUS_PREVIOUS_SCRIPT
        };
        let _ = document::eval(script);
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
    let root_class = cn(["relative", class.as_str()]);
    rsx! {
        div {
            class: root_class,
            "data-vmux-dropdown-root": "",
            onkeydown: move |event: KeyboardEvent| {
                if open && event.key() == Key::Escape {
                    event.prevent_default();
                    event.stop_propagation();
                    on_dismiss.call(());
                    return;
                }
                if open && keyboard_navigation
                    && let Some(forward) = DropdownKeyboard::direction(&event)
                {
                    event.prevent_default();
                    event.stop_propagation();
                    DropdownKeyboard::focus(forward);
                }
            },
            if open {
                DropdownBlurDismiss { on_dismiss }
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
