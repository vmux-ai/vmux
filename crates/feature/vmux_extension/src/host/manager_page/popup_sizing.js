
(() => {
  globalThis.__vmuxHostedPopup = true;
  if (globalThis.__vmuxPopupSizer) return;
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    cef.emit({ channel: "vmux-extension-popup-close", window_id: globalThis.__vmuxPopupWindowId });
  });
  let scheduled = false;
  let lastWidth = 0;
  let lastHeight = 0;
  const measure = () => {
    scheduled = false;
    const body = document.body;
    if (!body) return;
    const root = document.documentElement;
    const viewportHeight = root.clientHeight || globalThis.innerHeight;
    const viewportWidth = root.clientWidth || globalThis.innerWidth;
    let height = 0;
    let width = 0;
    for (const element of body.querySelectorAll("*")) {
      const style = getComputedStyle(element);
      if (style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse") {
        continue;
      }
      const rect = element.getBoundingClientRect();
      if (!Number.isFinite(rect.bottom) || !Number.isFinite(rect.right)) {
        continue;
      }
      const fillsHeight = rect.top <= 1 && Math.abs(rect.height - viewportHeight) <= 1 && element.scrollHeight <= element.clientHeight + 1;
      const fillsWidth = rect.left <= 1 && Math.abs(rect.width - viewportWidth) <= 1 && element.scrollWidth <= element.clientWidth + 1;
      if (!fillsHeight) {
        height = Math.max(height, rect.bottom);
      }
      if (!fillsWidth) {
        width = Math.max(width, rect.right);
      }
      if (element.scrollHeight > element.clientHeight + 1) {
        height = Math.max(height, rect.top + element.scrollHeight);
      }
      if (element.scrollWidth > element.clientWidth + 1) {
        width = Math.max(width, rect.left + element.scrollWidth);
      }
      resize.observe(element);
    }
    const style = getComputedStyle(body);
    height = Math.ceil(height + parseFloat(style.paddingBottom || 0) + parseFloat(style.marginBottom || 0));
    width = Math.ceil(width + parseFloat(style.paddingRight || 0) + parseFloat(style.marginRight || 0));
    if (height === lastHeight && width === lastWidth) return;
    lastHeight = height;
    lastWidth = width;
    cef.emit({ channel: "vmux-extension-popup-size", width, height });
  };
  const schedule = () => {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(measure);
  };
  const resize = new ResizeObserver(schedule);
  resize.observe(document.documentElement);
  resize.observe(document.body);
  const mutation = new MutationObserver(schedule);
  mutation.observe(document.body, { childList: true, subtree: true, attributes: true, characterData: true });
  globalThis.__vmuxPopupSizer = { resize, mutation };
  schedule();
})();
