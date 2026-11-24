
    __VMUX_POPUP_RUNTIME__
    __VMUX_DropdownPopupOwner::CONNECTION_WATCHER__
    globalThis.__vmuxDropdownKeyboardCleanup?.();
    let active = true;
    const root = document.getElementById('__VMUX_DROPDOWN_ROOT__');
    let unwatch = () => {};
    const rootFor = event => event.target instanceof Element
      ? event.target.closest('[data-vmux-dropdown-keyboard="true"]')
      : null;
    const optionsFor = root => [...root.querySelectorAll('[data-vmux-dropdown-option]:not(:disabled)')];
    const clear = root => {
      for (const option of optionsFor(root)) {
        option.removeAttribute('data-vmux-dropdown-active');
        option.removeAttribute('aria-selected');
      }
    };
    const activate = (root, options, index) => {
      clear(root);
      const option = options[index];
      option.setAttribute('data-vmux-dropdown-active', 'true');
      option.setAttribute('aria-selected', 'true');
      const popup = option.closest('[data-vmux-dropdown-popup]');
      if (popup) globalThis.__vmuxPopupPresentation.reveal(popup, option);
    };
    const keydown = event => {
      if (event.isComposing) return;
      const root = rootFor(event);
      if (!root) return;
      const options = optionsFor(root);
      if (!options.length) return;
      const current = options.findIndex(option => option.getAttribute('data-vmux-dropdown-active') === 'true');
      const control = event.ctrlKey && !event.altKey && !event.metaKey;
      const key = event.key.toLowerCase();
      const forward = event.key === 'ArrowDown' || (control && key === 'n');
      const backward = event.key === 'ArrowUp' || (control && key === 'p');
      if (forward || backward) {
        event.preventDefault();
        event.stopPropagation();
        const index = forward
          ? (current < 0 ? 0 : (current + 1) % options.length)
          : (current < 0 ? options.length - 1 : (current - 1 + options.length) % options.length);
        activate(root, options, index);
        return;
      }
      if (event.key === 'Enter' && current >= 0) {
        event.preventDefault();
        event.stopPropagation();
        options[current].click();
      }
    };
    const input = event => {
      const root = rootFor(event);
      if (root) clear(root);
    };
    const cleanup = () => {
      if (!active) return;
      active = false;
      unwatch();
      document.removeEventListener('keydown', keydown, true);
      document.removeEventListener('input', input, true);
      if (globalThis.__vmuxDropdownKeyboardCleanup === cleanup) {
        delete globalThis.__vmuxDropdownKeyboardCleanup;
      }
    };
    globalThis.__vmuxDropdownKeyboardCleanup = cleanup;
    document.addEventListener('keydown', keydown, true);
    document.addEventListener('input', input, true);
    if (root) {
      unwatch = watchConnection(
        `keyboard:${root.id}`,
        root,
        cleanup,
        () => root.isConnected && root.getAttribute('data-vmux-dropdown-open') === 'true',
      );
    }
    dioxus.send(true);
    