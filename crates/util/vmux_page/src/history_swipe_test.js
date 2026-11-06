const vm = require('node:vm');
const fs = require('node:fs');
const assert = require('node:assert/strict');
const script = fs.readFileSync(0, 'utf8');

class GestureProbe {
  constructor() {
    this.now = 0;
    this.handlers = new Map();
    const probe = this;
    this.Element = class {
      constructor(scrollLeft = 0, overflowX = 'auto') {
        this.scrollLeft = scrollLeft;
        this.scrollWidth = 500;
        this.clientWidth = 200;
        this.overflowX = overflowX;
      }
      matches() { return false; }
    };
    this.scope = {
      Element: this.Element, navigations: [],
      performance: { now: () => this.now },
      getComputedStyle: node => ({ overflowX: node.overflowX }),
      document: {
        scrollingElement: null,
        addEventListener: (name, handler) => this.handlers.set(name, handler),
        removeEventListener: name => this.handlers.delete(name),
      },
      addEventListener() {}, removeEventListener() {},
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
}

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

const protectedInput = new GestureProbe();
const input = new protectedInput.Element();
input.matches = () => true;
protectedInput.wheel(-240, 0, [input]);
protectedInput.expect();

const scrolling = new GestureProbe();
scrolling.wheel(-240, 0, [new scrolling.Element(50)]);
scrolling.expect();
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
