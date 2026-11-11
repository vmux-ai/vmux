const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const shim = fs.readFileSync(0, 'utf8');

class ScrollHost {
  constructor() {
    this.frames = [];
    this.requests = [];
    let polls = 0;
    const host = this;
    class Request {
      constructor() {
        this.headers = {};
        this.responseText = '{"preventDefault":true,"stopPropagation":true}';
      }
      open(method, url, async) { this.async = async; this.url = url; }
      setRequestHeader(name, value) { this.headers[name] = String(value); }
      send() {
        host.requests.push({
          async: this.async,
          url: this.url,
          headers: this.headers,
          body: JSON.parse(Buffer.from(this.headers['dioxus-data'], 'base64').toString()),
        });
      }
    }
    const interpreter = {
      root: {}, eventsPath: 'vmux://file/__events', run_from_bytes() {},
      sendSerializedEvent(body) {
        const xhr = new Request();
        xhr.open('POST', this.eventsPath, false);
        xhr.setRequestHeader('dioxus-data', Buffer.from(JSON.stringify(body)).toString('base64'));
        xhr.send();
        return JSON.parse(xhr.responseText);
      },
    };
    const scope = {
      document: {
        title: '', head: {}, addEventListener() {}, querySelector: () => null,
        querySelectorAll: () => [], getSelection: () => ({ isCollapsed: true }),
      },
      addEventListener() {}, TextEncoder, TextDecoder, Uint8Array, DataView, ArrayBuffer,
      Map, Set, WeakMap, WeakSet, Promise, Element: class {}, HTMLInputElement: class {}, HTMLTextAreaElement: class {},
      XMLHttpRequest: Request, MutationObserver: class { observe() {} },
      queueMicrotask, requestAnimationFrame: callback => host.frames.push(callback),
      setTimeout: callback => queueMicrotask(callback), matchMedia: () => ({ matches: false }),
      btoa: value => Buffer.from(value, 'binary').toString('base64'),
      atob: value => Buffer.from(value, 'base64').toString('binary'),
      ipc: { postMessage() {} }, __vmuxFrameCapability: '777', interpreter,
      fetch: async () => {
        if (++polls > 1) return new Promise(() => {});
        return {
          status: 412,
          headers: { get: name => ({
            'x-vmux-navigation': '1', 'x-vmux-frame-epoch': '1',
            'x-vmux-frame-capability': '777', 'x-vmux-frame': '0',
          })[name] || null },
        };
      },
    };
    scope.window = scope;
    scope.globalThis = scope;
    vm.runInNewContext(shim, scope);
    scope.vmuxWry.start();
    this.scope = scope;
  }
  send(name, element, top) {
    return this.scope.interpreter.sendSerializedEvent({ name, element, bubbles: false, data: { scroll_top: top } });
  }
  paint() { for (const callback of this.frames.splice(0)) callback(); }
}

(async () => {
  const host = new ScrollHost();
  await new Promise(resolve => setImmediate(resolve));
  for (let top = 0; top < 100; top++) host.send('scroll', 4, top);
  host.send('scroll', 5, 300);
  host.send('scrollend', 4, 99);
  assert.equal(host.requests.length, 0, 'scroll dispatch must return without waiting for native replies');
  assert.equal(host.frames.length, 1, 'a scroll burst needs one animation-frame callback');
  host.paint();
  assert.equal(host.requests.length, 3);
  for (const request of host.requests) {
    assert.equal(request.async, true);
    assert.equal(request.url, 'vmux://file/__events');
    assert.equal(request.headers['x-vmux-navigation'], '1');
    assert.equal(request.headers['x-vmux-ipc-epoch'], '1');
  }
  assert.deepEqual(host.requests.map(request => [request.body.name, request.body.element, request.body.data.scroll_top]), [
    ['scroll', 4, 99], ['scroll', 5, 300], ['scrollend', 4, 99],
  ]);
  const result = host.send('keydown', 4, 0);
  assert.equal(host.requests.at(-1).async, false);
  assert.equal(result.preventDefault, true);
  assert.equal(result.stopPropagation, true);
  host.send('scroll', 4, 500);
  host.scope.__vmuxIpcEpoch = 2;
  host.paint();
  assert.equal(host.requests.length, 4, 'a stale scroll must not reach a new document');
  host.scope.__vmuxIpcEpoch = 1;
  host.send('scroll', 4, 600);
  host.scope.interpreter = {};
  host.paint();
  assert.equal(host.requests.length, 4, 'a replaced interpreter must discard queued scrolls');

  const chat = new ScrollHost();
  await new Promise(resolve => setImmediate(resolve));
  let now = 0;
  let nextTimer = 0;
  const timers = new Map();
  chat.scope.Date = { now: () => now };
  chat.scope.setTimeout = (callback, delay) => {
    const id = ++nextTimer;
    timers.set(id, { callback, at: now + delay });
    return id;
  };
  chat.scope.clearTimeout = id => timers.delete(id);
  chat.scope.interpreter.getNode = id => ({ closest: selector => id === 4 && selector === '#chat-scroll' ? {} : null });
  const advance = amount => {
    now += amount;
    for (const [id, timer] of [...timers]) {
      if (timer.at <= now) {
        timers.delete(id);
        timer.callback();
      }
    }
  };
  chat.send('wheel', 4, 0);
  chat.send('scroll', 4, 1);
  advance(0);
  assert.deepEqual(chat.requests.map(request => request.body.name), ['wheel', 'scroll']);
  assert.ok(chat.requests.every(request => request.async), 'chat input must leave native scrolling independent of host replies');
  for (let frame = 0; frame < 30; frame++) {
    chat.send('scroll', 4, frame);
    advance(16);
  }
  assert.ok(chat.requests.filter(request => request.body.name === 'scroll').length <= 6, 'continuous scrolling must not wake the host once per display frame');
  chat.send('scrollend', 4, 30);
  assert.equal(chat.requests.at(-1).body.name, 'scrollend');
  assert.equal(chat.requests.at(-1).body.data.scroll_top, 30);
  chat.send('scroll', 4, 40);
  chat.send('click', 4, 0);
  assert.deepEqual(chat.requests.slice(-2).map(request => [request.body.name, request.async]), [['scroll', false], ['click', false]], 'navigation must observe the last viewport position before replacing the page');
  chat.send('scroll', 4, 50);
  const sent = chat.requests.length;
  chat.scope.__vmuxIpcEpoch = 2;
  advance(200);
  assert.equal(chat.requests.length, sent, 'a delayed chat scroll cannot reach a new document');
})().catch(error => { console.error(error); process.exitCode = 1; });
