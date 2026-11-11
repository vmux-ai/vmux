(() => {
  if (window.__vmuxStickySections) return;
  const states = new Map();
  const containerSelector = '[data-vmux-sticky-container]';
  const sentinelSelector = '[data-vmux-sticky-sentinel]';
  const headerSelector = '[data-vmux-sticky-header]';
  const attach = sentinel => {
    const root = sentinel.closest(containerSelector);
    const header = sentinel.parentElement?.querySelector(':scope > [data-vmux-sticky-header]');
    if (!root || !header) return;
    let state = states.get(root);
    if (!state) {
      const observer = new IntersectionObserver(entries => {
        for (const entry of entries) {
          const header = entry.target.matches(headerSelector) ? entry.target : entry.target.parentElement?.querySelector(':scope > [data-vmux-sticky-header]');
          const section = header?.parentElement;
          if (!section || !header.isConnected || !entry.rootBounds) continue;
          const active = section.getBoundingClientRect().top < entry.rootBounds.top && header.getBoundingClientRect().bottom > entry.rootBounds.top;
          header.dataset.vmuxStickyActive = String(active);
          header.inert = !active;
          header.setAttribute('aria-hidden', String(!active));
        }
      }, { root, threshold: [0, 1] });
      state = { observer, targets: new Set() };
      states.set(root, state);
    }
    for (const target of [sentinel, header]) {
      if (state.targets.has(target)) continue;
      state.targets.add(target);
      state.observer.observe(target);
    }
  };
  const scan = node => {
    if (!(node instanceof Element)) return;
    if (node.matches(headerSelector)) {
      const sentinel = node.parentElement?.querySelector(':scope > [data-vmux-sticky-sentinel]');
      if (sentinel) {
        const state = states.get(sentinel.closest(containerSelector));
        state?.observer.unobserve(sentinel);
        state?.targets.delete(sentinel);
        attach(sentinel);
      }
    }
    if (node.matches(sentinelSelector)) attach(node);
    for (const sentinel of node.querySelectorAll(sentinelSelector)) attach(sentinel);
  };
  const mutations = new MutationObserver(records => {
    let removed = false;
    for (const record of records) {
      for (const node of record.addedNodes) scan(node);
      for (const node of record.removedNodes) {
        if (node instanceof Element) removed = true;
      }
    }
    if (!removed) return;
    for (const [root, state] of states) {
      if (!root.isConnected) {
        state.observer.disconnect();
        states.delete(root);
        continue;
      }
      for (const target of state.targets) {
        if (target.isConnected) continue;
        state.observer.unobserve(target);
        state.targets.delete(target);
      }
    }
  });
  mutations.observe(document, { childList: true, subtree: true });
  window.__vmuxStickySections = { states };
  for (const sentinel of document.querySelectorAll(sentinelSelector)) attach(sentinel);
})();
