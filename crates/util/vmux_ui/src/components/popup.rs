use crate::motion::Motion;
use dioxus::core::Task;
use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy)]
pub enum Surface {
    Context,
    Dropdown,
    Select,
}

impl Surface {
    pub const ENTER: &str = "animate-[vmux-popup-in_var(--vmux-motion-ms)_ease-out_forwards] motion-reduce:animate-none";
    pub const SELECT: &str = "data-[state=closed]:pointer-events-none data-[state=closed]:opacity-0 data-[state=open]:pointer-events-auto data-[state=open]:opacity-100 data-[state=open]:animate-[vmux-popup-in_var(--vmux-motion-ms)_ease-out_forwards] motion-reduce:animate-none";

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
        RUNTIME.replace("__VMUX_POPUP_EXIT_MS__", &Motion::FEEDBACK_MS.to_string())
    }
}

const RUNTIME: &str = r#"
globalThis.__vmuxPopupPresentation ??= (() => {
  class Presentation {
    known = new WeakSet();
    closing = new WeakSet();
    styles = new WeakMap();
    activations = new WeakMap();
    ghosts = new Set();
    visible(element) {
      if (!element.isConnected) return false;
      if (element.hasAttribute('popover')) return element.matches(':popover-open');
      return element.dataset.state !== 'closed' && element.getBoundingClientRect().height > 0;
    }
    snapshot(element) {
      const rendered = element.dataset.vmuxPopupKind === 'context' ? element.querySelector('[role="menu"]') ?? element : element;
      const style = getComputedStyle(rendered);
      const scrolling = [];
      const nodes = [rendered, ...rendered.querySelectorAll('*')];
      for (let index = 0; index < nodes.length; index++) {
        const node = nodes[index];
        if (node.scrollTop || node.scrollLeft) scrolling.push({ index, top: node.scrollTop, left: node.scrollLeft });
      }
      return { element, rendered, rect: rendered.getBoundingClientRect(), opacity: style.opacity, scrolling, typography: { color: style.color, fontFamily: style.fontFamily, fontSize: style.fontSize, fontWeight: style.fontWeight, lineHeight: style.lineHeight, letterSpacing: style.letterSpacing } };
    }
    capture() {
      const captures = [];
      for (const element of document.querySelectorAll('[data-vmux-popup-kind]')) {
        if (this.visible(element)) captures.push(this.snapshot(element));
      }
      return captures;
    }
    retire(capture) {
      const { element, rendered, rect, opacity, scrolling, typography } = capture;
      if (rect.width <= 0 || rect.height <= 0) return;
      if (this.closing.has(element)) return;
      this.closing.add(element);
      if (window.matchMedia('(prefers-reduced-motion: reduce)').matches || typeof element.animate !== 'function') return;
      const ghost = rendered.cloneNode(true);
      for (const node of [ghost, ...ghost.querySelectorAll('[id], [data-dioxus-id], [data-vmux-popup-kind]')]) {
        node.removeAttribute('id');
        node.removeAttribute('data-dioxus-id');
        node.removeAttribute('data-vmux-popup-kind');
      }
      ghost.inert = true;
      ghost.setAttribute('aria-hidden', 'true');
      ghost.setAttribute('popover', 'manual');
      Object.assign(ghost.style, typography, { position: 'fixed', left: rect.left + 'px', top: rect.top + 'px', right: 'auto', bottom: 'auto', width: rect.width + 'px', height: rect.height + 'px', maxWidth: 'none', maxHeight: 'none', margin: '0', pointerEvents: 'none', animation: 'none', opacity, visibility: 'visible' });
      document.body.appendChild(ghost);
      let animation;
      const cleanup = () => { ghost.remove(); this.ghosts.delete(cleanup); animation?.cancel(); };
      this.ghosts.add(cleanup);
      try {
        ghost.showPopover();
        const nodes = [ghost, ...ghost.querySelectorAll('*')];
        for (const scroll of scrolling) {
          const node = nodes[scroll.index];
          if (node) { node.scrollTop = scroll.top; node.scrollLeft = scroll.left; }
        }
        animation = ghost.animate([{ opacity, transform: 'translateY(0)' }, { opacity: 0, transform: 'translateY(4px)' }], { duration: __VMUX_POPUP_EXIT_MS__, easing: 'ease-out' });
        animation.finished.then(cleanup, cleanup);
      } catch (_) { cleanup(); }
    }
    place(popup) {
      if (!popup.isConnected) return;
      const margin = 6;
      if (popup.dataset.vmuxPopupKind === 'context') {
        const menu = popup.querySelector('[role="menu"]');
        if (!menu) return;
        const bounds = menu.getBoundingClientRect();
        menu.style.left = Math.min(Math.max(margin, Number(popup.dataset.vmuxPopupX)), Math.max(margin, window.innerWidth - bounds.width - margin)) + 'px';
        menu.style.top = Math.min(Math.max(margin, Number(popup.dataset.vmuxPopupY)), Math.max(margin, window.innerHeight - bounds.height - margin)) + 'px';
        return;
      }
      if (popup.dataset.vmuxPopupKind !== 'dropdown') return;
      const root = popup.closest('[data-vmux-dropdown-root]');
      const anchor = root?.querySelector('[data-vmux-dropdown-trigger]') ?? root;
      if (!anchor?.isConnected) return;
      let base = this.styles.get(popup);
      if (!base) {
        base = { width: popup.style.width, minWidth: popup.style.minWidth, maxHeight: popup.style.maxHeight, overflowY: popup.style.overflowY };
        this.styles.set(popup, base);
      }
      Object.assign(popup.style, base);
      const bounds = anchor.getBoundingClientRect();
      const inset = Number(popup.dataset.vmuxPopupInset ?? 0);
      const gap = Number(popup.dataset.vmuxPopupGap ?? 4);
      const top = popup.dataset.vmuxPopupPlacement === 'top';
      if (popup.dataset.vmuxPopupMatchWidth === 'true') popup.style.width = Math.max(0, bounds.width - inset * 2) + 'px';
      const availableWidth = Math.max(0, window.innerWidth - margin * 2);
      const availableHeight = Math.max(0, window.innerHeight - margin * 2);
      popup.style.maxWidth = availableWidth + 'px';
      let size = popup.getBoundingClientRect();
      if (size.width > availableWidth) {
        popup.style.minWidth = '0px';
        popup.style.width = availableWidth + 'px';
        size = popup.getBoundingClientRect();
      }
      if (size.height > availableHeight) {
        popup.style.maxHeight = availableHeight + 'px';
        popup.style.overflowY = 'auto';
        size = popup.getBoundingClientRect();
      }
      let x = popup.dataset.vmuxPopupAlign === 'end' ? bounds.right - inset - size.width : bounds.left + inset;
      let y = top ? bounds.top - gap - size.height : bounds.bottom + gap;
      const above = bounds.top - gap - size.height;
      const below = bounds.bottom + gap;
      if (!top && y + size.height > window.innerHeight - margin && above >= margin) y = above;
      else if (top && y < margin && below + size.height <= window.innerHeight - margin) y = below;
      x = Math.min(Math.max(margin, x), Math.max(margin, window.innerWidth - size.width - margin));
      y = Math.min(Math.max(margin, y), Math.max(margin, window.innerHeight - size.height - margin));
      Object.assign(popup.style, { left: x + 'px', top: y + 'px', right: 'auto', bottom: 'auto', visibility: 'visible' });
    }
    apply(captures) {
      const popups = [...document.querySelectorAll('[data-vmux-popup-kind]')];
      if (popups.some(popup => !this.known.has(popup) || this.activations.get(popup) !== (popup.dataset.vmuxPopupActivation ?? ''))) for (const cleanup of [...this.ghosts]) cleanup();
      for (const capture of captures) if (!this.visible(capture.element)) this.retire(capture);
      for (const popup of popups) {
        const activation = popup.dataset.vmuxPopupActivation ?? '';
        const first = !this.known.has(popup) || this.activations.get(popup) !== activation;
        this.known.add(popup);
        this.activations.set(popup, activation);
        if (popup.dataset.state === 'closed') continue;
        if (first && popup.hasAttribute('popover')) {
          if (popup.dataset.vmuxPopupKind === 'dropdown') {
            for (const other of document.querySelectorAll('[data-vmux-dropdown-popup]:popover-open')) if (other !== popup) other.hidePopover();
          }
          try { if (!popup.matches(':popover-open')) popup.showPopover(); } catch (_) {}
          this.closing.delete(popup);
        }
        if (!this.visible(popup)) continue;
        this.closing.delete(popup);
        this.place(popup);
        if (first && popup.dataset.vmuxPopupKind === 'context') popup.focus({ preventScroll: true });
      }
    }
  }
  const presentation = new Presentation();
  document.addEventListener('beforetoggle', event => {
    const element = event.target;
    if (!(element instanceof Element) || !element.hasAttribute('data-vmux-popup-kind')) return;
    if (element.dataset.vmuxPopupKind === 'dropdown') {
      const root = element.closest('[data-vmux-dropdown-root]');
      root?.querySelector('[data-vmux-dropdown-trigger]')?.setAttribute('aria-expanded', String(event.newState === 'open'));
      root?.querySelector('[data-vmux-dropdown-indicator]')?.setAttribute('data-state', event.newState);
    }
    if (event.newState === 'closed') presentation.retire(presentation.snapshot(element));
    else presentation.closing.delete(element);
  }, true);
  document.addEventListener('pointerdown', event => {
    const path = event.composedPath();
    for (const popup of document.querySelectorAll('[data-vmux-popup-kind][popover="manual"]:popover-open')) {
      const context = popup.dataset.vmuxPopupKind === 'context';
      const target = context ? popup.querySelector('[role="menu"]') : popup;
      const root = context ? null : popup.closest('[data-vmux-dropdown-root]');
      if (!path.includes(target) && (!root || !path.includes(root))) popup.hidePopover();
    }
  }, true);
  document.addEventListener('keydown', event => {
    if (event.key !== 'Escape') return;
    for (const popup of document.querySelectorAll('[data-vmux-popup-kind][popover="manual"]:popover-open')) popup.hidePopover();
  }, true);
  return presentation;
})();
globalThis.__vmuxPopupPresentation.apply([]);
"#;

