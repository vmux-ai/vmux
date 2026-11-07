const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const source = JSON.parse(fs.readFileSync(0, 'utf8'));

class Element {
  constructor(parent = null) {
    this.parentElement = parent;
    this.isConnected = true;
    this.dataset = {};
    this.style = {};
    this.attrs = new Map();
    this.handlers = new Map();
    this.scrollTop = 0;
    this.scrollLeft = 0;
    this.clientTop = 0;
    this.clientHeight = 0;
    this.scrollHeight = 0;
  }
  contains(element) {
    for (; element; element = element.parentElement) if (element === this) return true;
    return false;
  }
  closest(selector) {
    for (let element = this; element; element = element.parentElement) {
      if (selector === '[data-vmux-dropdown-root]' && element === root) return element;
      if (selector === '[data-vmux-dropdown-keyboard="true"]' && element === root) return element;
      if (selector === '[data-vmux-dropdown-popup]' && element === popup) return element;
    }
    return null;
  }
  hasAttribute(name) { return this.attrs.has(name); }
  getAttribute(name) { return this.attrs.get(name) ?? null; }
  setAttribute(name, value) { this.attrs.set(name, String(value)); }
  removeAttribute(name) { this.attrs.delete(name); }
  addEventListener(name, callback) { this.handlers.set(name, callback); }
  removeEventListener(name) { this.handlers.delete(name); }
  querySelectorAll() { return []; }
  matches(selector) { return selector === ':popover-open' && this.opened; }
}

const root = new Element();
root.id = 'test-dropdown-root';
root.scrollLeft = 37;
const anchor = new Element(root);
anchor.getBoundingClientRect = () => ({ left: 200, right: 320, top: 40, bottom: 64, width: 120, height: 24 });
root.querySelector = selector => selector === '[data-vmux-dropdown-trigger]' ? anchor : null;
root.querySelectorAll = selector => selector.startsWith('[data-vmux-dropdown-option]') ? [selected] : [];
const popup = new Element(root);
popup.id = 'vmux-dropdown-popup-0';
popup.dataset = { vmuxPopupKind: 'dropdown', vmuxPopupAlign: 'end', vmuxPopupGap: '4' };
popup.attrs = new Map([['data-vmux-popup-kind', 'dropdown'], ['data-vmux-dropdown-popup', ''], ['popover', 'manual']]);
popup.clientHeight = 160;
popup.scrollHeight = 640;
popup.getBoundingClientRect = () => {
  const left = parseFloat(popup.style.left) || 0;
  const top = (parseFloat(popup.style.top) || 0) + 4;
  return { left, right: left + 192, top, bottom: top + 160, width: 192, height: 160 };
};
const selected = new Element(popup);
selected.attrs = new Map([['data-vmux-dropdown-option', ''], ['data-vmux-dropdown-selected', 'true']]);
selected.getBoundingClientRect = () => {
  const bounds = popup.getBoundingClientRect();
  const top = bounds.top + 480 - popup.scrollTop;
  return { left: bounds.left + 4, right: bounds.right - 4, top, bottom: top + 32, width: 184, height: 32 };
};
selected.scrollIntoView = options => {
  popup.scrollTop = 352;
  if (options?.container !== 'nearest') root.scrollLeft += 80;
};
let selectionAvailable = true;
popup.querySelector = selector => selector === '[data-vmux-dropdown-selected="true"]' && selectionAvailable ? selected : null;
const handlers = new Map();
let popups = [];
const frames = [];
const document = {
  documentElement: root,
  addEventListener(name, callback) {
    const existing = handlers.get(name) || [];
    existing.push(callback);
    handlers.set(name, existing);
  },
  removeEventListener() {},
  getElementById: id => id === popup.id ? popup : id === root.id ? root : null,
  querySelectorAll(selector) {
    if (selector === '[data-vmux-popup-kind]') return popups;
    if (selector.includes(':popover-open')) return popups.filter(popup => popup.opened);
    return [];
  },
};
const dispatch = (name, event) => {
  for (const handler of handlers.get(name) || []) handler(event);
};
popup.showPopover = () => {
  dispatch('beforetoggle', { target: popup, newState: 'open' });
  popup.opened = true;
};
popup.hidePopover = () => { popup.opened = false; };
const scope = {
  Element, document, Map, Set, WeakMap, WeakSet,
  innerWidth: 800, innerHeight: 600,
  matchMedia: () => ({ matches: true }),
  getComputedStyle: () => ({ opacity: '1', visibility: 'visible' }),
  addEventListener() {}, removeEventListener() {},
  requestAnimationFrame: callback => { frames.push(callback); return frames.length; },
  cancelAnimationFrame() {},
  MutationObserver: class { observe() {} disconnect() {} },
  ResizeObserver: class { observe() {} disconnect() {} },
  dioxus: { send() {} },
};
scope.window = scope;
scope.globalThis = scope;
vm.runInNewContext(source.runtime, scope);
popups = [popup];
scope.__vmuxPopupPresentation.apply([]);
let viewport = popup.getBoundingClientRect();
let item = selected.getBoundingClientRect();
assert.ok(item.top >= viewport.top && item.bottom <= viewport.top + popup.clientHeight, 'the selected entry must be visible in the first painted menu');
assert.equal(root.scrollLeft, 37, 'initial selection must not move the breadcrumb');

