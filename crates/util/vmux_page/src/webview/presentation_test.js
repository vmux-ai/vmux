const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const shim = fs.readFileSync(0, 'utf8');

class PageProbe {
  constructor() {
    this.frames = [];
    this.messages = [];
    this.requests = [];
    this.observers = [];
    this.pending = [];
    this.loading = [];
    this.rows = [];
    this.revision = '1';
    this.epoch = '1';
    this.sequence = 0;
    this.fonts = { status: 'loaded', ready: Promise.resolve() };
    this.container = {
      isConnected: true, clientHeight: 200, scrollTop: 0,
      dataset: { vmuxInitialScroll: 'bottom', vmuxFollowScroll: 'true' },
      firstElementChild: {}, addEventListener() {},
      querySelectorAll: () => this.rows,
    };
    Object.defineProperty(this.container, 'scrollHeight', {
      get: () => this.rows.length ? this.rows.reduce((height, row) => height + row.height, 0) : 1000,
    });
    const probe = this;
    const document = {
      title: '', head: {}, fonts: this.fonts, addEventListener() {},
      querySelector: () => null,
      querySelectorAll: selector => {
        if (selector === '[data-vmux-presentation-pending]') return this.pending;
        if (selector === '[data-vmux-loading-surface]') return this.loading;
        if (selector === '[data-vmux-initial-scroll]') return this.rows.length ? [this.container] : [];
        return [];
      },
      getSelection: () => ({ isCollapsed: true }),
    };
    const scope = {
      document, addEventListener() {}, TextEncoder, TextDecoder, Uint8Array, DataView, ArrayBuffer,
      Map, Set, WeakMap, WeakSet, Promise, Element: class {}, HTMLInputElement: class {}, HTMLTextAreaElement: class {},
      ResizeObserver: class {
        constructor(callback) { this.callback = callback; probe.observers.push(this); }
        observe() {} unobserve() {} disconnect() { this.disconnected = true; }
      },
      XMLHttpRequest: class { open() {} send() {} }, MutationObserver: class { observe() {} },
      queueMicrotask, requestAnimationFrame: callback => this.frames.push(callback),
      setTimeout: callback => queueMicrotask(callback),
      matchMedia: () => ({ matches: false }),
      getComputedStyle: element => element.computed || { display: 'block', visibility: 'visible', opacity: '1' },
      btoa: value => Buffer.from(value, 'binary').toString('base64'),
      atob: value => Buffer.from(value, 'base64').toString('binary'),
      ipc: { postMessage: message => this.messages.push(message) }, __vmuxFrameCapability: '777',
      interpreter: { root: {}, run_from_bytes: () => this.edit?.() },
      fetch: async (_, options) => {
        this.requests.push(options.headers);
        if (this.requests.length === 1) return this.response(412);
        return new Promise(resolve => { this.deliver = resolve; });
      },
    };
    scope.window = scope;
    scope.globalThis = scope;
    vm.runInNewContext(shim, scope);
    scope.vmuxWry.start();
  }
  response(status) {
    const packet = new Uint8Array(5);
    packet[4] = 1;
    return {
      status, ok: status === 200,
      headers: { get: name => ({
        'x-vmux-navigation': this.revision, 'x-vmux-frame-epoch': this.epoch,
        'x-vmux-frame-capability': '777', 'x-vmux-frame': String(this.sequence),
      })[name] || null },
      arrayBuffer: async () => packet.buffer,
    };
  }
  async flush() { await new Promise(resolve => setImmediate(resolve)); }
  async send(edit = () => {}, options = {}) {
    await this.flush();
    this.edit = edit;
    if (options.epoch) {
      this.epoch = options.epoch;
      this.deliver(this.response(412));
      await this.flush();
    }
    this.revision = options.revision || this.revision;
    this.sequence += 1;
    assert.equal(typeof this.deliver, 'function');
    this.deliver(this.response(200));
    await this.flush();
    assert.equal(this.requests.at(-1)['x-vmux-applied-frame'], String(this.sequence), 'DOM delivery must be acknowledged before presentation');
  }
  async paint() {
    const callbacks = this.frames.splice(0);
    for (const callback of callbacks) callback();
    await this.flush();
  }
  loadingFonts() {
    this.fonts.status = 'loading';
    this.fonts.ready = new Promise(resolve => { this.finishFonts = () => { this.fonts.status = 'loaded'; resolve(); }; });
  }
  get presented() { return this.messages.filter(message => message.includes(':presented:')); }
  row(height) {
    const row = { height, style: { contentVisibility: 'auto' } };
    this.rows.push(row);
    return row;
  }
}

