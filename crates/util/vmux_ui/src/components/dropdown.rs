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

#[component]
pub fn Dropdown(
    open: bool,
    #[props(default)] class: String,
    on_dismiss: EventHandler<()>,
    children: Element,
) -> Element {
    let root_class = cn(["relative", class.as_str()]);
    rsx! {
        div {
            class: root_class,
            onkeydown: move |event: KeyboardEvent| {
                if open && event.key() == Key::Escape {
                    event.prevent_default();
                    event.stop_propagation();
                    on_dismiss.call(());
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
