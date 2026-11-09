const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const shim = fs.readFileSync(0, 'utf8');

class Item {
  constructor(key, left) {
    this.origin = key;
    this.dataset = { vmuxMotionItem: key };
    this.attributes = { id: key, 'data-dioxus-id': '12', 'data-vmux-motion-item': key };
    this.style = {};
    this.left = left;
    this.children = [];
    this.animations = [];
    this.textContent = 'Start';
  }
  getBoundingClientRect() { return { left: this.left, top: 0, right: this.left + 208, bottom: 36, width: 208, height: 36 }; }
  cloneNode() {
    const copy = new Item(this.origin, this.left);
    copy.children = this.children.map(child => child.cloneNode(true));
    return copy;
  }
  querySelectorAll() { return this.children; }
  removeAttribute(name) {
    delete this.attributes[name];
    if (name === 'data-vmux-motion-item') delete this.dataset.vmuxMotionItem;
    if (name === 'data-vmux-stack-row') delete this.dataset.vmuxStackRow;
  }
  setAttribute(name, value) { this.attributes[name] = value; }
  animate(frames, options) {
    let finish;
    const animation = { frames, options, finished: new Promise(resolve => { finish = resolve; }), finish: () => finish(), cancel() {} };
    this.animations.push(animation);
    return animation;
  }
  remove() { this.parent.children = this.parent.children.filter(child => child !== this); }
}

class MovementProbe {
  constructor(reduced = false) {
    this.a = new Item('tab-a', 100);
    this.b = new Item('tab-b', 312);
    this.c = new Item('tab-c', 524);
    this.plus = new Item('new-tab-button', 736);
    this.b.children = [new Item('close-b', 312)];
    this.container = {
      children: [this.a, this.b, this.c, this.plus],
      getBoundingClientRect: () => ({ left: 100, top: 0, right: 1100, bottom: 36 }),
      querySelectorAll() { return this.children.filter(child => child.dataset.vmuxMotionItem || child.dataset.vmuxStackRow); },
      appendChild(child) { child.parent = this; this.children.push(child); },
    };
    let polls = 0;
    const probe = this;
    const scope = {
      document: {
        title: '', head: {}, addEventListener() {}, querySelector: () => null,
        querySelectorAll: () => [], getSelection: () => ({ isCollapsed: true }),
        getElementById: id => id === 'tabs' ? probe.container : null,
      },
      addEventListener() {}, TextEncoder, TextDecoder, Uint8Array, DataView, ArrayBuffer,
      Map, Set, WeakMap, WeakSet, Promise, Element: Item, HTMLInputElement: class {}, HTMLTextAreaElement: class {},
      XMLHttpRequest: class { open() {} send() {} }, MutationObserver: class { observe() {} },
      queueMicrotask, requestAnimationFrame: callback => queueMicrotask(callback),
      setTimeout: callback => queueMicrotask(callback), matchMedia: () => ({ matches: reduced }),
      btoa: value => Buffer.from(value, 'binary').toString('base64'),
      atob: value => Buffer.from(value, 'base64').toString('binary'),
      ipc: { postMessage() {} }, __vmuxFrameCapability: '777',
      interpreter: { root: {}, sendSerializedEvent() {}, run_from_bytes() {
        probe.container.children = [probe.a, probe.c, probe.plus];
        probe.c.left = 312;
        probe.plus.left = 524;
      } },
      fetch: async () => {
        if (++polls === 1) return this.response(412);
        return new Promise(resolve => { this.deliver = resolve; });
      },
    };
    scope.window = scope;
    scope.globalThis = scope;
    vm.runInNewContext(shim, scope);
    scope.vmuxWry.start();
  }
  response(status) {
    const requests = Buffer.from(JSON.stringify([{ kind: 'animateLayout', element: 'tabs', duration_ms: 120, revision: 1, epoch: 1 }]));
    const packet = Buffer.alloc(requests.length + 5);
    packet.writeUInt32LE(requests.length);
    requests.copy(packet, 4);
    packet[packet.length - 1] = 1;
    return {
      status, ok: status === 200,
      headers: { get: name => ({
        'x-vmux-navigation': '1', 'x-vmux-frame-epoch': '1', 'x-vmux-frame-capability': '777', 'x-vmux-frame': '0',
      })[name] || null },
      arrayBuffer: async () => packet.buffer.slice(packet.byteOffset, packet.byteOffset + packet.byteLength),
    };
  }
  async close() {
    await new Promise(resolve => setImmediate(resolve));
    this.deliver(this.response(200));
    await new Promise(resolve => setImmediate(resolve));
  }
}

(async () => {
  const probe = new MovementProbe();
  await probe.close();
  assert.deepEqual(probe.container.children.filter(child => child.dataset.vmuxMotionItem).map(child => child.origin), ['tab-a', 'tab-c', 'new-tab-button']);
  assert.equal(probe.c.left, 312, 'layout must close the gap before animation completes');
  const ghost = probe.container.children.find(child => !child.dataset.vmuxMotionItem);
  assert.equal(ghost.origin, 'tab-b', 'the removed middle tab must be the one that fades');
  assert.equal(ghost.inert, true);
  assert.equal(ghost.attributes['aria-hidden'], 'true');
  for (const node of [ghost, ...ghost.children]) {
    assert.equal(node.attributes.id, undefined);
    assert.equal(node.attributes['data-dioxus-id'], undefined);
  }
  assert.equal(probe.a.animations.length, 0);
  assert.equal(probe.c.animations[0].frames[0].transform, 'translate(212px, 0px)');
  assert.equal(probe.plus.animations[0].frames[0].transform, 'translate(212px, 0px)');
  assert.ok(probe.c.animations[0].options.duration + probe.c.animations[0].options.delay <= 160);
  ghost.animations[0].finish();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(probe.container.children.map(child => child.origin), ['tab-a', 'tab-c', 'new-tab-button']);
  const reduced = new MovementProbe(true);
  await reduced.close();
  assert.equal(reduced.c.left, 312);
  assert.equal(reduced.c.animations.length, 0);
  assert.deepEqual(reduced.container.children.map(child => child.origin), ['tab-a', 'tab-c', 'new-tab-button']);
})().catch(error => { console.error(error); process.exitCode = 1; });
