importScripts("config.js");

const observations = [];
const internalObservations = [];

function observe(key, value) {
  observations.push({ key, value });
}

function observeInternal(key, value) {
  internalObservations.push({ key, value });
}

async function postCapture(observationValues, internalValues) {
  const response = await fetch(globalThis.VMUX_CONFORMANCE.collector, {
    method: "POST",
    body: JSON.stringify({
      target: globalThis.VMUX_CONFORMANCE.target,
      chromium_major: 148,
      observations: observationValues,
      internal_observations: internalValues,
    }),
  });
  if (!response.ok) throw new Error(`capture POST failed: ${response.status}`);
}

function withTimeout(promise, timeoutMs) {
  let timeout;
  const expired = new Promise((_, reject) => {
    timeout = setTimeout(() => reject(new Error("runtime message attempt timed out")), timeoutMs);
  });
  return Promise.race([promise, expired]).finally(() => clearTimeout(timeout));
}

async function runtimeRoundTrip() {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    try {
      const response = await withTimeout(
        chrome.runtime.sendMessage({ type: "__vmux_conformance_ping" }),
        500,
      );
      if (response === "pong") return response;
    } catch (_error) {}
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("runtime message round-trip timed out");
}

if (globalThis.__vmuxExtensionRuntime) {
  globalThis.__vmuxExtensionRuntime.register(
    "__vmux_conformance",
    "modelChanged",
    async (snapshot) => {
      if (!snapshot || !Array.isArray(snapshot.windows) || !Array.isArray(snapshot.tabs)) {
        return;
      }
      await postCapture([], [{ key: "worker.wakeEvent", value: true }]);
    },
  );
}

async function run() {
  await chrome.storage.local.set({ value: "value" });
  const stored = await chrome.storage.local.get("value");
  observe("runtime.id.length", chrome.runtime.id.length);
  observe("storage.local.roundTrip", stored.value);
  observe("runtime.message.roundTrip", await runtimeRoundTrip());

  if (globalThis.__vmuxExtensionRuntime) {
    try {
      const snapshot = await globalThis.__vmuxExtensionRuntime.request(
        "__vmux_conformance",
        "snapshot",
        {},
      );
      observeInternal("bridge.connected", true);
      observeInternal("bridge.tabCount", snapshot.tabs.length);
    } catch (_error) {
      observeInternal("bridge.connected", false);
    }
  }

  const sent = await chrome.storage.local.get("captureSent");
  if (!sent.captureSent) {
    await chrome.storage.local.set({ captureSent: true });
    await postCapture(observations, internalObservations);
  }
}

run().catch(async (error) => {
  await postCapture(observations, [
    ...internalObservations,
    { key: "worker.error", value: String(error && error.message ? error.message : error) },
  ]);
});
