(() => {
  const root = document.getElementById(__VMUX_ADDRESS_ROOT__);
  if (!root) return;
  const duration = __VMUX_ADDRESS_DURATION__;
  const owners = globalThis.__vmuxAddressMotion ??= new Map();
  owners.get(root.id)?.();
  let previous = new Map();
  let frame = 0;
  const animations = new Map();
  const ghosts = new Set();
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
  const play = (node, frames, done) => {
    animations.get(node)?.cancel();
    const animation = node.animate(frames, { duration, easing: 'ease-out' });
    animations.set(node, animation);
    const finish = () => {
      if (animations.get(node) === animation) animations.delete(node);
      done?.();
    };
    animation.finished.then(finish, finish);
  };
  const reconcile = () => {
    const next = new Map();
    const positions = new Map();
    const offsets = new Map();
    for (const [key, state] of previous) {
      let x = 0;
      let y = 0;
      if (state.node.isConnected && animations.has(state.node)) {
        const transform = getComputedStyle(state.node).transform;
        if (transform !== 'none') {
          const matrix = new DOMMatrixReadOnly(transform);
          x = matrix.m41;
          y = matrix.m42;
        }
      }
      offsets.set(state.node, { x, y });
      positions.set(key, { x: state.rect.x + x, y: state.rect.y + y });
    }
    for (const node of root.querySelectorAll('[data-vmux-crumb-key]')) {
      const key = node.dataset.vmuxCrumbKey;
      const old = previous.get(key);
      const offset = offsets.get(node) ?? { x: 0, y: 0 };
      const bounds = node.getBoundingClientRect();
      const rect = { x: bounds.x - offset.x, y: bounds.y - offset.y, width: bounds.width, height: bounds.height };
      const label = node.textContent;
      const copy = old?.node === node && old.label === label ? old.copy : node.cloneNode(true);
      next.set(key, { node, rect, label, copy });
    }
    if (!reduced.matches && typeof root.animate === 'function') {
      for (const [key, state] of next) {
        const old = previous.get(key);
        const changed = !old || old.node !== state.node || Math.abs(old.rect.x - state.rect.x) > 0.5 || Math.abs(old.rect.y - state.rect.y) > 0.5;
        if (!changed) continue;
        const position = positions.get(key);
        const dx = position ? position.x - state.rect.x : 6;
        const dy = position ? position.y - state.rect.y : 0;
        const frames = old ? [
          { transform: `translate(${dx}px, ${dy}px)` },
          { transform: 'translate(0, 0)' },
        ] : [
          { transform: `translate(${dx}px, ${dy}px)`, opacity: 0 },
          { transform: 'translate(0, 0)', opacity: 1 },
        ];
        play(state.node, frames);
      }
      const bounds = root.getBoundingClientRect();
      for (const [key, state] of previous) {
        if (next.has(key)) continue;
        animations.get(state.node)?.cancel();
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
    } else {
      for (const animation of animations.values()) animation.cancel();
      animations.clear();
      for (const ghost of ghosts) ghost.remove();
      ghosts.clear();
    }
    previous = next;
  };
  const schedule = () => {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      reconcile();
    });
  };
  for (const node of root.querySelectorAll('[data-vmux-crumb-key]')) {
    previous.set(node.dataset.vmuxCrumbKey, { node, rect: node.getBoundingClientRect(), label: node.textContent, copy: node.cloneNode(true) });
  }
  const observer = new MutationObserver(records => {
    if (records.some(record => {
      if (record.target.closest?.('[data-vmux-address-ghost]')) return false;
      if (record.type !== 'childList') return true;
      return [...record.addedNodes, ...record.removedNodes].some(node => node.nodeType !== 1 || node.dataset.vmuxAddressGhost !== 'true');
    })) schedule();
  });
  observer.observe(root, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ['class', 'style'] });
  let size = root.getBoundingClientRect();
  const resize = new ResizeObserver(() => {
    const next = root.getBoundingClientRect();
    if (Math.abs(next.width - size.width) > 0.5 || Math.abs(next.height - size.height) > 0.5) {
      size = next;
      schedule();
    }
  });
  resize.observe(root);
  reduced.addEventListener('change', schedule);
  const cleanup = () => {
    observer.disconnect();
    resize.disconnect();
    reduced.removeEventListener('change', schedule);
    if (frame) cancelAnimationFrame(frame);
    for (const animation of animations.values()) animation.cancel();
    for (const ghost of ghosts) ghost.remove();
    previous.clear();
    animations.clear();
    ghosts.clear();
    if (owners.get(root.id) === cleanup) owners.delete(root.id);
  };
  owners.set(root.id, cleanup);
})();
