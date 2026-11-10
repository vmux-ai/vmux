const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { pathToFileURL } = require("node:url");
const { spawn, spawnSync } = require("node:child_process");
const { once } = require("node:events");
const assert = require("node:assert/strict");
const shim = fs.readFileSync(0, "utf8");

function scenario() {
  const box = element => {
    const rect = element.getBoundingClientRect();
    return { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
  };
  const source = box(document.querySelector(".vmux-agent-composer-shared"));
  let animation;
  let polls = 0;
  let resolve;
  window.__composerProbe = new Promise(done => { resolve = done; });
  window.ipc = { postMessage() {} };
  window.__vmuxFrameCapability = 777;
  const nativeAnimate = Element.prototype.animate;
  Element.prototype.animate = function (frames, options) {
    const result = nativeAnimate.call(this, frames, options);
    if (this.classList.contains("vmux-agent-composer-shared")) {
      result.pause();
      result.currentTime = 0;
      animation = result;
    }
    return result;
  };
  window.vmuxCreateInterpreter = () => ({
    initialize(root) { this.root = root; },
    sendSerializedEvent() {},
    run_from_bytes() {
      this.root.innerHTML = '<div class="vmux-agent-composer-shared" style="position:absolute;left:120px;top:730px;width:760px;height:130px"></div>';
    },
  });
  window.interpreter = { root: document.getElementById("root"), sendSerializedEvent() {}, run_from_bytes() {} };
  window.fetch = async (_, options) => {
    if (++polls === 1) {
      return new Response(null, { status: 412, headers: {
        "x-vmux-navigation": "1", "x-vmux-frame-epoch": "1", "x-vmux-frame-capability": "777",
      } });
    }
    if (polls === 2) {
      const requests = new TextEncoder().encode(JSON.stringify([{ kind: "remount", revision: 2, epoch: 1 }]));
      const frame = new Uint8Array(requests.length + 5);
      new DataView(frame.buffer).setUint32(0, requests.length, true);
      frame.set(requests, 4);
      frame[frame.length - 1] = 1;
      return new Response(frame, { headers: {
        "x-vmux-navigation": "2", "x-vmux-frame-epoch": "1", "x-vmux-frame-capability": "777", "x-vmux-frame": "1",
      } });
    }
    const composer = document.querySelector(".vmux-agent-composer-shared");
    if (!animation) {
      resolve({ error: "the composer did not animate", source });
      return new Promise(() => {});
    }
    const start = box(composer);
    animation.currentTime = animation.effect.getTiming().duration / 2;
    const middle = box(composer);
    animation.currentTime = 0;
    animation.play();
    animation.finished.then(() => resolve({
      source, start, middle, end: box(composer), ack: options.headers["x-vmux-applied-frame"],
    }));
    return new Promise(() => {});
  };
}

class BrowserProbe {
  constructor(binary, profile) {
    this.pending = new Map();
    this.listeners = [];
    this.sequence = 0;
    this.buffer = "";
    this.stderr = "";
    this.child = spawn(binary, [
      "--headless", "--disable-gpu", "--disable-background-networking", "--no-first-run",
      "--no-default-browser-check", "--remote-debugging-pipe", "--window-size=1400,1100",
      "--user-data-dir=" + profile, "about:blank",
    ], { stdio: ["ignore", "ignore", "pipe", "pipe", "pipe"] });
    this.child.stderr.on("data", chunk => { this.stderr = (this.stderr + chunk).slice(-4000); });
    this.child.stdio[4].on("data", chunk => {
      this.buffer += chunk.toString();
      let delimiter;
      while ((delimiter = this.buffer.indexOf("\0")) >= 0) {
        const message = JSON.parse(this.buffer.slice(0, delimiter));
        this.buffer = this.buffer.slice(delimiter + 1);
        if (message.id) {
          const pending = this.pending.get(message.id);
          if (!pending) continue;
          this.pending.delete(message.id);
          if (message.error) pending.reject(new Error(JSON.stringify(message.error)));
          else pending.resolve(message.result);
        } else {
          for (const listener of [...this.listeners]) {
            if (listener.method === message.method && listener.session === message.sessionId) {
              this.listeners.splice(this.listeners.indexOf(listener), 1);
              listener.resolve(message.params);
            }
          }
        }
      }
    });
    this.child.on("error", error => this.fail(error));
    this.child.on("exit", () => this.fail(new Error("Chrome exited: " + this.stderr)));
  }
  send(method, params = {}, sessionId) {
    const id = ++this.sequence;
    const result = new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
    this.child.stdio[3].write(JSON.stringify({ id, method, params, sessionId }) + "\0");
    return result;
  }
  event(method, session) {
    return new Promise((resolve, reject) => this.listeners.push({ method, session, resolve, reject }));
  }
  fail(error) {
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
    for (const listener of this.listeners) listener.reject(error);
    this.listeners.length = 0;
  }
  async close() {
    if (this.child.pid && this.child.exitCode === null && this.child.signalCode === null) {
      const exited = once(this.child, "exit");
      this.child.kill();
      await exited;
    }
  }
}

(async () => {
  const candidates = process.platform === "darwin"
    ? ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "chromium"]
    : ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"];
  if (process.env.CHROME_BIN) candidates.unshift(process.env.CHROME_BIN);
  let binary;
  for (const candidate of candidates) {
    if (spawnSync(candidate, ["--version"], { timeout: 5000 }).status === 0) {
      binary = candidate;
      break;
    }
  }
  assert.ok(binary, "Chrome or Chromium is required for the native browser layout test (CHROME_BIN can select it)");
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "vmux-composer-browser-"));
  const fixture = path.join(temporary, "fixture.html");
  fs.writeFileSync(fixture, '<!doctype html><html><body style="margin:0"><div id="root"><div class="vmux-agent-composer-shared" style="position:absolute;left:180px;top:320px;width:640px;height:110px"></div></div><script>('
    + scenario.toString() + ')();</script><script>'
    + shim.replace(/<\/script/gi, "<\\/script")
    + '</script><script>vmuxWry.start();</script></body></html>');
  const browser = new BrowserProbe(binary, path.join(temporary, "profile"));
  const timeout = setTimeout(() => { browser.child.kill(); }, 15000);
  try {
    const target = await browser.send("Target.createTarget", { url: "about:blank" });
    const attached = await browser.send("Target.attachToTarget", { targetId: target.targetId, flatten: true });
    const session = attached.sessionId;
    await browser.send("Page.enable", {}, session);
    await browser.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] }, session);
    const loaded = browser.event("Page.loadEventFired", session);
    await browser.send("Page.navigate", { url: pathToFileURL(fixture).href }, session);
    await loaded;
    const evaluation = await browser.send("Runtime.evaluate", { expression: "window.__composerProbe", awaitPromise: true, returnByValue: true }, session);
    assert.ok(!evaluation.exceptionDetails, JSON.stringify(evaluation.exceptionDetails));
    const result = evaluation.result.value;
    assert.ok(!result.error, JSON.stringify(result));
    assert.equal(result.ack, "1", "navigation must acknowledge before the native animation finishes");
    for (const key of ["left", "top", "width", "height"]) {
      assert.ok(Math.abs(result.start[key] - result.source[key]) < 0.5, JSON.stringify(result));
    }
    assert.ok(result.middle.top > result.source.top && result.middle.top < 730, JSON.stringify(result));
    assert.deepEqual(result.end, { left: 120, top: 730, width: 760, height: 130 });
  } finally {
    clearTimeout(timeout);
    await browser.close();
    fs.rmSync(temporary, { recursive: true, force: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
