const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const shim = fs.readFileSync(0, 'utf8');

function packet() {
  const json = Buffer.from(JSON.stringify([{ kind: 'preserveScroll', revision: 1, epoch: 1 }]));
  const bytes = Buffer.alloc(5 + json.length);
  bytes.writeUInt32LE(json.length);
  json.copy(bytes, 4);
  bytes[bytes.length - 1] = 1;
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
}

async function scenario(growth, clamp, input, expected, initial = null, replay = false, options = {}) {
  let scope;
  let polls = 0;
  let documentTop = 110;
  const observers = [];
  const handlers = new Map();
  const content = {};
  const frames = options.followingFrames?.length || (replay ? 2 : 1);
  const container = {
    isConnected: true, scrollTop: options.unlaidOut ? 0 : 100,
    scrollHeight: options.unlaidOut ? 0 : 1000, clientHeight: options.unlaidOut ? 0 : 200, dataset: {},
    firstElementChild: content, addEventListener: (name, handler) => handlers.set(name, handler),
    contains: element => element === anchor,
    getBoundingClientRect: () => ({ left: 0, right: 500, top: 0, bottom: 500 }),
  };
  const anchor = {
    id: 'file', isConnected: true,
    closest: () => anchor,
    getBoundingClientRect: () => ({ top: documentTop - container.scrollTop, height: 100 }),
  };
  const document = {
    title: '', head: {}, addEventListener() {}, querySelector: () => null,
    querySelectorAll: selector => {
      if (selector === '[data-vmux-initial-scroll]') return initial === null || !container.isConnected ? [] : [container];
      if (selector === '[data-vmux-preserve-scroll]') return initial === null ? [container] : [];
      return [];
    },
    getElementById: id => id === anchor.id ? anchor : null,
    elementsFromPoint: () => [anchor],
    getSelection: () => ({ isCollapsed: true }),
  };
  scope = {
    document, addEventListener() {}, TextEncoder, TextDecoder, Uint8Array, DataView, ArrayBuffer,
    Map, Set, WeakMap, WeakSet, Promise, Element: class {}, HTMLInputElement: class {}, HTMLTextAreaElement: class {},
    ResizeObserver: class {
      constructor(callback) { this.callback = callback; this.observed = new Set(); observers.push(this); }
      observe(target) { this.observed.add(target); }
      unobserve(target) { this.observed.delete(target); }
      disconnect() { this.disconnected = true; this.observed.clear(); }
    },
    XMLHttpRequest: class { open() {} send() {} }, MutationObserver: class { observe() {} },
    queueMicrotask, requestAnimationFrame: callback => queueMicrotask(callback),
    setTimeout: callback => queueMicrotask(callback),
    matchMedia: () => ({ matches: false }), getComputedStyle: () => ({ opacity: '1' }),
    btoa: value => Buffer.from(value, 'binary').toString('base64'),
    atob: value => Buffer.from(value, 'base64').toString('binary'),
    ipc: { postMessage() {} }, __vmuxFrameCapability: '777',
    interpreter: { root: {}, run_from_bytes() {
      documentTop += growth;
      if (initial !== null) container.dataset.vmuxInitialScroll = initial;
      scope.renderedFrames = (scope.renderedFrames || 0) + 1;
      container.dataset.vmuxFollowScroll = String(options.followingFrames?.[scope.renderedFrames - 1] ?? options.follow ?? false);
      if (clamp !== null) container.scrollTop = clamp;
      queueMicrotask(() => {
        if (scope.appliedInput === true) return;
        if (options.manual) handlers.get('wheel')?.({});
        if (options.interactivePointer) handlers.get('pointerdown')?.({ target: { closest: () => ({}) } });
        container.scrollTop += input;
        scope.appliedInput = true;
        if (options.lateGrowth) container.scrollHeight += options.lateGrowth;
        if (options.unlaidOut) { container.scrollHeight = 1000; container.clientHeight = 200; }
        for (const observer of observers) {
          if (observer.observed.has(options.unlaidOut ? container : content)) observer.callback();
        }
        if (options.disconnect) container.isConnected = false;
      });
    } },
    fetch: async () => {
      const index = polls++;
      if (index > frames) return new Promise(() => {});
      return {
        status: index === 0 ? 412 : 200, ok: index !== 0,
        headers: { get: name => ({
          'x-vmux-navigation': '1', 'x-vmux-frame-epoch': '1',
          'x-vmux-frame-capability': '777', 'x-vmux-frame': String(index),
        })[name] || null },
        arrayBuffer: async () => packet(),
      };
    },
  };
  scope.window = scope;
  scope.globalThis = scope;
  vm.runInNewContext(shim, scope);
  scope.vmuxWry.start();
  const required = frames + 2;
  for (let i = 0; i < 20 && polls < required; i++) await new Promise(resolve => setImmediate(resolve));
  assert.equal(polls, required, 'the frame must finish');
  assert.equal(container.scrollTop, expected);
  if (options.lateGrowth || options.unlaidOut) assert.ok(observers.length, 'layout growth must be observed');
  if (options.disconnect) assert.ok(observers.some(observer => observer.disconnected), 'discarded scroll containers must release their observer');
}

(async () => {
  await scenario(80, null, 20, 200);
  await scenario(-80, 40, 0, 20);
  await scenario(0, null, 20, 120);
  await scenario(0, null, 0, 800, 'bottom');
  await scenario(0, null, 0, 150, '150');
  await scenario(0, null, -100, 700, 'bottom', true);
  await scenario(0, null, 0, 1600, 'bottom', false, { follow: true, lateGrowth: 800 });
  await scenario(0, null, 0, 1600, 'bottom', false, { follow: true, lateGrowth: 800, interactivePointer: true });
  await scenario(0, null, 0, 150, '150', false, { follow: true, lateGrowth: 800 });
  await scenario(0, null, -100, 700, 'bottom', true, { follow: true, manual: true, lateGrowth: 800 });
  await scenario(0, null, -100, 1600, 'bottom', true, { manual: true, lateGrowth: 800, followingFrames: [true, false, true] });
  await scenario(0, null, 0, 800, 'bottom', false, { unlaidOut: true });
  await scenario(0, null, 0, 800, 'bottom', true, { follow: true, disconnect: true });
})().catch(error => { console.error(error); process.exitCode = 1; });
