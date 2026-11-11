use dioxus::prelude::*;

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
        DISCLOSURE_RUNTIME.replace(
            "__VMUX_DISCLOSURE_MS__",
            &crate::motion::Motion::CONTENT_MS.to_string(),
        )
    }
}

const DISCLOSURE_RUNTIME: &str = r#"
(() => {
  if (window.__vmuxDisclosures) return;
  const states = new WeakMap();
  window.__vmuxDisclosures = { states };
  document.addEventListener('click', event => {
    if (event.defaultPrevented || event.button !== 0 || !(event.target instanceof Element)) return;
    const summary = event.target.closest('summary');
    const details = summary?.parentElement;
    if (!details?.matches('details.disclosure')) return;
    if (event.target.closest('a, button, input, textarea, select')) return;
    event.preventDefault();
    const previous = states.get(details);
    const expanded = !(previous?.expanded ?? details.open);
    const from = details.getBoundingClientRect().height;
    const opacities = new Map([...details.children].map(child => [child, Number(getComputedStyle(child).opacity)]));
    const original = previous?.original ?? { height: details.style.height, overflow: details.style.overflow };
    details.style.height = from + 'px';
    previous?.animation.cancel();
    for (const animation of previous?.fades ?? []) animation.cancel();
    details.dataset.vmuxDisclosureManaged = 'true';
    details.dataset.vmuxDisclosureOpen = String(expanded);
    details.style.height = original.height;
    details.open = expanded;
    const to = details.getBoundingClientRect().height;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches || typeof details.animate !== 'function') {
      Object.assign(details.style, original);
      states.delete(details);
      return;
    }
    details.open = true;
    details.style.height = from + 'px';
    details.style.overflow = 'hidden';
    const timing = { duration: __VMUX_DISCLOSURE_MS__, easing: 'ease-out', fill: 'forwards' };
    const animation = details.animate([{ height: from + 'px' }, { height: to + 'px' }], timing);
    const fades = [];
    for (const child of details.children) {
      if (child === summary) continue;
      fades.push(child.animate([{ opacity: previous ? opacities.get(child) : expanded ? 0 : 1 }, { opacity: expanded ? 1 : 0 }], timing));
    }
    const state = { animation, fades, expanded, original };
    states.set(details, state);
    animation.onfinish = () => {
      if (states.get(details) !== state) return;
      details.open = expanded;
      Object.assign(details.style, original);
      states.delete(details);
      animation.cancel();
      for (const fade of fades) fade.cancel();
    };
  });
})();
"#;

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
