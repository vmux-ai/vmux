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
        SCRIPT
            .replace("__VMUX_HISTORY_CALLBACK__", callback)
            .replace(
                "__VMUX_HISTORY_FEEDBACK_MS__",
                &vmux_ui::motion::Motion::FEEDBACK_MS.to_string(),
            )
    }
}

const SCRIPT: &str = r#"
(() => {
  globalThis.__vmuxHistorySwipeCleanup?.();
  const threshold = 200;
  const idle = 240;
  const cooldown = 650;
  const duration = __VMUX_HISTORY_FEEDBACK_MS__;
  class HistoryFeedback {
    node = null;
    ring = null;
    timer = null;
    create(direction) {
      const node = document.createElement('div');
      node.setAttribute('data-vmux-history-indicator', '');
      node.setAttribute('data-direction', direction);
      node.setAttribute('aria-hidden', 'true');
      node.inert = true;
      const theme = document.documentElement?.dataset.theme;
      const dark = theme === 'dark' || (theme !== 'light' && globalThis.matchMedia?.('(prefers-color-scheme: dark)').matches);
      const reduced = globalThis.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
      Object.assign(node.style, {
        position: 'fixed', top: '50%', margin: '0', padding: '0', display: 'block', left: direction === 'back' ? '8px' : 'auto',
        right: direction === 'forward' ? '8px' : 'auto', width: '44px', height: '44px',
        borderRadius: '50%', backgroundColor: dark ? '#303134' : '#f1f3f4',
        color: dark ? '#e8eaed' : '#202124', border: '1px solid rgba(128,128,128,0.25)',
        boxShadow: '0 2px 8px rgba(0,0,0,0.18)', pointerEvents: 'none',
        userSelect: 'none', zIndex: '2147483647', opacity: '0',
        transition: reduced ? 'none' : `transform ${duration}ms ease-out, opacity ${duration}ms ease-out`,
      });
      const shadow = node.attachShadow({ mode: 'closed' });
      shadow.innerHTML = `<svg class="track" viewBox="0 0 44 44" aria-hidden="true"><circle class="progress" cx="22" cy="22" r="20" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-dasharray="125.664" /></svg><svg class="arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M19 12H5m7-7-7 7 7 7" /></svg>`;
      Object.assign(shadow.querySelector('.track').style, { position: 'absolute', inset: '0', width: '44px', height: '44px', transform: 'rotate(-90deg)', opacity: '0.4' });
      Object.assign(shadow.querySelector('.arrow').style, { position: 'absolute', left: '10px', top: '10px', width: '24px', height: '24px', transform: direction === 'forward' ? 'scaleX(-1)' : 'none' });
      document.body.appendChild(node);
      this.node = node;
      this.ring = shadow.querySelector('.progress');
    }
    update(direction, progress) {
      if (!document.body) return;
      if (this.node?.getAttribute('data-direction') !== direction) this.remove();
      if (!this.node) this.create(direction);
      clearTimeout(this.timer);
      const value = Math.min(1, Math.max(0, progress));
      this.node.setAttribute('data-state', 'preview');
      this.node.setAttribute('data-progress', String(value));
      this.node.style.opacity = String(0.35 + value * 0.65);
      this.node.style.transform = `translate(${(direction === 'back' ? -18 : 18) * (1 - value)}px, -50%)`;
      this.ring.setAttribute('stroke-dashoffset', String(125.664 * (1 - value)));
      this.timer = setTimeout(() => this.hide(), idle);
    }
    finish() {
      if (!this.node) return;
      clearTimeout(this.timer);
      this.node.setAttribute('data-state', 'committed');
      this.timer = setTimeout(() => this.hide(), duration);
    }
    hide() {
      if (!this.node) return;
      clearTimeout(this.timer);
      this.node.setAttribute('data-state', 'closing');
      this.node.style.opacity = '0';
      const offset = this.node.getAttribute('data-direction') === 'back' ? -18 : 18;
      this.node.style.transform = `translate(${offset}px, -50%)`;
      this.timer = setTimeout(() => this.remove(), duration);
    }
    remove() {
      clearTimeout(this.timer);
      this.timer = null;
      this.node?.remove();
      this.node = null;
      this.ring = null;
    }
  }
  const feedback = new HistoryFeedback();
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
      feedback.remove();
      horizontal = 0;
      vertical = 0;
      horizontalDirection = 0;
      blocked = false;
      navigated = false;
    }
    if (event.deltaMode !== 0 || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey || !event.cancelable) {
      blocked = true;
      feedback.hide();
      return;
    }
    if (blocked) return;
    const x = event.deltaX;
    const y = event.deltaY;
    if (!Number.isFinite(x) || !Number.isFinite(y)) {
      blocked = true;
      feedback.hide();
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
      feedback.hide();
      return;
    }
    event.preventDefault();
    feedback.update(horizontal < 0 ? 'back' : 'forward', Math.abs(horizontal) / threshold);
    if (Math.abs(horizontal) < threshold || now - lastNavigation < cooldown) return;
    const requested = horizontal < 0 ? 'back' : 'forward';
    navigated = true;
    lastNavigation = now;
    feedback.finish();
    const direction = requested;
    __VMUX_HISTORY_CALLBACK__
  };
  const reset = () => {
    lastEvent = -Infinity;
    blocked = true;
    horizontal = 0;
    vertical = 0;
    feedback.remove();
  };
  document.addEventListener('wheel', wheel, { capture: true, passive: false });
  window.addEventListener('blur', reset);
  globalThis.__vmuxHistorySwipeCleanup = () => {
    feedback.remove();
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
