(() => {
  const CHANNEL = "__vmux_extension_bridge_v1";
  const params = new URLSearchParams(location.search);
  const socket = new WebSocket(params.get("endpoint"));
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
        extension_id: params.get("extension"),
        profile_id: params.get("profile"),
        token: params.get("token"),
        context_id: "bridge-page",
        context_kind: "bridge_page",
      },
    });
    while (pendingFrames.length) socket.send(pendingFrames.shift());
  });

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
