const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const shim = fs.readFileSync(0, 'utf8');

async function scenario(padding, expected, reduced = false) {
  let polls = 0;
  let result;
  const messages = [];
  const message = { getBoundingClientRect: () => ({ top: 280 }) };
  const container = {
    scrollTop: 100, contains: element => element === message,
    getBoundingClientRect: () => ({ top: 20 }), scrollTo: options => { result = options; },
  };
  const request = Buffer.from(JSON.stringify([{ kind: 'scrollSection', revision: 1, epoch: 1, container: 'root', element: 'message', behavior: 'smooth' }]));
  const frame = Buffer.alloc(request.length + 5);
  frame.writeUInt32LE(request.length);
  request.copy(frame, 4);
  frame[frame.length - 1] = 1;
  const scope = {
    document: {
      title: '', head: {}, addEventListener() {}, querySelector: () => null,
      querySelectorAll: () => [], getSelection: () => ({ isCollapsed: true }),
      getElementById: id => id === 'root' ? container : id === 'message' ? message : null,
    },
    addEventListener() {}, TextEncoder, TextDecoder, Uint8Array, DataView, ArrayBuffer,
    Map, Set, WeakMap, WeakSet, Promise, Element: class {}, HTMLInputElement: class {}, HTMLTextAreaElement: class {},
    XMLHttpRequest: class { open() {} send() {} }, MutationObserver: class { observe() {} },
    ResizeObserver: class { observe() {} unobserve() {} disconnect() {} },
    queueMicrotask, requestAnimationFrame: callback => queueMicrotask(callback),
    setTimeout: callback => queueMicrotask(callback), matchMedia: () => ({ matches: reduced }),
    getComputedStyle: () => ({ opacity: '1', scrollPaddingTop: padding }),
    btoa: value => Buffer.from(value, 'binary').toString('base64'),
    atob: value => Buffer.from(value, 'base64').toString('binary'),
    ipc: { postMessage: message => messages.push(message) }, __vmuxFrameCapability: '777',
    interpreter: { root: {}, sendSerializedEvent() {}, run_from_bytes() {} },
    fetch: async () => {
      if (++polls > 2) return new Promise(() => {});
      return {
        status: polls === 1 ? 412 : 200, ok: polls === 2,
        headers: { get: name => ({ 'x-vmux-navigation': '1', 'x-vmux-frame-epoch': '1', 'x-vmux-frame-capability': '777', 'x-vmux-frame': polls === 1 ? '0' : '1' })[name] || null },
        arrayBuffer: async () => frame.buffer.slice(frame.byteOffset, frame.byteOffset + frame.byteLength),
      };
    },
  };
  scope.window = scope;
  scope.globalThis = scope;
  vm.runInNewContext(shim, scope);
  scope.vmuxWry.start();
  for (let i = 0; i < 10 && !result; i++) await new Promise(resolve => setImmediate(resolve));
  assert.ok(result, 'the navigation request must scroll its target: ' + JSON.stringify({ polls, messages }));
  assert.equal(result.top, expected);
  assert.equal(result.behavior, reduced ? 'instant' : 'smooth');
}

(async () => {
  await scenario('64px', 296);
  await scenario('0px', 344);
  await scenario('64px', 296, true);
})().catch(error => { console.error(error); process.exitCode = 1; });
