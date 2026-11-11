const vm = require('node:vm');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const script = fs.readFileSync(0, 'utf8');
const handlers = new Map();
let reduced = false;
class Element {
  constructor(details = null) { this.details = details; this.style = { height: '', overflow: '' }; this.dataset = {}; this.animations = []; }
  closest(selector) { return selector === 'summary' ? this.details.summary : null; }
  matches(selector) { return selector === 'details.disclosure'; }
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
const scope = { Element, window: { matchMedia: () => ({ matches: reduced }) }, document: { addEventListener: (name, handler) => handlers.set(name, handler) }, getComputedStyle: () => ({ opacity: '1' }) };
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