(async () => {
  const loading = new PageProbe();
  loading.loadingFonts();
  loading.loading = [{}];
  await loading.send();
  await loading.paint();
  await loading.paint();
  assert.deepEqual(loading.presented, ['1:1:presented:1'], 'the first layout must present while data and fonts are still loading');
  assert.equal(loading.loading.length, 1);
  assert.equal(loading.fonts.status, 'loading');
  loading.finishFonts();
  await loading.flush();
  await loading.paint();
  await loading.send(() => { loading.loading = []; });
  await loading.paint();
  await loading.paint();
  assert.equal(loading.presented.length, 1, 'replacing a loader must not delay or repeat presentation');

  const placement = new PageProbe();
  placement.pending = [{}];
  await placement.send();
  await placement.paint();
  await placement.paint();
  assert.deepEqual(placement.presented, [], 'messages awaiting initial scroll placement must stay hidden');
  await placement.send(() => { placement.pending = []; });
  await placement.paint();
  assert.deepEqual(placement.presented, [], 'delivery is not a paint opportunity');
  await placement.paint();
  assert.deepEqual(placement.presented, ['1:1:presented:1']);
  await placement.send();
  await placement.paint();
  await placement.paint();
  assert.equal(placement.presented.length, 1, 'streaming updates must not repeat initial presentation');

  const measuring = new PageProbe();
  measuring.loadingFonts();
  measuring.row(1000);
  await measuring.send();
  assert.equal(measuring.container.scrollTop, 800, 'initial scroll placement must precede the first visible frame');
  await measuring.paint();
  await measuring.paint();
  assert.deepEqual(measuring.presented, ['1:1:presented:1'], 'measuring transcript rows must not wait for font completion');
  assert.equal(measuring.fonts.status, 'loading');
  assert.equal(measuring.rows[0].style.contentVisibility, 'auto');

  const fonts = new PageProbe();
  await fonts.send();
  fonts.loadingFonts();
  await fonts.paint();
  await fonts.paint();
  assert.deepEqual(fonts.presented, ['1:1:presented:1'], 'font completion must not delay the first usable page layout');
  fonts.finishFonts();
  await fonts.flush();
  await fonts.paint();
  await fonts.paint();
  assert.deepEqual(fonts.presented, ['1:1:presented:1']);
  await fonts.send(() => {}, { epoch: '2' });
  await fonts.paint();
  await fonts.paint();
  assert.deepEqual(fonts.presented, ['1:1:presented:1', '2:1:presented:1'], 'rehydration needs a new presentation acknowledgement');

  const superseded = new PageProbe();
  await superseded.send();
  await superseded.send(() => {}, { revision: '2' });
  await superseded.paint();
  await superseded.paint();
  assert.deepEqual(superseded.presented, ['1:2:presented:2']);

  const hidden = new PageProbe();
  hidden.pending = [
    { closest: () => ({}) },
    { computed: { display: 'none', visibility: 'visible' } },
    { getClientRects: () => [] },
  ];
  await hidden.send();
  await hidden.paint();
  await hidden.paint();
  assert.equal(hidden.presented.length, 1, 'inert measurement and hidden regions must not hold navigation');

  const layout = new PageProbe();
  const first = layout.row(1000);
  await layout.send();
  assert.equal(first.style.contentVisibility, 'visible');
  await layout.paint();
  const appended = layout.row(800);
  await layout.send();
  await layout.paint();
  assert.equal(first.style.contentVisibility, 'auto');
  assert.equal(appended.style.contentVisibility, 'visible', 'new rows need their own layout opportunity');
  assert.equal(layout.presented.length, 1, 'a new row must not postpone presentation until streaming becomes stable');
  await layout.paint();
  await layout.paint();
  await layout.paint();
  await layout.paint();
  assert.equal(appended.style.contentVisibility, 'auto');
  assert.equal(layout.container.scrollTop, 1600);
  assert.equal(layout.presented.length, 1);

  const disposal = new PageProbe();
  const discarded = disposal.row(1000);
  await disposal.send();
  disposal.container.isConnected = false;
  await disposal.send();
  assert.equal(discarded.style.contentVisibility, 'auto');
  assert.ok(disposal.observers.some(observer => observer.disconnected));
  await disposal.paint();
  await disposal.paint();
  assert.equal(discarded.style.contentVisibility, 'auto', 'discarded measurement callbacks must not mutate the old document');
})().catch(error => { console.error(error); process.exitCode = 1; });