#[derive(Clone, Copy)]
pub struct Dismissal(u64);

impl Dismissal {
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
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    fn listen_script(self) -> String {
        format!(
            "{DISMISSAL_RUNTIME}\nglobalThis.__vmuxPopupDismissal.subscribe({}, () => dioxus.send(true));",
            self.0
        )
    }

    fn remove_script(self) -> String {
        format!("globalThis.__vmuxPopupDismissal?.remove({});", self.0)
    }
}

const DISMISSAL_RUNTIME: &str = r#"
globalThis.__vmuxPopupDismissal ??= (() => {
  class Dismissal {
    listeners = new Map();
    subscribe(owner, dismiss) { this.listeners.set(owner, dismiss); }
    remove(owner) { this.listeners.delete(owner); }
    dismiss() {
      const listeners = [...this.listeners.entries()];
      for (const [owner, dismiss] of listeners) {
        try { dismiss(); } catch (_) { this.listeners.delete(owner); }
      }
    }
  }
  const dismissal = new Dismissal();
  window.addEventListener('blur', event => {
    if (event.target === window) dismissal.dismiss();
  });
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') dismissal.dismiss();
  });
  window.addEventListener('pagehide', () => dismissal.dismiss());
  window.addEventListener('vmux-deactivated', () => dismissal.dismiss());
  return dismissal;
})();
"#;

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
    }

    #[test]
    fn native_popup_close_updates_the_trigger_before_the_host_round_trip() {
        PopupProbe::verify(include_str!("popup_state_test.js"), &Surface::runtime());
    }

    #[test]
    fn popups_share_deactivation_and_preserve_focus_moves_inside_the_page() {
        PopupProbe::verify(include_str!("popup_dismissal_test.js"), DISMISSAL_RUNTIME);
    }
}