popup.scrollTop = 24;
dispatch('wheel', { target: selected });
vm.runInNewContext('(function () {' + source.mount + '})();', scope);
scope.__vmuxPopupPresentation.apply(scope.__vmuxPopupPresentation.capture());
for (const frame of frames.splice(0)) frame();
assert.equal(popup.scrollTop, 24, 'mounted setup and later paints must preserve fresh reading input');
assert.equal(root.scrollLeft, 37);

vm.runInNewContext('(function () {' + source.keyboard + '})();', scope);
popup.scrollTop = 0;
dispatch('keydown', {
  target: selected, key: 'ArrowDown', isComposing: false,
  ctrlKey: false, altKey: false, metaKey: false,
  preventDefault() {}, stopPropagation() {},
});
viewport = popup.getBoundingClientRect();
item = selected.getBoundingClientRect();
assert.ok(item.top >= viewport.top && item.bottom <= viewport.top + popup.clientHeight, 'keyboard navigation must reveal its option locally');
assert.equal(root.scrollLeft, 37, 'keyboard navigation must not move the breadcrumb');

popup.opened = false;
popup.scrollTop = 0;
scope.__vmuxPopupPresentation = undefined;
vm.runInNewContext(source.runtime, scope);
viewport = popup.getBoundingClientRect();
item = selected.getBoundingClientRect();
assert.ok(item.bottom <= viewport.top + popup.clientHeight, 'reopening a menu must initialize its selection again');

selectionAvailable = false;
popup.opened = false;
popup.scrollTop = 0;
scope.__vmuxPopupPresentation = undefined;
vm.runInNewContext(source.runtime, scope);
dispatch('wheel', { target: selected });
popup.scrollTop = 12;
selectionAvailable = true;
scope.__vmuxPopupPresentation.apply([]);
assert.equal(popup.scrollTop, 12, 'a selection arriving late must not override reading input');

selectionAvailable = false;
popup.opened = false;
popup.scrollTop = 0;
scope.__vmuxPopupPresentation = undefined;
vm.runInNewContext(source.runtime, scope);
selectionAvailable = true;
scope.__vmuxPopupPresentation.apply([]);
viewport = popup.getBoundingClientRect();
item = selected.getBoundingClientRect();
assert.ok(item.bottom <= viewport.top + popup.clientHeight, 'newly available options must be placed before their first paint');

const nested = new Element(popup);
nested.clientHeight = 112;
nested.scrollHeight = 640;
nested.getBoundingClientRect = () => {
  const bounds = popup.getBoundingClientRect();
  return { left: bounds.left, top: bounds.top + 24, width: 192, height: 112 };
};
selected.parentElement = nested;
selected.getBoundingClientRect = () => {
  const bounds = nested.getBoundingClientRect();
  const top = bounds.top + 480 - nested.scrollTop;
  return { top, bottom: top + 32, width: 184, height: 32 };
};
popup.scrollHeight = popup.clientHeight;
scope.__vmuxPopupPresentation.reveal(popup, selected);
assert.ok(selected.getBoundingClientRect().bottom <= nested.getBoundingClientRect().top + nested.clientHeight, 'nested menu viewports must initialize without scrolling the page');
assert.equal(root.scrollLeft, 37);
