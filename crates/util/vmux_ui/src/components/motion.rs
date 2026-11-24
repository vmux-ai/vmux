use dioxus::prelude::*;

#[component]
pub fn ContentTransition(#[props(default)] class: String, children: Element) -> Element {
    let duration = crate::motion::Motion::CONTENT_MS;
    rsx! {
        div { class: "vmux-content-enter {class}", style: "--vmux-content-motion-ms:{duration}ms;", {children} }
    }
}

#[component]
pub fn Collapse(leaving: bool, #[props(default)] class: String, children: Element) -> Element {
    rsx! {
        Collapsible { expanded: !leaving, class, {children} }
    }
}

#[component]
pub fn Collapsible(
    expanded: bool,
    #[props(default)] class: String,
    #[props(default)] content_class: String,
    children: Element,
) -> Element {
    let duration = crate::motion::Motion::LAYOUT_MS;
    let state = if expanded {
        "grid-rows-[1fr] opacity-100"
    } else {
        "grid-rows-[0fr] opacity-0"
    };
    rsx! {
        div {
            style: "--vmux-motion-ms:{duration}ms;",
            class: "grid {state} transition-[grid-template-rows,opacity] duration-[var(--vmux-motion-ms)] ease-out motion-reduce:transition-none {class}",
            div { class: "min-h-0 overflow-hidden {content_class}", {children} }
        }
    }
}

pub struct Disclosure;

impl Disclosure {
    pub fn install() {
        crate::transport::Host::run_script(Self::runtime());
    }

    pub fn runtime() -> String {
        Self::RUNTIME.replace(
            "__VMUX_DISCLOSURE_MS__",
            &crate::motion::Motion::CONTENT_MS.to_string(),
        )
    }
    const RUNTIME: &str = include_str!("disclosure.js");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    struct Probe;

    impl Probe {
        fn run() {
            let mut child = Command::new("node")
                .args(["-e", include_str!("motion_disclosure_test.js")])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Node is required for disclosure behavior tests");
            child
                .stdin
                .take()
                .unwrap()
                .write_all(Disclosure::runtime().as_bytes())
                .unwrap();
            let result = child.wait_with_output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    #[test]
    fn disclosure_animates_both_directions_handles_reversal_and_respects_reduced_motion() {
        Probe::run();
    }
}
