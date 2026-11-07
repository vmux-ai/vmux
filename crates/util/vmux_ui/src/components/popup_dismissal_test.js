const fs = require('node:fs');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const source = fs.readFileSync(0, 'utf8');
const windowHandlers = new Map();
const documentHandlers = new Map();
const events = [];
const scope = {
  Map,
  document: {
    visibilityState: 'visible',
    addEventListener(name, handler) { documentHandlers.set(name, handler); },
  },
  addEventListener(name, handler) {
    assert.equal(windowHandlers.has(name), false, 'the shared listener must be installed only once');
    windowHandlers.set(name, handler);
  },
};
scope.window = scope;
scope.globalThis = scope;
vm.runInNewContext(source, scope);
vm.runInNewContext(source, scope);
const browserWindow = vm.runInNewContext('window', scope);
const dismissal = scope.__vmuxPopupDismissal;
dismissal.subscribe(1, () => events.push('command'));
dismissal.subscribe(2, () => events.push('dropdown'));
windowHandlers.get('blur')({ target: {} });
documentHandlers.get('visibilitychange')();
assert.deepEqual(events, [], 'moving focus between fields must preserve menus');
windowHandlers.get('blur')({ target: browserWindow });
assert.deepEqual(events, ['command', 'dropdown']);
dismissal.remove(1);
events.length = 0;
windowHandlers.get('vmux-deactivated')();
assert.deepEqual(events, ['dropdown'], 'native application deactivation must use the same listeners');
events.length = 0;
scope.document.visibilityState = 'hidden';
documentHandlers.get('visibilitychange')();
assert.deepEqual(events, ['dropdown']);
events.length = 0;
dismissal.remove(2);
windowHandlers.get('pagehide')();
assert.deepEqual(events, [], 'unmounted controls must not retain dismissal callbacks');
dismissal.subscribe(2, () => events.push('reopened'));
 dismissal.subscribe(3, () => { throw new Error('disposed evaluation'); });
windowHandlers.get('blur')({ target: browserWindow });
assert.deepEqual(events, ['reopened'], 'reopened menus must remain dismissible');
events.length = 0;
windowHandlers.get('vmux-deactivated')();
assert.deepEqual(events, ['reopened'], 'a discarded listener must not prevent other popups from closing');
