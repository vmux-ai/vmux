const vm = require('node:vm');
const fs = require('node:fs');
const assert = require('node:assert/strict');
const script = fs.readFileSync(0, 'utf8');
const handlers = new Map();
const attributes = new Map();
const indicatorAttributes = new Map();
const root = {
  querySelector(selector) {
    if (selector === '[data-vmux-dropdown-trigger]') return { setAttribute: (name, value) => attributes.set(name, value) };
    if (selector === '[data-vmux-dropdown-indicator]') return { setAttribute: (name, value) => indicatorAttributes.set(name, value) };
    return null;
  },
};
class Element {
  constructor() { this.dataset = { vmuxPopupKind: 'dropdown' }; }
  hasAttribute(name) { return name === 'data-vmux-popup-kind'; }
  closest() { return root; }
  matches() { return false; }
  querySelectorAll() { return []; }
  getBoundingClientRect() { return { left: 0, top: 0, width: 100, height: 20 }; }
}
const scope = {
  Element, document: { addEventListener: (name, callback) => handlers.set(name, callback), querySelectorAll: () => [] },
  getComputedStyle: () => ({ visibility: 'hidden' }),
  matchMedia: () => ({ matches: true }),
};
scope.globalThis = scope;
scope.window = scope;
vm.runInNewContext(script, scope);
const popup = new Element();
handlers.get('beforetoggle')({ target: popup, newState: 'open' });
assert.equal(attributes.get('aria-expanded'), 'true');
assert.equal(indicatorAttributes.get('data-state'), 'open');
handlers.get('beforetoggle')({ target: popup, newState: 'closed' });
assert.equal(attributes.get('aria-expanded'), 'false');
assert.equal(indicatorAttributes.get('data-state'), 'closed');
