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
  const protectsScroll = event => {
    for (const node of event.composedPath()) {
      if (!(node instanceof Element)) continue;
      if (node.matches('input, textarea, select, [contenteditable]:not([contenteditable="false"]), [popover], [role="menu"], [role="listbox"], [role="dialog"]')) return true;
      if (node.scrollWidth <= node.clientWidth + 1) continue;
      const overflow = getComputedStyle(node).overflowX;
      if (node === document.scrollingElement || overflow === 'auto' || overflow === 'scroll' || overflow === 'overlay') return true;
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
      blocked = protectsScroll(event);
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
    if (navigated) {
      if (Math.abs(x) > Math.abs(y) && x !== 0) event.preventDefault();
      return;
    }
    if (Math.abs(y) > Math.abs(x) * 0.65 || x === 0) {
      blocked = true;
      return;
    }
    if (horizontalDirection && horizontalDirection !== Math.sign(x)) {
      horizontal = 0;
      vertical = 0;
    }
    horizontalDirection = Math.sign(x);
    horizontal += x;
    vertical += Math.abs(y);
    if (vertical > Math.abs(horizontal) * 0.65) {
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
