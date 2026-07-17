(() => {
  const CHANNEL = __VMUX_BRIDGE_CHANNEL__;
  const KEEPALIVE_CHANNEL = __VMUX_KEEPALIVE_CHANNEL__;
  const BRIDGE_URL = chrome.runtime.getURL("vmux_bridge.html");
  const listeners = new Map();
  const subscriptionRetries = new Map();
  const deliveredSequences = new Set();
  const deliveredSequenceOrder = [];
  const nativeSendMessage = chrome.runtime.sendMessage.bind(chrome.runtime);
  const nativeRandomUUID = crypto.randomUUID.bind(crypto);
  const nativeSetTimeout = globalThis.setTimeout.bind(globalThis);
  const nativeClearTimeout = globalThis.clearTimeout.bind(globalThis);
  const nativeMin = Math.min.bind(Math);
  const nativeOnConnectAdd = chrome.runtime.onConnect.addListener.bind(chrome.runtime.onConnect);
  const nativeOnConnectRemove = chrome.runtime.onConnect.removeListener.bind(chrome.runtime.onConnect);
  const nativeOnConnectHas = chrome.runtime.onConnect.hasListener.bind(chrome.runtime.onConnect);
  const onConnectWrappers = new WeakMap();
  const nativeOnMessageAdd = chrome.runtime.onMessage.addListener.bind(chrome.runtime.onMessage);
  const nativeOnMessageRemove = chrome.runtime.onMessage.removeListener.bind(chrome.runtime.onMessage);
  const nativeOnMessageHas = chrome.runtime.onMessage.hasListener.bind(chrome.runtime.onMessage);
  const onMessageWrappers = new WeakMap();
  let lastTab = null;

  function webUrl(value) {
    return typeof value === "string" && /^(https?|file):/.test(value) ? value : null;
  }

  function rememberTab(sender) {
    if (sender?.tab && typeof sender.tab.id === "number" && webUrl(sender.tab.url)) {
      lastTab = sender.tab;
    }
  }

  function senderWithTab(message, sender) {
    rememberTab(sender);
    if (sender?.tab) return sender;
    const url = webUrl(message?.__vmuxSenderUrl) || webUrl(sender?.url);
    const useLastTab = message?.command === "triggerAutofillScriptInjection" && lastTab;
    if (!url && !useLastTab) return sender;
    const tab =
      lastTab && (!url || lastTab.url === url)
        ? lastTab
        : {
            id: 1,
            windowId: 1,
            index: 0,
            active: true,
            highlighted: true,
            status: "complete",
            url,
          };
    lastTab = tab;
    return { ...(sender || {}), tab };
  }

  nativeOnConnectAdd((port) => {
    rememberTab(port?.sender);
    if (!port || port.name !== KEEPALIVE_CHANNEL) return;
    if (port.sender?.id !== chrome.runtime.id || port.sender?.url !== BRIDGE_URL) {
      port.disconnect();
      return;
    }
    port.onMessage.addListener(() => undefined);
  });
  try {
    chrome.runtime.onConnect.addListener = (listener) => {
      if (onConnectWrappers.has(listener)) return;
      const wrapper = (port) => {
        if (port?.name === KEEPALIVE_CHANNEL) return;
        rememberTab(port?.sender);
        return listener(port);
      };
      onConnectWrappers.set(listener, wrapper);
      nativeOnConnectAdd(wrapper);
    };
    chrome.runtime.onConnect.removeListener = (listener) => {
      const wrapper = onConnectWrappers.get(listener) || listener;
      onConnectWrappers.delete(listener);
      nativeOnConnectRemove(wrapper);
    };
    chrome.runtime.onConnect.hasListener = (listener) =>
      nativeOnConnectHas(onConnectWrappers.get(listener) || listener);
  } catch (_error) {}
  nativeOnMessageAdd((message, sender, sendResponse) => {
    if (!message || message.channel !== CHANNEL) return undefined;
    if (sender?.id !== chrome.runtime.id || sender?.url !== BRIDGE_URL) return undefined;
    if (message.type === "event") {
      if (deliveredSequences.has(message.sequence)) {
        sendResponse({ ok: true, sequence: message.sequence });
        return false;
      }
      const handlers = listeners.get(`${message.namespace}.${message.event}`) || [];
      for (const handler of handlers) {
        try {
          handler(...message.arguments);
        } catch (_error) {}
      }
      deliveredSequences.add(message.sequence);
      deliveredSequenceOrder.push(message.sequence);
      while (deliveredSequenceOrder.length > 256) {
        deliveredSequences.delete(deliveredSequenceOrder.shift());
      }
      sendResponse({ ok: true, sequence: message.sequence });
      return false;
    }
    return undefined;
  });

  function isReservedMessage(message, sender) {
    return (
      message?.channel === CHANNEL &&
      sender?.id === chrome.runtime.id &&
      sender?.url === BRIDGE_URL
    );
  }

  try {
    chrome.runtime.onMessage.addListener = (listener) => {
      if (onMessageWrappers.has(listener)) return;
      const wrapper = (message, sender, sendResponse) => {
        if (isReservedMessage(message, sender)) return undefined;
        return listener(message, senderWithTab(message, sender), sendResponse);
      };
      onMessageWrappers.set(listener, wrapper);
      nativeOnMessageAdd(wrapper);
    };
    chrome.runtime.onMessage.removeListener = (listener) => {
      const wrapper = onMessageWrappers.get(listener) || listener;
      onMessageWrappers.delete(listener);
      nativeOnMessageRemove(wrapper);
    };
    chrome.runtime.onMessage.hasListener = (listener) =>
      nativeOnMessageHas(onMessageWrappers.get(listener) || listener);
  } catch (_error) {}

  function cancelSubscriptionRetry(key) {
    const retry = subscriptionRetries.get(key);
    if (retry?.timer) nativeClearTimeout(retry.timer);
    subscriptionRetries.delete(key);
  }

  function sendSubscription(key, namespace, event, attempt) {
    if (!listeners.has(key)) return;
    nativeSendMessage({
      channel: CHANNEL,
      type: "subscribe",
      subscriptionId: key,
      namespace,
      event,
    }).then(
      (response) => {
        if (response?.accepted === true) {
          cancelSubscriptionRetry(key);
          return;
        }
        scheduleSubscriptionRetry(key, namespace, event, attempt + 1);
      },
      () => scheduleSubscriptionRetry(key, namespace, event, attempt + 1),
    );
  }

  function scheduleSubscriptionRetry(key, namespace, event, attempt) {
    if (!listeners.has(key) || attempt >= 8) return;
    cancelSubscriptionRetry(key);
    const delay = nativeMin(30000, 250 * 2 ** attempt);
    const timer = nativeSetTimeout(() => {
      subscriptionRetries.delete(key);
      sendSubscription(key, namespace, event, attempt);
    }, delay);
    subscriptionRetries.set(key, { timer });
  }
  globalThis.__vmuxExtensionRuntime = {
    channel: CHANNEL,
    register(namespace, event, handler) {
      const key = `${namespace}.${event}`;
      const handlers = listeners.get(key) || [];
      handlers.push(handler);
      listeners.set(key, handlers);
      if (handlers.length === 1) {
        sendSubscription(key, namespace, event, 0);
      }
      return () => {
        const current = listeners.get(key) || [];
        const index = current.indexOf(handler);
        if (index >= 0) current.splice(index, 1);
        if (current.length) return;
        listeners.delete(key);
        cancelSubscriptionRetry(key);
        nativeSendMessage({
          channel: CHANNEL,
          type: "unsubscribe",
          subscriptionId: key,
        }).catch(() => undefined);
      };
    },
    request(namespace, method, argumentsValue) {
      return nativeSendMessage({
        channel: CHANNEL,
        type: "api_request",
        requestId: nativeRandomUUID(),
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
