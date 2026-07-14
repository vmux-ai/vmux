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
  await fetch(globalThis.VMUX_CONFORMANCE.collector, {
    method: "POST",
    body: JSON.stringify({
      target: globalThis.VMUX_CONFORMANCE.target,
      chromium_major: 148,
      observations: observationValues,
      internal_observations: internalValues,
    }),
  });
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message === "ping") {
    sendResponse("pong");
  }
  return false;
});

if (globalThis.__vmuxExtensionRuntime) {
  globalThis.__vmuxExtensionRuntime.register(
    "__vmux_conformance",
    "modelChanged",
    async () => {
      await postCapture([], [{ key: "worker.wakeEvent", value: true }]);
    },
  );
}

async function run() {
  await chrome.storage.local.set({ value: "value" });
  const stored = await chrome.storage.local.get("value");
  observe("runtime.id.length", chrome.runtime.id.length);
  observe("storage.local.roundTrip", stored.value);
  observe("runtime.message.roundTrip", await chrome.runtime.sendMessage("ping"));

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

run();
