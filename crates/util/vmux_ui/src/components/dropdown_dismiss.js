
__VMUX_DropdownPopupOwner::CONNECTION_WATCHER__
const id = '__VMUX_DROPDOWN_ID__';
const activation = __VMUX_ACTIVATION__;
const popup = document.getElementById(id);
if (popup) {
  let opened = popup.matches(':popover-open');
  const root = popup.closest('[data-vmux-dropdown-root]');
  const manual = popup.getAttribute('popover') === 'manual';
  const entries = globalThis.__vmuxDropdownToggleEntries ??= new Map();
  entries.get(id)?.cleanup();
  let active = true;
  let unwatch = () => {};
  const cleanup = () => {
    if (!active) return;
    active = false;
    unwatch();
    popup.removeEventListener('beforetoggle', beforetoggle);
    document.removeEventListener('pointerdown', pointerdown, true);
    document.removeEventListener('keydown', keydown, true);
    if (entries.get(id)?.cleanup === cleanup) entries.delete(id);
  };
  const dismiss = () => {
    if (!active) return;
    cleanup();
    if (Number(root?.dataset.vmuxDropdownActivation ?? 0) !== activation) return;
    dioxus.send('dismissed');
  };
  const beforetoggle = event => {
    if (event.newState === 'open') opened = true;
    if (event.newState === 'closed' && opened) dismiss();
  };
  const pointerdown = event => {
    const path = event.composedPath();
    if (path.includes(popup) || (root && path.includes(root))) return;
    dismiss();
  };
  const keydown = event => {
    if (event.key !== 'Escape' || popup.getAttribute('popover') !== 'manual') return;
    event.preventDefault();
    event.stopPropagation();
    dismiss();
  };
  entries.set(id, { cleanup });
  unwatch = watchConnection(id, popup, cleanup);
  popup.addEventListener('beforetoggle', beforetoggle);
  if (manual) {
    document.addEventListener('pointerdown', pointerdown, true);
    document.addEventListener('keydown', keydown, true);
  }
}
