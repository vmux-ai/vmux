// vmux extension shim, loaded before the real service worker.
//
// vmux embeds CEF in chrome-bootstrap + alloy-style browsers, where chrome.windows
// resolves the focused window to WINDOW_ID_NONE (-1) and chrome.tabs.query/get are
// unpopulated -- yet message/port senders still carry a valid sender.tab. Observe
// that tab via our own (non-invasive) listeners and feed it back through shimmed
// chrome.tabs.query/get, and supply a synthetic focused window, so extensions (e.g. a
// password manager) can resolve the current tab and position their UI instead of
// throwing. Valid as both a classic importScripts target and an ES module import.
(function () {
  var c = globalThis.chrome;
  if (!c) return;
  function log() {
    try {
      console.log.apply(console, ["[vmux]"].concat([].slice.call(arguments)));
    } catch (e) {}
  }

  var lastTab = null;
  var loggedTabId = null;
  function capture(sender, where) {
    if (sender && sender.tab && typeof sender.tab.id === "number") {
      lastTab = sender.tab;
      if (lastTab.id !== loggedTabId) {
        loggedTabId = lastTab.id;
        log("captured tab", where, lastTab.id, lastTab.url);
      }
    }
  }

  // Non-invasive observers: our own listeners run alongside the extension's without
  // wrapping or consuming its messages (return undefined => not handled).
  if (c.runtime && c.runtime.onMessage) {
    c.runtime.onMessage.addListener(function (msg, sender) {
      capture(sender, "msg");
      var cmd = msg && (msg.command || msg.type);
      if (cmd && /openAutofillInlineMenu|updateAutofillInlineMenuListCiphers|cipher/i.test(String(cmd))) {
        log("msg", cmd, "ciphers:", msg && msg.ciphers ? msg.ciphers.length : "n/a");
      }
      return undefined;
    });
  }
  if (c.runtime && c.runtime.onConnect) {
    c.runtime.onConnect.addListener(function (port) {
      if (!port) return;
      if (port.sender) capture(port.sender, "port:" + (port.name || "?"));
      if (/inline|overlay|autofill/i.test(String(port.name || ""))) {
        log("port connect", port.name);
        var origPost = port.postMessage && port.postMessage.bind(port);
        if (origPost) {
          port.postMessage = function (m) {
            try {
              var cmd = m && (m.command || m.type);
              if (cmd && (/cipher/i.test(String(cmd)) || (m && m.ciphers))) {
                log("port>", port.name, cmd, "ciphers:", m && m.ciphers ? m.ciphers.length : "n/a");
              }
            } catch (e) {}
            return origPost(m);
          };
        }
      }
    });
  }

  var FAKE_WINDOW_ID = 1;
  function fakeWindow(getInfo) {
    var w = {
      id: FAKE_WINDOW_ID, focused: true, top: 0, left: 0, width: 1920, height: 1080,
      incognito: false, type: "normal", state: "normal", alwaysOnTop: false,
    };
    if (getInfo && getInfo.populate) w.tabs = lastTab ? [lastTab] : [];
    return w;
  }
  function resolved(makeResult) {
    return function () {
      var args = [].slice.call(arguments);
      var cb = typeof args[args.length - 1] === "function" ? args.pop() : null;
      var result;
      try {
        result = makeResult.apply(null, args);
      } catch (e) {
        result = undefined;
      }
      if (cb) {
        try {
          cb(result);
        } catch (e) {}
        return;
      }
      return Promise.resolve(result);
    };
  }

  var nativeTabsCreate = c.tabs && c.tabs.create ? c.tabs.create.bind(c.tabs) : null;
  function firstUrl(info) {
    var u = info && info.url;
    return Array.isArray(u) ? u[0] : u;
  }
  // chrome.windows.create / tabs.create don't open anything under alloy-style CEF, so
  // extension popouts (unlock, add-login) never appear. Open the URL via the SW
  // clients.openWindow API instead, which doesn't depend on chrome.windows.
  function openPopout(info) {
    var url = firstUrl(info);
    log("open popout", url);
    if (!url) return;
    try {
      if (self.clients && self.clients.openWindow) {
        self.clients.openWindow(url).then(
          function (client) { log("openWindow", client ? "ok" : "null"); },
          function (e) { log("openWindow fail", String(e)); }
        );
        return;
      }
    } catch (e) {
      log("openWindow threw", String(e));
    }
    if (nativeTabsCreate) {
      try {
        nativeTabsCreate({ url: url });
      } catch (e) {}
    }
  }

  if (c.windows) {
    c.windows.getCurrent = resolved(function (gi) { return fakeWindow(gi); });
    c.windows.getLastFocused = resolved(function (gi) { return fakeWindow(gi); });
    c.windows.get = resolved(function (id, gi) {
      var w = fakeWindow(gi);
      if (typeof id === "number" && id >= 0) w.id = id;
      return w;
    });
    c.windows.getAll = resolved(function (gi) { return [fakeWindow(gi)]; });
    c.windows.update = resolved(function (id) {
      var w = fakeWindow();
      if (typeof id === "number" && id >= 0) w.id = id;
      return w;
    });
    c.windows.create = function (info, cb) {
      openPopout(info);
      var w = fakeWindow();
      if (typeof cb === "function") {
        cb(w);
        return;
      }
      return Promise.resolve(w);
    };
  }

  if (c.tabs) {
    var origQuery = c.tabs.query ? c.tabs.query.bind(c.tabs) : null;
    c.tabs.query = function (queryInfo, cb) {
      var wantsActive =
        queryInfo && (queryInfo.active || queryInfo.currentWindow || queryInfo.lastFocusedWindow);
      var deliver = function (res) {
        if (cb) {
          cb(res);
          return;
        }
        return Promise.resolve(res);
      };
      if (wantsActive && lastTab) return deliver([lastTab]);
      if (origQuery) {
        try {
          var p = origQuery(queryInfo);
          if (p && p.then) {
            return p.then(
              function (r) {
                if (wantsActive && (!r || !r.length) && lastTab) r = [lastTab];
                return deliver(r || []);
              },
              function () {
                return deliver(lastTab ? [lastTab] : []);
              }
            );
          }
        } catch (e) {}
      }
      return deliver(lastTab ? [lastTab] : []);
    };
    var origGet = c.tabs.get ? c.tabs.get.bind(c.tabs) : null;
    c.tabs.get = function (id, cb) {
      var deliver = function (res) {
        if (cb) {
          cb(res);
          return;
        }
        return Promise.resolve(res);
      };
      if (lastTab && (id == null || id === lastTab.id)) return deliver(lastTab);
      if (origGet) {
        try {
          var p = origGet(id);
          if (p && p.then) return p.then(deliver, function () { return deliver(lastTab); });
        } catch (e) {}
      }
      return deliver(lastTab);
    };
    c.tabs.create = function (info, cb) {
      openPopout(info);
      var t = lastTab || { id: FAKE_WINDOW_ID, active: true };
      if (typeof cb === "function") {
        cb(t);
        return;
      }
      return Promise.resolve(t);
    };
  }

  log("shim v5 installed (capture + windows + tabs + openWindow popouts)");
})();
