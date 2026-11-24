use crate::motion::Motion;
use dioxus::core::Task;
use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub enum Surface {
    Context,
    Dropdown,
    Select,
}

impl Surface {
    pub const ENTER: &str = "animate-[vmux-popup-in_var(--vmux-motion-ms)_ease-out_forwards] motion-reduce:animate-none";
    pub const SELECT: &str = "data-[state=closed]:pointer-events-none data-[state=closed]:opacity-0 data-[state=open]:pointer-events-auto data-[state=open]:opacity-100 data-[state=open]:animate-[vmux-popup-in_var(--vmux-motion-ms)_ease-out_forwards] motion-reduce:animate-none";

    const RUNTIME: &str = include_str!("popup_presentation.js");

    pub fn kind(self) -> &'static str {
        match self {
            Self::Context => "context",
            Self::Dropdown => "dropdown",
            Self::Select => "select",
        }
    }

    pub fn style() -> String {
        format!("--vmux-motion-ms:{}ms;", Motion::CONTENT_MS)
    }

    pub fn runtime() -> String {
        Self::RUNTIME.replace("__VMUX_POPUP_EXIT_MS__", &Motion::FEEDBACK_MS.to_string())
    }
}

#[derive(Clone, Copy)]
pub struct Dismissal(u64);

impl Dismissal {
    const RUNTIME: &str = include_str!("popup_dismissal.js");

    pub fn use_open(
        open: ReadSignal<Option<bool>>,
        default_open: bool,
        on_open_change: Callback<bool>,
    ) -> (Memo<bool>, Callback<bool>) {
        let (open, set_open) =
            dioxus_primitives::use_controlled(open, default_open, on_open_change);
        Self::use_listener(open(), EventHandler::new(move |()| set_open.call(false)));
        (open, set_open)
    }

    pub fn use_listener(open: bool, on_dismiss: EventHandler<()>) {
        let owner = use_hook(Self::next);
        let mut task = use_signal(|| None::<Task>);
        let dismiss = use_callback(move |()| on_dismiss.call(()));
        use_effect(use_reactive!(|open| {
            if let Some(task) = task.write().take() {
                task.cancel();
            }
            crate::transport::Host::run_script(owner.remove_script());
            if !open {
                return;
            }
            task.set(Some(spawn(async move {
                let mut eval = document::eval(&owner.listen_script());
                while let Ok(dismissed) = eval.recv::<bool>().await {
                    if dismissed {
                        dismiss.call(());
                    }
                }
            })));
        }));
        use_drop(move || {
            if let Some(task) = task.write().take() {
                task.cancel();
            }
            crate::transport::Host::run_script(owner.remove_script());
        });
    }

    fn next() -> Self {
        Self(dioxus::core::current_scope_id().0 as u64)
    }

    fn listen_script(self) -> String {
        format!(
            "{}\nglobalThis.__vmuxPopupDismissal.subscribe({}, () => dioxus.send(true));",
            Self::RUNTIME,
            self.0
        )
    }

    fn remove_script(self) -> String {
        format!("globalThis.__vmuxPopupDismissal?.remove({});", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    struct PopupProbe;

    impl PopupProbe {
        fn verify(script: &str, source: &str) {
            let mut child = Command::new("node")
                .args(["-e", script])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Node is required for the popup runtime test");
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        fn verify_state() {
            Self::verify(include_str!("popup_state_test.js"), &Surface::runtime());
        }

        fn verify_dismissal() {
            Self::verify(include_str!("popup_dismissal_test.js"), Dismissal::RUNTIME);
        }
    }

    #[test]
    fn native_popup_close_updates_the_trigger_before_the_host_round_trip() {
        PopupProbe::verify_state();
    }

    #[test]
    fn popups_share_deactivation_and_preserve_focus_moves_inside_the_page() {
        PopupProbe::verify_dismissal();
    }
}
