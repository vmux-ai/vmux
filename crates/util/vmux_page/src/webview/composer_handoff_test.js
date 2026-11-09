const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const shim = fs.readFileSync(0, 'utf8');

class Composer {
  constructor(rect) { this.rect = rect; this.animations = []; }
  getBoundingClientRect() { return this.rect; }
  animate(frames, options) {
    const animation = { frames, options, canceled: false, finished: new Promise(() => {}), cancel() { this.canceled = true; } };
    this.animations.push(animation);
    return animation;
  }
}

class Root {
  constructor(probe, composer) { this.probe = probe; this.composer = composer; }
  querySelector() { return this.composer; }
  cloneNode() { return new Root(this.probe, null); }
  replaceWith(root) { this.probe.root = root; }
}

class HandoffProbe {
  constructor(reduced = false, missing = false) {
    this.source = new Composer({ left: 180, top: 320, width: 640, height: 110 });
    this.target = new Composer({ left: 120, top: 730, width: 760, height: 130 });
    this.root = new Root(this, missing ? null : this.source);
    this.frames = [];
    this.requests = [];
    this.revision = '1';
    let polls = 0;
    const probe = this;
    const scope = {
      document: { title: '', head: {}, body: {}, addEventListener() {}, querySelector: () => null,
        querySelectorAll: () => [], getSelection: () => ({ isCollapsed: true }) },
      addEventListener() {}, TextEncoder, TextDecoder, Uint8Array, DataView, ArrayBuffer,
      Map, Set, WeakMap, WeakSet, Promise, Element: Composer, HTMLInputElement: class {}, HTMLTextAreaElement: class {},
      XMLHttpRequest: class { open() {} send() {} }, MutationObserver: class { observe() {} },
      queueMicrotask, requestAnimationFrame: callback => this.frames.push(callback),
      setTimeout: callback => queueMicrotask(callback), matchMedia: () => ({ matches: reduced }),
      btoa: value => Buffer.from(value, 'binary').toString('base64'), atob: value => Buffer.from(value, 'base64').toString('binary'),
      ipc: { postMessage() {} }, __vmuxFrameCapability: '777',
      interpreter: { root: this.root, sendSerializedEvent() {}, run_from_bytes() {} },
      vmuxCreateInterpreter: () => ({ initialize(root) { this.root = root; }, sendSerializedEvent() {}, run_from_bytes() { this.root.composer = probe.target; } }),
      fetch: async (_, options) => {
        this.requests.push(options.headers);
        if (++polls === 1) return this.response(412);
        return new Promise(resolve => { this.deliver = resolve; });
      },
    };
    scope.window = scope;
    scope.globalThis = scope;
    this.scope = scope;
    vm.runInNewContext(shim, scope);
    scope.vmuxWry.start();
  }
  response(status, sequence = 0) {
    const requests = Buffer.from(JSON.stringify([{ kind: 'remount', revision: Number(this.revision), epoch: 1 }]));
    const packet = Buffer.alloc(requests.length + 5);
    packet.writeUInt32LE(requests.length);
    requests.copy(packet, 4);
    packet[packet.length - 1] = 1;
    return { status, ok: status === 200,
      headers: { get: name => ({ 'x-vmux-navigation': this.revision, 'x-vmux-frame-epoch': '1', 'x-vmux-frame-capability': '777', 'x-vmux-frame': String(sequence) })[name] || null },
      arrayBuffer: async () => packet.buffer.slice(packet.byteOffset, packet.byteOffset + packet.byteLength) };
  }
  async remount() {
    await new Promise(resolve => setImmediate(resolve));
    this.revision = '2';
    this.deliver(this.response(200, 1));
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(this.requests.at(-1)['x-vmux-applied-frame'], '1', 'the DOM must acknowledge navigation without waiting for animation or paint');
  }
}

(async () => {
  const probe = new HandoffProbe();
  await probe.remount();
  assert.equal(probe.root.composer, probe.target);
  assert.equal(probe.target.getBoundingClientRect().top, 730, 'final dock layout is committed immediately');
  assert.equal(probe.target.animations.length, 1);
  const animation = probe.target.animations[0];
  assert.equal(animation.frames[0].transform, 'translate(60px, -410px) scale(0.8421052631578947, 0.8461538461538461)');
  assert.equal(animation.frames[1].transform, 'translate(0px, 0px) scale(1, 1)');
  assert.equal(animation.frames[0].transformOrigin, 'top left');
  assert.ok(animation.options.duration > 0 && animation.options.duration <= 160);
  const reduced = new HandoffProbe(true);
  await reduced.remount();
  assert.equal(reduced.root.composer, reduced.target);
  assert.equal(reduced.target.animations.length, 0);
  const missing = new HandoffProbe(false, true);
  await missing.remount();
  assert.equal(missing.root.composer, missing.target);
  assert.equal(missing.target.animations.length, 0);
})().catch(error => { console.error(error); process.exitCode = 1; });
