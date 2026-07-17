chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message && message.type === "__vmux_conformance_ping") {
    sendResponse("pong");
  }
  return false;
});
