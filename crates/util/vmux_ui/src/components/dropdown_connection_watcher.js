
    const watchConnection = globalThis.__vmuxDropdownWatchConnection ??= (() => {
      const entries = new Map();
      let observer = null;
      const stop = () => {
        if (entries.size || !observer) return;
        observer.disconnect();
        observer = null;
      };
      const inspect = () => {
        for (const [key, entry] of entries) {
          if (entry.active()) continue;
          entries.delete(key);
          for (const cleanup of entry.cleanups) cleanup();
        }
        stop();
      };
      const start = () => {
        if (observer) return;
        observer = new MutationObserver(inspect);
        observer.observe(document, {
          childList: true,
          subtree: true,
          attributes: true,
          attributeFilter: ['data-vmux-dropdown-open'],
        });
      };
      return (key, element, cleanup, active = () => element.isConnected) => {
        let entry = entries.get(key);
        if (!entry) {
          entry = { element, active, cleanups: new Set() };
          entries.set(key, entry);
        }
        entry.element = element;
        entry.active = active;
        entry.cleanups.add(cleanup);
        start();
        return () => {
          const current = entries.get(key);
          if (!current) return;
          current.cleanups.delete(cleanup);
          if (!current.cleanups.size) entries.delete(key);
          stop();
        };
      };
    })();
    