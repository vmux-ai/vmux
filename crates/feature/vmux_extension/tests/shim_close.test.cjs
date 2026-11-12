const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const patch = fs.readFileSync(path.join(__dirname, "../src/host/shim.js"), "utf8")
  .replaceAll("__VMUX_BRIDGE_CHANNEL__", JSON.stringify("bridge"))
  .replaceAll("__VMUX_KEEPALIVE_CHANNEL__", JSON.stringify("keepalive"));

class Popup {
  constructor({ hosted, id = 1000000001, popupId = 1000000001, bridge = true, reject = false }) {
    this.calls = [];
    this.emitted = [];
    this.nativeCloses = 0;
    const context = {
      URL,
      chrome: { runtime: { getURL: (file) => `chrome-extension://extension/${file}` }, windows: {} },
      __vmuxHostedPopup: hosted,
      __vmuxPopupWindowId: popupId,
      __vmuxExtensionRuntime: {
        request: (namespace, method, args) => {
          this.calls.push([namespace, method, args]);
          if (reject) return Promise.reject(new Error("unavailable"));
          return Promise.resolve(method === "getCurrent" ? { id } : undefined);
        },
      },
      nativeClose: () => { this.nativeCloses += 1; },
    };
    if (bridge) context.cef = { emit: (event) => this.emitted.push(event) };
    this.context = vm.createContext(context);
    vm.runInContext("window = globalThis; close = nativeClose;", this.context);
    vm.runInContext(patch, this.context);
  }

  async close() {
    this.context.close();
    await new Promise(setImmediate);
  }
}

test("hosted popup closes through its owning entity even with a browser window ID", async () => {
  const popup = new Popup({ hosted: true, id: 1 });
  await popup.close();
  assert.equal(popup.calls.length, 0);
  assert.equal(popup.nativeCloses, 0);
  assert.equal(popup.emitted.length, 1);
  assert.equal(popup.emitted[0].channel, "vmux-extension-popup-close");
  assert.equal(popup.emitted[0].window_id, 1000000001);
});

test("standalone extension window retains window API close", async () => {
  const popup = new Popup({ hosted: false });
  await popup.close();
  assert.deepEqual(popup.calls.map((call) => call[1]), ["getCurrent", "remove"]);
  assert.equal(popup.calls[1][2][0], 1000000001);
  assert.equal(popup.emitted.length, 0);
});

test("ordinary browser window cannot be closed by an extension page", async () => {
  const popup = new Popup({ hosted: false, id: 1 });
  await popup.close();
  assert.deepEqual(popup.calls.map((call) => call[1]), ["getCurrent"]);
  assert.equal(popup.nativeCloses, 0);
  assert.equal(popup.emitted.length, 0);
});

test("missing host bridge retains native close fallback", async () => {
  const popup = new Popup({ hosted: true, bridge: false, reject: true });
  await popup.close();
  assert.equal(popup.nativeCloses, 1);
  assert.equal(popup.emitted.length, 0);
});
