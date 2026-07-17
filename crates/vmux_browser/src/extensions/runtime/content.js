(() => {
  const runtime = globalThis.chrome?.runtime;
  if (!runtime || globalThis.__vmuxRuntimeSendMessageRetry) return;
  globalThis.__vmuxRuntimeSendMessageRetry = true;

  const nativeSendMessage = runtime.sendMessage.bind(runtime);
  const nativeSetTimeout = globalThis.setTimeout.bind(globalThis);
  const retryDelays = [25, 50, 100, 200, 400, 800, 1600];

  function withSenderContext(args) {
    const messageIndex = typeof args[0] === "string" ? 1 : 0;
    const message = args[messageIndex];
    if (!message || typeof message !== "object" || Array.isArray(message)) return args;
    const contextualized = args.slice();
    contextualized[messageIndex] = {
      ...message,
      __vmuxSenderUrl: globalThis.location?.href || "",
    };
    return contextualized;
  }

  function missingReceiver(error) {
    const message = typeof error === "string" ? error : error?.message;
    return (
      typeof message === "string" &&
      (message.includes("Receiving end does not exist") ||
        message.includes("Could not establish connection"))
    );
  }

  function schedule(attempt, callback) {
    nativeSetTimeout(callback, retryDelays[attempt]);
  }

  function sendCallback(args, callback, attempt) {
    nativeSendMessage(...args, (response) => {
      const error = runtime.lastError;
      if (missingReceiver(error) && attempt < retryDelays.length) {
        schedule(attempt, () => sendCallback(args, callback, attempt + 1));
        return;
      }
      callback(response);
    });
  }

  function sendPromise(args, attempt) {
    let result;
    try {
      result = nativeSendMessage(...args);
    } catch (error) {
      return Promise.reject(error);
    }
    return Promise.resolve(result).catch((error) => {
      if (!missingReceiver(error) || attempt >= retryDelays.length) throw error;
      return new Promise((resolve) => schedule(attempt, resolve)).then(() =>
        sendPromise(args, attempt + 1),
      );
    });
  }

  function patchedSendMessage() {
    const args = withSenderContext(Array.from(arguments));
    const callback = typeof args.at(-1) === "function" ? args.pop() : null;
    if (!callback) return sendPromise(args, 0);
    sendCallback(args, callback, 0);
  }

  runtime.sendMessage = patchedSendMessage;
})();
