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

async function scenario(growth, clamp, input, expected) {
  let scope;
  let polls = 0;
  let documentTop = 110;
  const container = {
    isConnected: true, scrollTop: 100,
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
    querySelectorAll: selector => selector === '[data-vmux-preserve-scroll]' ? [container] : [],
    getElementById: id => id === anchor.id ? anchor : null,
    elementsFromPoint: () => [anchor],
    getSelection: () => ({ isCollapsed: true }),
  };
  scope = {
    document, addEventListener() {}, TextEncoder, TextDecoder, Uint8Array, DataView, ArrayBuffer,
    Map, Set, WeakMap, WeakSet, Promise, Element: class {}, HTMLInputElement: class {}, HTMLTextAreaElement: class {},
    XMLHttpRequest: class { open() {} send() {} }, MutationObserver: class { observe() {} },
    queueMicrotask, requestAnimationFrame: callback => queueMicrotask(callback),
    setTimeout: callback => queueMicrotask(callback),
    matchMedia: () => ({ matches: false }), getComputedStyle: () => ({ opacity: '1' }),
    btoa: value => Buffer.from(value, 'binary').toString('base64'),
    atob: value => Buffer.from(value, 'base64').toString('binary'),
    ipc: { postMessage() {} }, __vmuxFrameCapability: '777',
    interpreter: { root: {}, run_from_bytes() {
      documentTop += growth;
      if (clamp !== null) container.scrollTop = clamp;
      queueMicrotask(() => { container.scrollTop += input; });
    } },
    fetch: async () => {
      const index = polls++;
      if (index > 1) return new Promise(() => {});
      return {
        status: index === 0 ? 412 : 200, ok: index !== 0,
        headers: { get: name => ({
          'x-vmux-navigation': '1', 'x-vmux-frame-epoch': '1',
          'x-vmux-frame-capability': '777', 'x-vmux-frame': index === 0 ? '0' : '1',
        })[name] || null },
        arrayBuffer: async () => packet(),
      };
    },
  };
  scope.window = scope;
  scope.globalThis = scope;
  vm.runInNewContext(shim, scope);
  scope.vmuxWry.start();
  for (let i = 0; i < 20 && polls < 3; i++) await new Promise(resolve => setImmediate(resolve));
  assert.equal(polls, 3, 'the frame must finish');
  assert.equal(container.scrollTop, expected);
}

(async () => {
  await scenario(80, null, 20, 200);
  await scenario(-80, 40, 0, 20);
  await scenario(0, null, 20, 120);
})().catch(error => { console.error(error); process.exitCode = 1; });
