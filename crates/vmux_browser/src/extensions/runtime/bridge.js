((config) => {
  const CHANNEL = __VMUX_BRIDGE_CHANNEL__;
  const KEEPALIVE_CHANNEL = __VMUX_KEEPALIVE_CHANNEL__;
  const PROTOCOL_VERSION = __VMUX_BRIDGE_PROTOCOL_VERSION__;
  const CONTEXT_ID = __VMUX_BRIDGE_CONTEXT_ID__;
  const MAX_FRAME_SIZE = __VMUX_BRIDGE_MAX_FRAME_SIZE__;
  const MAX_PENDING_FRAME_BYTES = __VMUX_BRIDGE_MAX_MESSAGE_SIZE__;
  const MAX_PENDING_FRAMES = 64;
  const MAX_PENDING_CALLBACKS = 256;
  const MAX_SUBSCRIPTIONS = 64;
  const MAX_INBOUND_FRAMES = 64;
  const MAX_INBOUND_BYTES = MAX_PENDING_FRAME_BYTES;
  const MAX_SOCKET_BUFFERED_BYTES = MAX_FRAME_SIZE;
  const REQUEST_TIMEOUT_MS = 30000;
  const WRITE_DEADLINE_MS = 5000;
  const WRITE_RETRY_MS = 25;
  const EVENT_DELIVERY_TIMEOUT_MS = 5000;
  const KEEPALIVE_LEASE_MS = 60000;
  const MIN_RECONNECT_DELAY_MS = 250;
  const MAX_RECONNECT_DELAY_MS = 30000;
  const MIN_RUNTIME_RETRY_DELAY_MS = 10;
  const MAX_RUNTIME_RETRY_DELAY_MS = 1000;
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
  const NativeWebSocket = globalThis.WebSocket;
  const NativePromise = globalThis.Promise;
  const nativePromiseResolve = NativePromise.resolve.bind(NativePromise);
  const nativeStringify = JSON.stringify;
  const nativeTextEncode = Function.prototype.call.bind(TextEncoder.prototype.encode);
  const textEncoder = new TextEncoder();
  const nativeWebSocketSend = Function.prototype.call.bind(NativeWebSocket.prototype.send);
  const nativeWebSocketClose = Function.prototype.call.bind(NativeWebSocket.prototype.close);
  const nativeBufferedAmount = Function.prototype.call.bind(
    Object.getOwnPropertyDescriptor(NativeWebSocket.prototype, "bufferedAmount").get,
  );
  const nativeAddEventListener = Function.prototype.call.bind(EventTarget.prototype.addEventListener);
  const nativeSetTimeout = globalThis.setTimeout.bind(globalThis);
  const nativeClearTimeout = globalThis.clearTimeout.bind(globalThis);
  const nativeRandomUUID = crypto.randomUUID.bind(crypto);
  const nativeRandom = Math.random.bind(Math);
  const nativeNow = Date.now.bind(Date);
  const WEBSOCKET_OPEN = 1;
  const WEBSOCKET_CLOSED = 3;

  const pendingFrames = [];
  const pendingCallbacks = new Map();
  const subscriptions = new Map();
  const inboundFrames = [];
  let socket = null;
  let keepalivePort = null;
  let keepaliveLeaseTimer = null;
  let keepaliveLeaseDeadline = 0;
  let reconnectTimer = null;
  let flushTimer = null;
  let reconnectDelay = MIN_RECONNECT_DELAY_MS;
  let pendingFrameBytes = 0;
  let inboundFrameBytes = 0;
  let drainingInbound = false;
  let connectionGeneration = 0;
  let ready = false;
  let stopped = false;
  let helloFrame = null;
  let nativeRuntimeConnect = null;
  let nativeRuntimeSendMessage = null;
  let nativeOnMessageAdd = null;
  let runtimeListenerInstalled = false;
  let runtimeRetryTimer = null;
  let runtimeRetryDelay = MIN_RUNTIME_RETRY_DELAY_MS;

  function responseError(message) {
    return { error: { code: "bridge_unavailable", message } };
  }

  function validIdentifier(value) {
    return typeof value === "string" && value.length > 0 && value.length <= 256;
  }

  function captureRuntimeBindings() {
    if (nativeRuntimeConnect && nativeRuntimeSendMessage && nativeOnMessageAdd) return true;
    const runtime = globalThis.chrome?.runtime;
    const connect = runtime?.connect;
    const sendMessage = runtime?.sendMessage;
    const onMessage = runtime?.onMessage;
    const addListener = onMessage?.addListener;
    if (
      typeof connect !== "function" ||
      typeof sendMessage !== "function" ||
      typeof addListener !== "function"
    ) {
      return false;
    }
    nativeRuntimeConnect = connect.bind(runtime);
    nativeRuntimeSendMessage = sendMessage.bind(runtime);
    nativeOnMessageAdd = addListener.bind(onMessage);
    return true;
  }

  function disconnectKeepalive() {
    const port = keepalivePort;
    keepalivePort = null;
    keepaliveLeaseDeadline = 0;
    if (keepaliveLeaseTimer) nativeClearTimeout(keepaliveLeaseTimer);
    keepaliveLeaseTimer = null;
    try {
      port?.disconnect();
    } catch (_error) {}
  }

  function pulseWorker() {
    if (nativeNow() >= keepaliveLeaseDeadline) return;
    if (!captureRuntimeBindings()) return;
    if (!keepalivePort) {
      try {
        const port = nativeRuntimeConnect({ name: KEEPALIVE_CHANNEL });
        keepalivePort = port;
        port.onDisconnect.addListener(() => {
          if (keepalivePort === port) keepalivePort = null;
        });
      } catch (_error) {
        keepalivePort = null;
      }
    }
    try {
      keepalivePort?.postMessage({ type: "heartbeat" });
    } catch (_error) {
      disconnectKeepalive();
    }
  }

  function leaseWorker() {
    keepaliveLeaseDeadline = nativeNow() + KEEPALIVE_LEASE_MS;
    pulseWorker();
    if (keepaliveLeaseTimer) nativeClearTimeout(keepaliveLeaseTimer);
    keepaliveLeaseTimer = nativeSetTimeout(disconnectKeepalive, KEEPALIVE_LEASE_MS);
  }

  function removePendingFrames(requestId) {
    for (let index = pendingFrames.length - 1; index >= 0; index -= 1) {
      if (pendingFrames[index].requestId !== requestId) continue;
      pendingFrameBytes -= pendingFrames[index].bytes;
      pendingFrames.splice(index, 1);
    }
  }

  function finishCallback(requestId, response) {
    const pending = pendingCallbacks.get(requestId);
    if (!pending) return;
    pendingCallbacks.delete(requestId);
    nativeClearTimeout(pending.timeout);
    removePendingFrames(requestId);
    try {
      pending.callback(response);
    } catch (_error) {}
  }

  function failPending(message) {
    if (flushTimer) nativeClearTimeout(flushTimer);
    flushTimer = null;
    pendingFrames.length = 0;
    pendingFrameBytes = 0;
    for (const requestId of [...pendingCallbacks.keys()]) {
      finishCallback(requestId, responseError(message));
    }
  }

  function requeuePendingRequests() {
    if (flushTimer) nativeClearTimeout(flushTimer);
    flushTimer = null;
    pendingFrames.length = 0;
    pendingFrameBytes = 0;
    for (const [requestId, pending] of pendingCallbacks) {
      transmit(pending.frame, requestId);
    }
  }

  function encode(frame) {
    const text = nativeStringify(frame);
    const bytes = nativeTextEncode(textEncoder, text).byteLength;
    if (bytes > MAX_FRAME_SIZE) {
      throw new Error("extension bridge frame exceeds size limit");
    }
    return { text, bytes };
  }

  function transmit(frame, requestId) {
    let encoded;
    try {
      encoded = encode(frame);
    } catch (error) {
      if (requestId) finishCallback(requestId, responseError(String(error)));
      return false;
    }
    if (
      pendingFrames.length >= MAX_PENDING_FRAMES ||
      pendingFrameBytes + encoded.bytes > MAX_PENDING_FRAME_BYTES
    ) {
      if (requestId) {
        finishCallback(requestId, responseError("extension bridge queue is full"));
      } else if (socket && socket.readyState !== WEBSOCKET_CLOSED) {
        nativeWebSocketClose(socket, 1013, "extension bridge queue is full");
      }
      return false;
    }
    pendingFrames.push({
      ...encoded,
      requestId,
      deadline: nativeNow() + WRITE_DEADLINE_MS,
    });
    pendingFrameBytes += encoded.bytes;
    if (ready && socket) flushPending(socket);
    return true;
  }

  function scheduleFlush(current) {
    if (flushTimer || socket !== current || !ready) return;
    flushTimer = nativeSetTimeout(() => {
      flushTimer = null;
      flushPending(current);
    }, WRITE_RETRY_MS);
  }

  function flushPending(current) {
    if (socket !== current || !ready || current.readyState !== WEBSOCKET_OPEN) return;
    if (flushTimer) nativeClearTimeout(flushTimer);
    flushTimer = null;
    while (pendingFrames.length && ready && current.readyState === WEBSOCKET_OPEN) {
      const frame = pendingFrames[0];
      if (nativeNow() >= frame.deadline) {
        if (frame.requestId) {
          finishCallback(frame.requestId, responseError("extension bridge write timed out"));
        }
        nativeWebSocketClose(current, 1013, "extension bridge write timed out");
        return;
      }
      if (nativeBufferedAmount(current) > MAX_SOCKET_BUFFERED_BYTES) {
        scheduleFlush(current);
        return;
      }
      try {
        nativeWebSocketSend(current, frame.text);
        pendingFrames.shift();
        pendingFrameBytes -= frame.bytes;
      } catch (_error) {
        scheduleFlush(current);
        return;
      }
    }
  }

  function callerContext(sender) {
    if (!sender || sender.id !== config.extension) return null;
    const tabId = Number.isInteger(sender.tab?.id) ? sender.tab.id : null;
    const frameId = Number.isInteger(sender.frameId) ? sender.frameId : 0;
    const documentId = typeof sender.documentId === "string" ? sender.documentId : null;
    const senderUrl = typeof sender.url === "string" ? sender.url : null;
    const extensionOrigin = `chrome-extension://${config.extension}/`;
    if (documentId && senderUrl?.startsWith(extensionOrigin)) {
      return {
        context_kind: "extension_page",
        extension_id: sender.id,
        context_id: documentId,
        url: senderUrl,
        document_id: documentId,
      };
    }
    if (tabId !== null && senderUrl) {
      return {
        context_kind: "content_script",
        extension_id: sender.id,
        context_id: documentId || `tab:${tabId}:frame:${frameId}`,
        url: senderUrl,
        tab_id: tabId,
        frame_id: frameId,
        document_id: documentId,
      };
    }
    if (tabId === null && !documentId && senderUrl?.startsWith(extensionOrigin)) {
      return {
        context_kind: "service_worker",
        extension_id: sender.id,
        context_id: senderUrl,
        url: senderUrl,
      };
    }
    return null;
  }

  function scheduleReconnect() {
    if (stopped || reconnectTimer) return;
    const delay = reconnectDelay * (0.8 + nativeRandom() * 0.4);
    reconnectDelay = Math.min(MAX_RECONNECT_DELAY_MS, reconnectDelay * 2);
    reconnectTimer = nativeSetTimeout(() => {
      reconnectTimer = null;
      connect();
    }, delay);
  }

  function withTimeout(promise, timeoutMs) {
    return new NativePromise((resolve, reject) => {
      const timeout = nativeSetTimeout(() => reject(new Error("operation timed out")), timeoutMs);
      nativePromiseResolve(promise).then(
        (value) => {
          nativeClearTimeout(timeout);
          resolve(value);
        },
        (error) => {
          nativeClearTimeout(timeout);
          reject(error);
        },
      );
    });
  }

  async function deliverRuntimeEvent(payload) {
    if (!captureRuntimeBindings()) return false;
    const event = {
      channel: CHANNEL,
      type: "event",
      namespace: payload.namespace,
      event: payload.event,
      arguments: payload.arguments,
      sequence: payload.sequence,
    };
    try {
      const delivery = await withTimeout(
        nativeRuntimeSendMessage(event),
        EVENT_DELIVERY_TIMEOUT_MS,
      );
      if (delivery?.ok === true) return true;
    } catch (_error) {}
    leaseWorker();
    try {
      const delivery = await withTimeout(
        nativeRuntimeSendMessage(event),
        EVENT_DELIVERY_TIMEOUT_MS,
      );
      return delivery?.ok === true;
    } catch (_error) {
      return false;
    }
  }

  function currentSession(current, generation) {
    return socket === current && connectionGeneration === generation && !stopped;
  }

  async function handleServerMessage(current, event, generation) {
    let message;
    try {
      message = JSON.parse(event.data);
    } catch (_error) {
      nativeWebSocketClose(current, 1002, "invalid bridge message");
      return;
    }
    if (message.type === "ready") {
      if (message.payload?.protocol_version !== PROTOCOL_VERSION) {
        stopped = true;
        nativeWebSocketClose(current, 1002, "bridge protocol mismatch");
        return;
      }
      ready = true;
      reconnectDelay = MIN_RECONNECT_DELAY_MS;
      for (const subscription of subscriptions.values()) transmit(subscription);
      flushPending(current);
      return;
    }
    if (message.type === "heartbeat") {
      if (nativeNow() < keepaliveLeaseDeadline) pulseWorker();
      return;
    }
    if (message.type === "fatal") {
      stopped = true;
      failPending(message.payload?.message || "extension bridge failed");
      disconnectKeepalive();
      nativeWebSocketClose(current, 1008, "extension bridge failed");
      return;
    }
    if (!ready) {
      nativeWebSocketClose(current, 1002, "bridge message before ready");
      return;
    }
    if (message.type === "response") {
      finishCallback(message.payload.request_id, {
        result: message.payload.result,
        error: message.payload.error,
      });
      return;
    }
    if (message.type === "event") {
      const delivered = await deliverRuntimeEvent(message.payload);
      if (delivered && currentSession(current, generation)) {
        transmit({ type: "ack", payload: { sequence: message.payload.sequence } });
      }
    }
  }

  async function drainInbound() {
    if (drainingInbound) return;
    drainingInbound = true;
    try {
      while (inboundFrames.length) {
        const item = inboundFrames.shift();
        inboundFrameBytes -= item.bytes;
        if (!currentSession(item.current, item.generation)) continue;
        await handleServerMessage(item.current, item.event, item.generation);
      }
    } finally {
      drainingInbound = false;
    }
  }

  function enqueueInbound(current, event, generation) {
    if (typeof event.data !== "string") {
      nativeWebSocketClose(current, 1002, "invalid bridge frame type");
      return;
    }
    const bytes = nativeTextEncode(textEncoder, event.data).byteLength;
    if (
      bytes > MAX_FRAME_SIZE ||
      inboundFrames.length >= MAX_INBOUND_FRAMES ||
      inboundFrameBytes + bytes > MAX_INBOUND_BYTES
    ) {
      nativeWebSocketClose(current, 1009, "extension bridge inbound queue is full");
      return;
    }
    inboundFrames.push({ current, event, generation, bytes });
    inboundFrameBytes += bytes;
    void drainInbound();
  }

  function connect() {
    if (stopped) return;
    const generation = ++connectionGeneration;
    const current = new NativeWebSocket(config.endpoint);
    socket = current;
    ready = false;
    nativeAddEventListener(current, "open", () => {
      if (!currentSession(current, generation)) return;
      nativeWebSocketSend(current, helloFrame);
    }, { once: true });
    nativeAddEventListener(current, "message", (event) => {
      if (currentSession(current, generation)) enqueueInbound(current, event, generation);
    });
    nativeAddEventListener(current, "error", () => {
      if (socket === current && current.readyState !== WEBSOCKET_CLOSED) {
        nativeWebSocketClose(current);
      }
    });
    nativeAddEventListener(current, "close", () => {
      if (socket !== current) return;
      socket = null;
      ready = false;
      disconnectKeepalive();
      requeuePendingRequests();
      scheduleReconnect();
    });
  }

  helloFrame = encode({
    type: "hello",
    payload: {
      protocol_version: PROTOCOL_VERSION,
      extension_id: config.extension,
      profile_id: config.profile,
      token: config.token,
      context_id: CONTEXT_ID,
      context_kind: "bridge_page",
    },
  }).text;
  config.token = "";

  function handleRuntimeMessage(message, sender, sendResponse) {
    if (!message || message.channel !== CHANNEL) return undefined;
    const caller = callerContext(sender);
    if (!caller) {
      sendResponse(responseError("extension bridge sender is not authorized"));
      return false;
    }
    if (message.type === "subscribe") {
      if (
        !validIdentifier(message.subscriptionId) ||
        !validIdentifier(message.namespace) ||
        !validIdentifier(message.event)
      ) {
        sendResponse(responseError("invalid extension bridge subscription"));
        return false;
      }
      if (!subscriptions.has(message.subscriptionId) && subscriptions.size >= MAX_SUBSCRIPTIONS) {
        sendResponse(responseError("extension bridge subscription limit reached"));
        return false;
      }
      const frame = {
        type: "subscribe",
        payload: {
          subscription_id: message.subscriptionId,
          namespace: message.namespace,
          event: message.event,
          caller_context: caller,
        },
      };
      subscriptions.set(message.subscriptionId, frame);
      if (ready) transmit(frame);
      sendResponse({ accepted: true });
      return false;
    }
    if (message.type === "unsubscribe") {
      if (!validIdentifier(message.subscriptionId)) {
        sendResponse(responseError("invalid extension bridge subscription"));
        return false;
      }
      subscriptions.delete(message.subscriptionId);
      if (ready) {
        transmit({
          type: "unsubscribe",
          payload: { subscription_id: message.subscriptionId, caller_context: caller },
        });
      }
      sendResponse({ accepted: true });
      return false;
    }
    if (message.type !== "api_request") return undefined;
    if (!validIdentifier(message.namespace) || !validIdentifier(message.method)) {
      sendResponse(responseError("invalid extension bridge request"));
      return false;
    }
    if (pendingCallbacks.size >= MAX_PENDING_CALLBACKS) {
      sendResponse(responseError("extension bridge callback queue is full"));
      return false;
    }
    const requestId = validIdentifier(message.requestId) ? message.requestId : nativeRandomUUID();
    if (pendingCallbacks.has(requestId)) {
      sendResponse(responseError("duplicate extension bridge request id"));
      return false;
    }
    const frame = {
      type: "api_request",
      payload: {
        request_id: requestId,
        namespace: message.namespace,
        method: message.method,
        arguments: message.arguments,
        caller_context: caller,
      },
    };
    const timeout = nativeSetTimeout(() => {
      finishCallback(requestId, responseError("extension bridge request timed out"));
    }, REQUEST_TIMEOUT_MS);
    pendingCallbacks.set(requestId, { callback: sendResponse, timeout, frame });
    transmit(frame, requestId);
    return true;
  }

  function start() {
    if (stopped || runtimeListenerInstalled) return;
    if (!captureRuntimeBindings()) {
      if (!runtimeRetryTimer) {
        const delay = runtimeRetryDelay;
        runtimeRetryDelay = Math.min(MAX_RUNTIME_RETRY_DELAY_MS, runtimeRetryDelay * 2);
        runtimeRetryTimer = nativeSetTimeout(() => {
          runtimeRetryTimer = null;
          start();
        }, delay);
      }
      return;
    }
    try {
      nativeOnMessageAdd(handleRuntimeMessage);
    } catch (_error) {
      nativeRuntimeConnect = null;
      nativeRuntimeSendMessage = null;
      nativeOnMessageAdd = null;
      runtimeRetryDelay = MIN_RUNTIME_RETRY_DELAY_MS;
      runtimeRetryTimer = nativeSetTimeout(() => {
        runtimeRetryTimer = null;
        start();
      }, runtimeRetryDelay);
      return;
    }
    runtimeListenerInstalled = true;
    connect();
  }

  nativeAddEventListener(globalThis, "pagehide", () => {
    stopped = true;
    if (runtimeRetryTimer) nativeClearTimeout(runtimeRetryTimer);
    runtimeRetryTimer = null;
    if (reconnectTimer) nativeClearTimeout(reconnectTimer);
    reconnectTimer = null;
    failPending("extension bridge page closed");
    disconnectKeepalive();
    if (socket) nativeWebSocketClose(socket);
  }, { once: true });

  start();
})(__VMUX_BRIDGE_CONFIG__);
