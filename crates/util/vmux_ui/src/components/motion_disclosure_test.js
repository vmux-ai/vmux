const vm = require('node:vm');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const script = fs.readFileSync(0, 'utf8');
const handlers = new Map();
let reduced = false;
let mutation;
const pendingMutations = [];
class Element {
  constructor(details = null) { this.details = details; this.style = { height: '', overflow: '' }; this.dataset = {}; this.animations = []; }
  closest(selector) { return selector === 'summary' ? this.details.summary : null; }
  matches(selector) { return selector === 'details.disclosure'; }
  querySelectorAll() { return []; }
  getBoundingClientRect() { return { height: this.style.height ? Number.parseFloat(this.style.height) : this.open ? 180 : 24 }; }
  animate(frames, timing) {
    const animation = { frames, timing, cancelled: false, cancel() { this.cancelled = true; } };
    this.animations.push(animation);
    return animation;
  }
}
const details = new Element();
details.open = false;
details.summary = new Element(details);
details.summary.parentElement = details;
details.content = new Element(details);
details.children = [details.summary, details.content];
const scope = { Element, MutationObserver: class { constructor(callback) { mutation = callback; } observe() {} takeRecords() { return pendingMutations.splice(0); } }, window: { matchMedia: () => ({ matches: reduced }) }, document: { addEventListener: (name, handler) => handlers.set(name, handler) }, getComputedStyle: () => ({ opacity: '1' }) };
vm.runInNewContext(script, scope);
const click = () => handlers.get('click')({ defaultPrevented: false, button: 0, target: details.summary, preventDefault() {} });
click();
assert.equal(details.open, true);
assert.equal(details.dataset.vmuxDisclosureOpen, 'true');
let opening = details.animations.at(-1);
assert.equal(opening.frames[0].height, '24px');
assert.equal(opening.frames[1].height, '180px');
assert.ok(opening.timing.duration > 0 && opening.timing.duration <= 160);
opening.onfinish();
assert.equal(details.style.height, '');
for (let index = 0; index < 5000; index++) {
  details.dataset.vmuxDisclosureKey = `history:${index}`;
  click();
}
assert.ok(scope.window.__vmuxDisclosures.choices.size < 5000, 'old history choices must be evicted during a long document lifetime');
assert.equal(scope.window.__vmuxDisclosures.choices.has('history:0'), false);
assert.equal(scope.window.__vmuxDisclosures.choices.has('history:4999'), true);
const latest = new Element();
latest.dataset.vmuxDisclosureKey = 'history:4999';
latest.open = !details.open;
latest.matches = selector => selector === 'details[data-vmux-disclosure-key]';
pendingMutations.push({ type: 'childList', addedNodes: [latest] });
scope.window.__vmuxDisclosures.flush();
assert.equal(latest.open, details.open, 'the latest reader choice must survive reparenting after old choices are evicted');
details.dataset.vmuxDisclosureKey = 'session:one:tool:arguments';
click();
click();
assert.equal(details.open, true);
const reparented = new Element();
reparented.dataset.vmuxDisclosureKey = details.dataset.vmuxDisclosureKey;
reparented.open = false;
reparented.matches = selector => selector === 'details[data-vmux-disclosure-key]';
pendingMutations.push({ type: 'childList', addedNodes: [reparented] });
scope.window.__vmuxDisclosures.flush();
assert.equal(reparented.open, true, 'a reader-opened disclosure must retain its state when older history reparents its tool');
click();
assert.equal(details.open, true);
assert.equal(details.dataset.vmuxDisclosureOpen, 'false');
let closing = details.animations.at(-1);
assert.equal(closing.frames[1].height, '24px');
closing.onfinish();
assert.equal(details.open, false);
assert.equal(details.style.height, '');
click();
opening = details.animations.at(-1);
click();
assert.equal(opening.cancelled, true);
closing = details.animations.at(-1);
closing.onfinish();
assert.equal(details.open, false);
reduced = true;
const count = details.animations.length;
click();
assert.equal(details.open, true);
assert.equal(details.animations.length, count);
assert.equal(details.style.height, '');
