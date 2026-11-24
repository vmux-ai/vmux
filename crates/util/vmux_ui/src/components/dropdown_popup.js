
    __VMUX_POPUP_RUNTIME__
    __VMUX_DropdownPopupOwner::CONNECTION_WATCHER__
    const id = '__VMUX_DROPDOWN_POPUP__';
    const entries = globalThis.__vmuxDropdownPopupEntries ??= new Map();
    entries.get(id)?.cleanup();
    const popup = document.getElementById(id);
    const root = popup?.closest('[data-vmux-dropdown-root]');
    if (popup && root) {
      for (const other of document.querySelectorAll(
        '[data-vmux-dropdown-popup]:popover-open'
      )) {
        if (other !== popup) other.hidePopover();
      }
      let active = true;
      let frame = 0;
      const anchor = root.querySelector('[data-vmux-dropdown-trigger]') ?? root;
      let unwatch = () => {};
      const place = () => {
        frame = 0;
        if (!active || !popup.isConnected || !popup.matches(':popover-open')) return;
        globalThis.__vmuxPopupPresentation.place(popup);
      };
      const schedule = () => {
        if (!active || frame) return;
        frame = requestAnimationFrame(place);
      };
      const toggle = event => {
        if (event.newState === 'open') schedule();
      };
      const observer = new ResizeObserver(schedule);
      observer.observe(anchor);
      observer.observe(popup);
      popup.addEventListener('toggle', toggle);
      window.addEventListener('resize', schedule, true);
      window.addEventListener('scroll', schedule, true);
      const cleanup = () => {
        if (!active) return;
        active = false;
        if (frame) cancelAnimationFrame(frame);
        observer.disconnect();
        unwatch();
        popup.removeEventListener('toggle', toggle);
        window.removeEventListener('resize', schedule, true);
        window.removeEventListener('scroll', schedule, true);
        if (popup.matches(':popover-open')) {
          try { popup.hidePopover(); } catch (error) {}
        }
        if (entries.get(id)?.cleanup === cleanup) entries.delete(id);
      };
      unwatch = watchConnection(id, popup, cleanup);
      entries.set(id, { cleanup });
      place();
    }
    