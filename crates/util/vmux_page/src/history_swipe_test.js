const vm = require('node:vm');
const fs = require('node:fs');
const assert = require('node:assert/strict');
const script = fs.readFileSync(0, 'utf8');

class GestureProbe {
  constructor() {
    this.now = 0;
    this.handlers = new Map();
    this.windowHandlers = new Map();
    this.timers = new Map();
    this.nextTimer = 1;
    this.body = { children: [], appendChild: node => {
      node.ownerBody = this.body;
      this.body.children.push(node);
    } };
    this.Element = class {
      constructor(scrollLeft = 0, overflowX = 'auto') {
        this.scrollLeft = scrollLeft;
        this.scrollWidth = 500;
        this.clientWidth = 200;
        this.overflowX = overflowX;
        this.attributes = new Map();
        this.style = {};
      }
      matches() { return false; }
      setAttribute(name, value) { this.attributes.set(name, value); }
      getAttribute(name) { return this.attributes.get(name) ?? null; }
      attachShadow() {
        const nodes = new Map();
        this.shadow = { querySelector: selector => {
          if (!nodes.has(selector)) nodes.set(selector, { style: {}, setAttribute() {} });
          return nodes.get(selector);
        } };
        return this.shadow;
      }
      remove() {
        if (!this.ownerBody) return;
        const index = this.ownerBody.children.indexOf(this);
        if (index >= 0) this.ownerBody.children.splice(index, 1);
      }
    };
    this.scope = {
      Element: this.Element, navigations: [],
      performance: { now: () => this.now },
      getComputedStyle: node => ({ overflowX: node.overflowX }),
      document: {
        scrollingElement: null,
        body: this.body,
        createElement: () => new this.Element(),
        addEventListener: (name, handler) => this.handlers.set(name, handler),
        removeEventListener: name => this.handlers.delete(name),
      },
      addEventListener: (name, callback) => this.windowHandlers.set(name, callback),
      removeEventListener: name => this.windowHandlers.delete(name),
      setTimeout: (callback, delay) => {
        const id = this.nextTimer++;
        this.timers.set(id, { callback, at: this.now + delay });
        return id;
      },
      clearTimeout: id => this.timers.delete(id),
    };
    this.scope.window = this.scope;
    this.scope.globalThis = this.scope;
    vm.runInNewContext(script, this.scope);
  }
  wheel(x, y = 0, path = [], extra = {}) {
    this.now += 16;
    let prevented = false;
    this.handlers.get('wheel')({
      isTrusted: true, deltaMode: 0, cancelable: true, deltaX: x, deltaY: y,
      composedPath: () => path, preventDefault: () => { prevented = true; }, ...extra,
    });
    return prevented;
  }
  expect(...directions) { assert.deepEqual([...this.scope.navigations], directions); }
  flushTimer() {
    const [id, timer] = [...this.timers].sort((a, b) => a[1].at - b[1].at)[0];
    this.timers.delete(id);
    this.now = timer.at;
    timer.callback();
  }
}

const feedback = new GestureProbe();
feedback.wheel(-60);
assert.equal(feedback.body.children.length, 1, 'a partial swipe must show feedback before navigation');
const indicator = feedback.body.children[0];
assert.equal(indicator.getAttribute('data-direction'), 'back');
assert.equal(indicator.getAttribute('aria-hidden'), 'true');
const initialProgress = Number(indicator.getAttribute('data-progress'));
feedback.expect();
feedback.wheel(-80);
assert.equal(feedback.body.children.length, 1, 'one gesture must reuse its indicator');
assert.ok(Number(indicator.getAttribute('data-progress')) > initialProgress);
feedback.wheel(-80);
feedback.expect('back');
assert.equal(indicator.getAttribute('data-state'), 'committed');
feedback.flushTimer();
assert.equal(indicator.getAttribute('data-state'), 'closing');
feedback.flushTimer();
assert.equal(feedback.body.children.length, 0);

const cancelled = new GestureProbe();
cancelled.wheel(60);
assert.equal(cancelled.body.children[0].getAttribute('data-direction'), 'forward');
cancelled.scope.__vmuxHistorySwipeCleanup();
assert.equal(cancelled.body.children.length, 0, 'document replacement must remove gesture feedback');
assert.equal(cancelled.timers.size, 0, 'discarded documents must release their timers');

const noise = new GestureProbe();
noise.wheel(0, 0);
noise.wheel(0, 3);
noise.wheel(-120, 1);
noise.wheel(-120, 1);
noise.wheel(-240);
noise.expect('back');

const forward = new GestureProbe();
forward.wheel(120);
forward.wheel(120);
forward.expect('forward');

const vertical = new GestureProbe();
vertical.wheel(1, 30);
vertical.wheel(-240);
vertical.expect();
assert.equal(vertical.body.children.length, 0, 'vertical scrolling must not show history feedback');

const protectedInput = new GestureProbe();
const input = new protectedInput.Element();
input.matches = () => true;
protectedInput.wheel(-240, 0, [input]);
protectedInput.expect();

const scrolling = new GestureProbe();
scrolling.wheel(-240, 0, [new scrolling.Element(50)]);
scrolling.expect();
assert.equal(scrolling.body.children.length, 0, 'horizontal content scrolling must keep its own interaction');
const leftEdge = new GestureProbe();
leftEdge.wheel(-240, 0, [new leftEdge.Element(0)]);
leftEdge.expect('back');
const rightEdge = new GestureProbe();
rightEdge.wheel(240, 0, [new rightEdge.Element(300)]);
rightEdge.expect('forward');
const clipped = new GestureProbe();
const root = new clipped.Element(0, 'hidden');
clipped.scope.document.scrollingElement = root;
clipped.wheel(240, 0, [root]);
clipped.expect('forward');

for (const extra of [{ isTrusted: false }, { ctrlKey: true }, { deltaMode: 1 }]) {
  const invalid = new GestureProbe();
  invalid.wheel(-240, 0, [], extra);
  invalid.expect();
}
