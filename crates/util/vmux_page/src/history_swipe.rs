pub struct HistorySwipe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryDirection {
    Back,
    Forward,
}

impl HistoryDirection {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "back" => Some(Self::Back),
            "forward" => Some(Self::Forward),
            _ => None,
        }
    }
}

impl HistorySwipe {
    pub fn script(callback: &str) -> String {
        SCRIPT.replace("__VMUX_HISTORY_CALLBACK__", callback)
    }
}

const SCRIPT: &str = r#"
(() => {
  globalThis.__vmuxHistorySwipeCleanup?.();
  const threshold = 200;
  const idle = 240;
  const cooldown = 650;
  let lastEvent = -Infinity;
  let lastNavigation = -Infinity;
  let horizontal = 0;
  let vertical = 0;
  let horizontalDirection = 0;
  let blocked = false;
  let navigated = false;
  const protectsScroll = (event, direction) => {
    for (const node of event.composedPath()) {
      if (!(node instanceof Element)) continue;
      if (node.matches('input, textarea, select, [contenteditable]:not([contenteditable="false"]), [popover], [role="menu"], [role="listbox"], [role="dialog"]')) return true;
      if (node.scrollWidth <= node.clientWidth + 1) continue;
      const overflow = getComputedStyle(node).overflowX;
      if (overflow === 'hidden' || overflow === 'clip') continue;
      if (node !== document.scrollingElement && overflow !== 'auto' && overflow !== 'scroll' && overflow !== 'overlay') continue;
      if (direction < 0 && node.scrollLeft > 0.5) return true;
      if (direction > 0 && node.scrollLeft + node.clientWidth < node.scrollWidth - 0.5) return true;
    }
    return false;
  };
  const wheel = event => {
    if (!event.isTrusted) return;
    const now = performance.now();
    const fresh = now - lastEvent >= idle;
    lastEvent = now;
    if (fresh) {
      horizontal = 0;
      vertical = 0;
      horizontalDirection = 0;
      blocked = false;
      navigated = false;
    }
    if (event.deltaMode !== 0 || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey || !event.cancelable) {
      blocked = true;
      return;
    }
    if (blocked) return;
    const x = event.deltaX;
    const y = event.deltaY;
    if (!Number.isFinite(x) || !Number.isFinite(y)) {
      blocked = true;
      return;
    }
    if (x === 0 && y === 0) return;
    if (navigated) {
      if (Math.abs(x) > Math.abs(y) && x !== 0) event.preventDefault();
      return;
    }
    if (x !== 0 && horizontalDirection && horizontalDirection !== Math.sign(x)) {
      horizontal = 0;
      vertical = 0;
    }
    if (x !== 0) horizontalDirection = Math.sign(x);
    horizontal += x;
    vertical += Math.abs(y);
    if (Math.abs(horizontal) + vertical < 12) return;
    if (vertical > Math.abs(horizontal) * 0.65 || protectsScroll(event, Math.sign(horizontal))) {
      blocked = true;
      return;
    }
    event.preventDefault();
    if (Math.abs(horizontal) < threshold || now - lastNavigation < cooldown) return;
    const requested = horizontal < 0 ? 'back' : 'forward';
    navigated = true;
    lastNavigation = now;
    const direction = requested;
    __VMUX_HISTORY_CALLBACK__
  };
  const reset = () => {
    lastEvent = -Infinity;
    blocked = true;
    horizontal = 0;
    vertical = 0;
  };
  document.addEventListener('wheel', wheel, { capture: true, passive: false });
  window.addEventListener('blur', reset);
  globalThis.__vmuxHistorySwipeCleanup = () => {
    document.removeEventListener('wheel', wheel, true);
    window.removeEventListener('blur', reset);
  };
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    struct GestureProbe;

    impl GestureProbe {
        fn verify() {
            let mut child = Command::new("node")
                .args(["-e", include_str!("history_swipe_test.js")])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Node is required for the gesture runtime test");
            child
                .stdin
                .take()
                .unwrap()
                .write_all(
                    HistorySwipe::script("globalThis.navigations.push(direction);").as_bytes(),
                )
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
    fn trackpad_gestures_ignore_startup_noise_and_respect_horizontal_scroll_boundaries() {
        GestureProbe::verify();
    }
}
