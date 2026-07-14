(() => {
  const CHANNEL = "__vmux_extension_bridge_v1";
  const listeners = new Map();
  chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (!message || message.channel !== CHANNEL) return undefined;
    if (message.type === "event") {
      const handlers = listeners.get(`${message.namespace}.${message.event}`) || [];
      for (const handler of handlers) handler(...message.arguments);
      sendResponse({ ok: true, sequence: message.sequence });
      return true;
    }
    return undefined;
  });
  globalThis.__vmuxExtensionRuntime = {
    channel: CHANNEL,
    register(namespace, event, handler) {
      const key = `${namespace}.${event}`;
      const handlers = listeners.get(key) || [];
      handlers.push(handler);
      listeners.set(key, handlers);
      chrome.runtime.sendMessage({
        channel: CHANNEL,
        type: "subscribe",
        subscriptionId: key,
        namespace,
        event,
      });
    },
    request(namespace, method, argumentsValue) {
      return chrome.runtime.sendMessage({
        channel: CHANNEL,
        type: "api_request",
        requestId: crypto.randomUUID(),
        namespace,
        method,
        arguments: argumentsValue,
      }).then((response) => {
        if (response && response.error) {
          throw new Error(response.error.message);
        }
        return response ? response.result : undefined;
      });
    },
  };
})();
