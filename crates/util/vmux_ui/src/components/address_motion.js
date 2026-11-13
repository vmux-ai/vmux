(() => {
  const root = document.getElementById(__VMUX_ADDRESS_ROOT__);
  if (!root) return;
  const duration = __VMUX_ADDRESS_DURATION__;
  const owners = globalThis.__vmuxAddressMotion ??= new Map();
  owners.get(root.id)?.();
  let previous = new Map();
  const animations = new Map();
  const ghosts = new Set();
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
  const play = (node, frames, done) => {
    const animation = node.animate(frames, { duration, easing: 'ease-out' });
    animations.set(node, animation);
    animation.finished.then(() => {
      if (animations.get(node) === animation) animations.delete(node);
      done?.();
    }, () => done?.());
  };
  const reconcile = () => {
    const next = new Map();
    const positions = new Map();
    for (const [key, state] of previous) {
      const transform = state.node.isConnected ? getComputedStyle(state.node).transform : 'none';
      const matrix = transform === 'none' ? null : new DOMMatrixReadOnly(transform);
      positions.set(key, { x: state.rect.x + (matrix?.m41 ?? 0), y: state.rect.y + (matrix?.m42 ?? 0) });
    }
    for (const animation of animations.values()) animation.cancel();
    animations.clear();
    for (const node of root.querySelectorAll('[data-vmux-crumb-key]')) {
      next.set(node.dataset.vmuxCrumbKey, { node, rect: node.getBoundingClientRect(), label: node.textContent, copy: node.cloneNode(true) });
    }
    if (!reduced.matches && typeof root.animate === 'function') {
      for (const [key, state] of next) {
        const old = previous.get(key);
        const position = positions.get(key);
        const dx = position ? position.x - state.rect.x : 6;
        const dy = position ? position.y - state.rect.y : 0;
        if (!old || old.label !== state.label || Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) {
          play(state.node, [
            { transform: `translate(${dx}px, ${dy}px)`, opacity: old ? (old.label === state.label ? 1 : 0.65) : 0 },
            { transform: 'translate(0, 0)', opacity: 1 },
          ]);
        }
      }
      const bounds = root.getBoundingClientRect();
      for (const [key, state] of previous) {
        if (next.has(key)) continue;
        const ghost = state.copy;
        ghost.dataset.vmuxAddressGhost = 'true';
        for (const node of [ghost, ...ghost.querySelectorAll('*')]) {
          node.removeAttribute('id');
          node.removeAttribute('data-dioxus-id');
          node.removeAttribute('data-vmux-crumb-key');
        }
        ghost.inert = true;
        ghost.setAttribute('aria-hidden', 'true');
        const position = positions.get(key);
        Object.assign(ghost.style, { position: 'absolute', left: `${position.x - bounds.x}px`, top: `${position.y - bounds.y}px`, width: `${state.rect.width}px`, height: `${state.rect.height}px`, pointerEvents: 'none' });
        ghosts.add(ghost);
        root.appendChild(ghost);
        play(ghost, [{ opacity: 1, transform: 'translateX(0)' }, { opacity: 0, transform: 'translateX(-6px)' }], () => { ghosts.delete(ghost); ghost.remove(); });
      }
    }
    previous = next;
  };
  for (const node of root.querySelectorAll('[data-vmux-crumb-key]')) {
    previous.set(node.dataset.vmuxCrumbKey, { node, rect: node.getBoundingClientRect(), label: node.textContent, copy: node.cloneNode(true) });
  }
  const observer = new MutationObserver(records => {
    if (records.some(record => {
      if (record.target.closest?.('[data-vmux-address-ghost]')) return false;
      if (record.type !== 'childList') return true;
      return [...record.addedNodes, ...record.removedNodes].some(node => node.nodeType !== 1 || node.dataset.vmuxAddressGhost !== 'true');
    })) reconcile();
  });
  observer.observe(root, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ['class', 'style'] });
  let size = root.getBoundingClientRect();
  const resize = new ResizeObserver(() => {
    const next = root.getBoundingClientRect();
    if (Math.abs(next.width - size.width) > 0.5 || Math.abs(next.height - size.height) > 0.5) {
      size = next;
      reconcile();
    }
  });
  resize.observe(root);
  const cleanup = () => {
    observer.disconnect();
    resize.disconnect();
    for (const animation of animations.values()) animation.cancel();
    for (const ghost of ghosts) ghost.remove();
    previous.clear();
    animations.clear();
    ghosts.clear();
    if (owners.get(root.id) === cleanup) owners.delete(root.id);
  };
  owners.set(root.id, cleanup);
})();
