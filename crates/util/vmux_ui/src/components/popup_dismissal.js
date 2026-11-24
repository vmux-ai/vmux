
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
