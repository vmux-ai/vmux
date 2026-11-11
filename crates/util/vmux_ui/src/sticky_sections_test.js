const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const script = fs.readFileSync(0, 'utf8');
const intersections = [];
const mutations = [];
class Element {
  constructor(marker, parent = null) {
    this.marker = marker;
    this.parentElement = parent;
    this.children = [];
    this.isConnected = true;
    this.dataset = {};
    this.attributes = {};
    this.inert = true;
    this.rect = marker === 'header' ? { top: 12, bottom: 76 } : { top: 30, bottom: 300 };
    parent?.children.push(this);
  }
  matches(selector) { return selector === `[data-vmux-sticky-${this.marker}]`; }
  closest(selector) {
    let current = this;
    while (current) {
      if (current.matches(selector)) return current;
      current = current.parentElement;
    }
    return null;
  }
  querySelectorAll(selector) {
    return this.children.flatMap(child => [...(child.matches(selector) ? [child] : []), ...child.querySelectorAll(selector)]);
  }
  querySelector(selector) { return this.children.find(child => selector === `:scope > [data-vmux-sticky-${child.marker}]`) || null; }
  setAttribute(name, value) { this.attributes[name] = value; }
  getBoundingClientRect() { return this.rect; }
}
const root = new Element('container');
const first = new Element('section', root);
const firstSentinel = new Element('sentinel', first);
const firstHeader = new Element('header', first);
const second = new Element('section', root);
const secondSentinel = new Element('sentinel', second);
const secondHeader = new Element('header', second);
const scope = {
  window: {}, Element,
  document: { querySelectorAll: selector => root.querySelectorAll(selector) },
  IntersectionObserver: class {
    constructor(callback, options) { this.callback = callback; this.root = options.root; this.observed = new Set(); intersections.push(this); }
    observe(node) { this.observed.add(node); }
    unobserve(node) { this.observed.delete(node); }
    disconnect() { this.disconnected = true; this.observed.clear(); }
  },
  MutationObserver: class {
    constructor(callback) { this.callback = callback; mutations.push(this); }
    observe() {}
  },
};
vm.runInNewContext(script, scope);
vm.runInNewContext(script, scope);
assert.equal(intersections.length, 1);
assert.equal(mutations.length, 1);
assert.equal(intersections[0].root, root);
assert.equal(intersections[0].observed.size, 4);
const crossing = (target, top) => {
  if (target.marker === 'header') target.rect = { top, bottom: top + 64 };
  else target.parentElement.rect.top = top;
  intersections[0].callback([{ target, rootBounds: { top: 12 }, boundingClientRect: { top } }]);
};
crossing(firstSentinel, 30);
assert.equal(firstHeader.dataset.vmuxStickyActive, 'false');
assert.equal(firstHeader.inert, true);
crossing(firstSentinel, 11);
assert.equal(firstHeader.dataset.vmuxStickyActive, 'true');
assert.equal(firstHeader.inert, false);
assert.equal(firstHeader.attributes['aria-hidden'], 'false');
crossing(firstHeader, -70);
assert.equal(firstHeader.dataset.vmuxStickyActive, 'false');
assert.equal(firstHeader.inert, true);
crossing(secondSentinel, 30);
assert.equal(secondHeader.dataset.vmuxStickyActive, 'false');
crossing(secondSentinel, 11);
assert.equal(secondHeader.dataset.vmuxStickyActive, 'true');
crossing(secondSentinel, 13);
assert.equal(secondHeader.dataset.vmuxStickyActive, 'false');
assert.equal(secondHeader.inert, true);
const pending = new Element('section', root);
const pendingSentinel = new Element('sentinel', pending);
mutations[0].callback([{ addedNodes: [pending], removedNodes: [] }]);
assert.equal(intersections[0].observed.size, 4);
const pendingHeader = new Element('header', pending);
mutations[0].callback([{ addedNodes: [pendingHeader], removedNodes: [] }]);
assert.equal(intersections[0].observed.size, 6);
crossing(pendingSentinel, 11);
assert.equal(pendingHeader.dataset.vmuxStickyActive, 'true');
assert.equal(pendingHeader.inert, false);
const added = new Element('section', root);
const addedSentinel = new Element('sentinel', added);
new Element('header', added);
mutations[0].callback([{ addedNodes: [added], removedNodes: [] }]);
assert.equal(intersections[0].observed.size, 8);
addedSentinel.isConnected = false;
mutations[0].callback([{ addedNodes: [], removedNodes: [addedSentinel] }]);
assert.equal(intersections[0].observed.size, 7);
firstHeader.isConnected = false;
firstHeader.parentElement = null;
crossing(firstHeader, -70);
root.isConnected = false;
mutations[0].callback([{ addedNodes: [], removedNodes: [root] }]);
assert.equal(intersections[0].disconnected, true);
assert.equal(scope.window.__vmuxStickySections.states.size, 0);
