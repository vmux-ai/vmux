
(() => {
  if (window.__vmuxDisclosures) return;
  const states = new WeakMap();
  const choices = new Map();
  const remember = (key, expanded) => {
    choices.delete(key);
    choices.set(key, expanded);
    while (choices.size > 2048) choices.delete(choices.keys().next().value);
  };
  const restore = node => {
    if (!(node instanceof Element)) return;
    const details = node.matches('details[data-vmux-disclosure-key]') ? [node] : [];
    details.push(...node.querySelectorAll('details[data-vmux-disclosure-key]'));
    for (const element of details) {
      const key = element.dataset.vmuxDisclosureKey;
      if (key && choices.has(key)) {
        element.open = choices.get(key);
        remember(key, element.open);
      }
    }
  };
  const apply = records => {
    for (const record of records) {
      if (record.type === 'attributes') restore(record.target);
      for (const node of record.addedNodes ?? []) restore(node);
    }
  };
  const observer = new MutationObserver(apply);
  observer.observe(document, { childList: true, subtree: true, attributes: true, attributeFilter: ['data-vmux-disclosure-key'] });
  window.__vmuxDisclosures = { states, choices, flush: () => apply(observer.takeRecords()) };
  document.addEventListener('click', event => {
    if (event.defaultPrevented || event.button !== 0 || !(event.target instanceof Element)) return;
    const summary = event.target.closest('summary');
    const details = summary?.parentElement;
    if (!details?.matches('details.disclosure')) return;
    if (event.target.closest('a, button, input, textarea, select')) return;
    event.preventDefault();
    const previous = states.get(details);
    const expanded = !(previous?.expanded ?? details.open);
    if (details.dataset.vmuxDisclosureKey) remember(details.dataset.vmuxDisclosureKey, expanded);
    const from = details.getBoundingClientRect().height;
    const opacities = new Map([...details.children].map(child => [child, Number(getComputedStyle(child).opacity)]));
    const original = previous?.original ?? { height: details.style.height, overflow: details.style.overflow };
    details.style.height = from + 'px';
    previous?.animation.cancel();
    for (const animation of previous?.fades ?? []) animation.cancel();
    details.dataset.vmuxDisclosureManaged = 'true';
    details.dataset.vmuxDisclosureOpen = String(expanded);
    details.style.height = original.height;
    details.open = expanded;
    const to = details.getBoundingClientRect().height;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches || typeof details.animate !== 'function') {
      Object.assign(details.style, original);
      states.delete(details);
      return;
    }
    details.open = true;
    details.style.height = from + 'px';
    details.style.overflow = 'hidden';
    const timing = { duration: __VMUX_DISCLOSURE_MS__, easing: 'ease-out', fill: 'forwards' };
    const animation = details.animate([{ height: from + 'px' }, { height: to + 'px' }], timing);
    const fades = [];
    for (const child of details.children) {
      if (child === summary) continue;
      fades.push(child.animate([{ opacity: previous ? opacities.get(child) : expanded ? 0 : 1 }, { opacity: expanded ? 1 : 0 }], timing));
    }
    const state = { animation, fades, expanded, original };
    states.set(details, state);
    animation.onfinish = () => {
      if (states.get(details) !== state) return;
      details.open = expanded;
      Object.assign(details.style, original);
      states.delete(details);
      animation.cancel();
      for (const fade of fades) fade.cancel();
    };
  });
})();
