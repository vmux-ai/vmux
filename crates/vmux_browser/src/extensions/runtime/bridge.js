(() => {
  const CHANNEL = "__vmux_extension_bridge_v1";
  const config = globalThis.__vmuxBridgeConfig;
  delete globalThis.__vmuxBridgeConfig;
  if (
    !config ||
    typeof config.endpoint !== "string" ||
    typeof config.extension !== "string" ||
    typeof config.profile !== "string" ||
    typeof config.token !== "string" ||
    !config.endpoint ||
    !config.extension ||
    !config.profile ||
    !config.token
  ) {
    throw new Error("missing vmux bridge configuration");
  }
  let token = config.token;
  const socket = new WebSocket(config.endpoint);
  const pendingFrames = [];
  const pendingCallbacks = new Map();

  function send(frame) {
    const encoded = JSON.stringify(frame);
    if (socket.readyState === WebSocket.OPEN) socket.send(encoded);
    else pendingFrames.push(encoded);
  }

  socket.addEventListener("open", () => {
    send({
      type: "hello",
      payload: {
        protocol_version: 1,
        extension_id: config.extension,
        profile_id: config.profile,
        token,
        context_id: "bridge-page",
        context_kind: "bridge_page",
      },
    });
    token = "";
    while (pendingFrames.length) socket.send(pendingFrames.shift());
  }, { once: true });

  socket.addEventListener("message", async (event) => {
    const message = JSON.parse(event.data);
    if (message.type === "response") {
      const callback = pendingCallbacks.get(message.payload.request_id);
      if (callback) {
        pendingCallbacks.delete(message.payload.request_id);
        callback({ result: message.payload.result, error: message.payload.error });
      }
      return;
    }
    if (message.type === "event") {
      const delivery = await chrome.runtime.sendMessage({
        channel: CHANNEL,
        type: "event",
        namespace: message.payload.namespace,
        event: message.payload.event,
        arguments: message.payload.arguments,
        sequence: message.payload.sequence,
      });
      if (delivery && delivery.ok === true) {
        send({ type: "ack", payload: { sequence: message.payload.sequence } });
      }
    }
  });

  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (!message || message.channel !== CHANNEL) {
      return undefined;
    }
    if (message.type === "subscribe") {
      send({
        type: "subscribe",
        payload: {
          subscription_id: message.subscriptionId,
          namespace: message.namespace,
          event: message.event,
        },
      });
      sendResponse({ accepted: true });
      return false;
    }
    if (message.type !== "api_request") return undefined;
    pendingCallbacks.set(message.requestId, sendResponse);
    send({
      type: "api_request",
      payload: {
        request_id: message.requestId,
        namespace: message.namespace,
        method: message.method,
        arguments: message.arguments,
      },
    });
    return true;
  });
})();
