(function () {
  var c = globalThis.chrome;
  if (!c) return;
  var BRIDGE_CHANNEL = __VMUX_BRIDGE_CHANNEL__;
  var KEEPALIVE_CHANNEL = __VMUX_KEEPALIVE_CHANNEL__;
  var BRIDGE_URL = c.runtime && c.runtime.getURL ? c.runtime.getURL("vmux_bridge.html") : null;
  var ACTIVE_TAB_KEY = "__vmux_active_tab_v1";
  var FAKE_WINDOW_ID = 1;
  var FAKE_TAB_ID = 1;
  var lastTab = null;
  var knownWindows = [];

  function webUrl(value) {
    return typeof value === "string" && /^(https?|file):/.test(value) ? value : null;
  }

  function senderUrl(message, sender) {
    return (
      webUrl(sender && sender.tab && sender.tab.url) ||
      webUrl(sender && sender.url) ||
      webUrl(message && message.tab && message.tab.url) ||
      webUrl(message && message.url) ||
      webUrl(message && message.uri) ||
      webUrl(message && message.documentUrl)
    );
  }

  function capture(message, sender) {
    var url = senderUrl(message, sender);
    if (!url) return null;
    var tab = Object.assign({}, (sender && sender.tab) || (message && message.tab) || {});
    if (typeof tab.id !== "number" || tab.id < 0) tab.id = FAKE_TAB_ID;
    if (typeof tab.windowId !== "number" || tab.windowId < 0) tab.windowId = FAKE_WINDOW_ID;
    if (!webUrl(tab.url)) tab.url = url;
    if (typeof tab.index !== "number") tab.index = 0;
    if (typeof tab.active !== "boolean") tab.active = true;
    if (typeof tab.highlighted !== "boolean") tab.highlighted = tab.active;
    if (!tab.status) tab.status = "complete";
    lastTab = tab;
    if (c.storage && c.storage.session) {
      var stored = {};
      stored[ACTIVE_TAB_KEY] = tab;
      try {
        var result = c.storage.session.set(stored);
        if (result && typeof result.catch === "function") result.catch(function () {});
      } catch (_error) {}
    }
    return tab;
  }

  function reservedMessage(message, sender) {
    return (
      message &&
      message.channel === BRIDGE_CHANNEL &&
      BRIDGE_URL &&
      sender &&
      sender.url === BRIDGE_URL
    );
  }

  if (c.runtime && c.runtime.onMessage) {
    c.runtime.onMessage.addListener(function (message, sender) {
      if (!reservedMessage(message, sender)) capture(message, sender);
      return undefined;
    });
  }
  if (c.runtime && c.runtime.onConnect) {
    c.runtime.onConnect.addListener(function (port) {
      if (!port || port.name === KEEPALIVE_CHANNEL) return;
      capture(null, port.sender);
    });
  }

  function fakeWindow(getInfo) {
    var w = {
      id: FAKE_WINDOW_ID, focused: true, top: 0, left: 0, width: 1920, height: 1080,
      incognito: false, type: "normal", state: "normal", alwaysOnTop: false,
    };
    if (getInfo && getInfo.populate) w.tabs = lastTab ? [lastTab] : [];
    return w;
  }

  function requestActiveTab(done) {
    if (lastTab) {
      done(lastTab);
      return;
    }
    if (!c.storage || !c.storage.session) {
      done(null);
      return;
    }
    try {
      var result = c.storage.session.get(ACTIVE_TAB_KEY);
      Promise.resolve(result).then(
        function (stored) {
          var tab = stored && stored[ACTIVE_TAB_KEY];
          if (tab && webUrl(tab.url)) lastTab = tab;
          done(lastTab);
        },
        function () {
          done(null);
        },
      );
    } catch (_error) {
      done(null);
    }
  }
  var nativeTabsCreate = c.tabs && c.tabs.create ? c.tabs.create.bind(c.tabs) : null;
  var bridgeRuntime = globalThis.__vmuxExtensionRuntime;
  function rememberWindows(windows) {
    if (Array.isArray(windows)) {
      knownWindows = windows;
      return windows;
    }
    if (windows && typeof windows.id === "number") {
      knownWindows = knownWindows.filter(function (known) { return known.id !== windows.id; });
      knownWindows.push(windows);
    }
    return windows;
  }
  function sameDocument(a, b) {
    if (typeof a !== "string" || typeof b !== "string") return false;
    try {
      var left = new URL(a);
      var right = new URL(b);
      left.hash = "";
      right.hash = "";
      return left.href === right.href;
    } catch (_error) {
      return a === b;
    }
  }
  function matchingWindowId(tab) {
    for (var i = 0; i < knownWindows.length; i++) {
      var win = knownWindows[i];
      var tabs = win && Array.isArray(win.tabs) ? win.tabs : [];
      for (var j = 0; j < tabs.length; j++) {
        if (tabs[j].id === tab.id || sameDocument(tabs[j].url, tab.url)) return win.id;
      }
    }
    return null;
  }
  function wildcardMatch(pattern, value) {
    if (typeof pattern !== "string" || typeof value !== "string") return false;
    var escaped = pattern.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*");
    try {
      return new RegExp("^" + escaped + "$").test(value);
    } catch (_error) {
      return pattern === value;
    }
  }
  function focusedWindowId() {
    for (var i = 0; i < knownWindows.length; i++) {
      if (knownWindows[i] && knownWindows[i].focused) return knownWindows[i].id;
    }
    return null;
  }
  function tabMatchesQuery(tab, queryInfo, ignoreWindowId) {
    if (!queryInfo) return true;
    var requestedWindowId = typeof queryInfo.windowId === "number"
      ? queryInfo.windowId
      : queryInfo.currentWindow || queryInfo.lastFocusedWindow
        ? focusedWindowId()
        : null;
    if (!ignoreWindowId && typeof requestedWindowId === "number" && tab.windowId !== requestedWindowId) {
      return false;
    }
    if (typeof queryInfo.active === "boolean" && tab.active !== queryInfo.active) return false;
    if (typeof queryInfo.highlighted === "boolean" && tab.highlighted !== queryInfo.highlighted) return false;
    if (typeof queryInfo.pinned === "boolean" && tab.pinned !== queryInfo.pinned) return false;
    if (typeof queryInfo.status === "string" && tab.status !== queryInfo.status) return false;
    if (typeof queryInfo.index === "number" && tab.index !== queryInfo.index) return false;
    if (typeof queryInfo.title === "string" && !wildcardMatch(queryInfo.title, tab.title || "")) return false;
    if (queryInfo.url != null) {
      var patterns = Array.isArray(queryInfo.url) ? queryInfo.url : [queryInfo.url];
      if (!patterns.some(function (pattern) { return wildcardMatch(pattern, tab.url || ""); })) {
        return false;
      }
    }
    return true;
  }
  function normalizeTabWindowIds(tabs, queryInfo) {
    var sourceTabs = tabs || [];
    if (!bridgeRuntime || typeof bridgeRuntime.request !== "function") {
      return Promise.resolve(sourceTabs.filter(function (tab) {
        return tabMatchesQuery(tab, queryInfo, false);
      }));
    }
    var baseTabs = sourceTabs.filter(function (tab) {
      return tabMatchesQuery(tab, queryInfo, true);
    });
    return bridgeRuntime.request("windows", "getAll", [{ populate: true }]).then(
      function (windows) {
        rememberWindows(windows);
        var normalized = baseTabs.map(function (tab) {
          var windowId = matchingWindowId(tab);
          return typeof windowId === "number" ? Object.assign({}, tab, { windowId: windowId }) : tab;
        });
        for (var i = 0; i < knownWindows.length; i++) {
          var win = knownWindows[i];
          var windowTabs = win && Array.isArray(win.tabs) ? win.tabs : [];
          for (var j = 0; j < windowTabs.length; j++) {
            var tab = Object.assign({}, windowTabs[j], { windowId: win.id });
            if (!tabMatchesQuery(tab, queryInfo, false)) continue;
            if (normalized.some(function (known) { return known.id === tab.id; })) continue;
            normalized.push(tab);
          }
        }
        return normalized.filter(function (tab) {
          return tabMatchesQuery(tab, queryInfo, false);
        });
      },
      function () { return baseTabs; },
    );
  }
  function callbackResult(promise, cb, noResult) {
    if (typeof cb !== "function") return promise;
    promise.then(
      function (result) { noResult ? cb() : cb(result); },
      function () { noResult ? cb() : cb(undefined); },
    );
  }
  function fallbackCreate(info) {
    var w = fakeWindow();
    var u = info && info.url;
    var url = Array.isArray(u) ? u[0] : u;
    if (!url || !nativeTabsCreate) return Promise.resolve(w);
    try {
      return Promise.resolve(nativeTabsCreate({ url: url })).then(function () { return w; });
    } catch (_error) {
      return Promise.resolve(w);
    }
  }
  function windowRequest(method, args, fallback) {
    var useFallback = function () {
      try {
        return Promise.resolve(fallback());
      } catch (_error) {
        return Promise.resolve(undefined);
      }
    };
    if (bridgeRuntime && typeof bridgeRuntime.request === "function") {
      return bridgeRuntime.request("windows", method, args).then(
        function (result) {
          return typeof result === "undefined" ? useFallback() : result;
        },
        function (error) {
          if (error && error.code && error.code !== "bridge_unavailable") {
            return Promise.reject(error);
          }
          return useFallback();
        },
      );
    }
    return useFallback();
  }
  function knownWindowType(id) {
    for (var i = 0; i < knownWindows.length; i++) {
      if (knownWindows[i] && knownWindows[i].id === id) return knownWindows[i].type;
    }
    return null;
  }
  function patchWindowEvent(name) {
    var event = c.windows && c.windows[name];
    if (!event || !bridgeRuntime || typeof bridgeRuntime.register !== "function") return;
    var registrations = new Map();
    event.addListener = function (listener, filter) {
      if (typeof listener !== "function" || registrations.has(listener)) return;
      var wrapped = function () {
        var args = [].slice.call(arguments);
        var windowType = args[0] && typeof args[0] === "object"
          ? args[0].type
          : knownWindowType(args[0]);
        var windowTypes = filter && Array.isArray(filter.windowTypes)
          ? filter.windowTypes
          : ["normal", "popup"];
        if (windowType && windowTypes.indexOf(windowType) < 0) return;
        listener.apply(null, args);
      };
      registrations.set(listener, bridgeRuntime.register("windows", name, wrapped));
    };
    event.removeListener = function (listener) {
      var unregister = registrations.get(listener);
      if (!unregister) return;
      registrations.delete(listener);
      unregister();
    };
    event.hasListener = function (listener) { return registrations.has(listener); };
    event.hasListeners = function () { return registrations.size > 0; };
  }

  if (c.windows) {
    try { Object.defineProperty(c.windows, "WINDOW_ID_NONE", { value: -1, configurable: true }); } catch (_error) {}
    try { Object.defineProperty(c.windows, "WINDOW_ID_CURRENT", { value: -2, configurable: true }); } catch (_error) {}
    c.windows.get = function (id, options, cb) {
      if (typeof options === "function") { cb = options; options = undefined; }
      var promise = windowRequest("get", [id, options || {}], function () {
        var w = fakeWindow(options);
        if (typeof id === "number" && id >= 0) w.id = id;
        return w;
      }).then(rememberWindows);
      return callbackResult(promise, cb, false);
    };
    c.windows.getCurrent = function (options, cb) {
      if (typeof options === "function") { cb = options; options = undefined; }
      var promise = windowRequest("getCurrent", [options || {}], function () {
        return fakeWindow(options);
      }).then(rememberWindows);
      return callbackResult(promise, cb, false);
    };
    c.windows.getLastFocused = function (options, cb) {
      if (typeof options === "function") { cb = options; options = undefined; }
      var promise = windowRequest("getLastFocused", [options || {}], function () {
        return fakeWindow(options);
      }).then(rememberWindows);
      return callbackResult(promise, cb, false);
    };
    c.windows.getAll = function (options, cb) {
      if (typeof options === "function") { cb = options; options = undefined; }
      var promise = windowRequest("getAll", [options || {}], function () {
        return [fakeWindow(options)];
      }).then(rememberWindows);
      return callbackResult(promise, cb, false);
    };
    c.windows.create = function (info, cb) {
      var promise = windowRequest("create", [info || {}], function () {
        return fallbackCreate(info || {});
      }).then(function (win) {
        if (win) knownWindows.push(win);
        return win;
      });
      return callbackResult(promise, cb, false);
    };
    c.windows.update = function (id, info, cb) {
      var promise = windowRequest("update", [id, info || {}], function () {
        var w = fakeWindow();
        if (typeof id === "number" && id >= 0) w.id = id;
        return Object.assign(w, info || {});
      }).then(function (win) {
        if (win) {
          knownWindows = knownWindows.filter(function (known) { return known.id !== win.id; });
          knownWindows.push(win);
        }
        return win;
      });
      return callbackResult(promise, cb, false);
    };
    c.windows.remove = function (id, cb) {
      var promise = windowRequest("remove", [id], function () { return undefined; }).then(function () {
        knownWindows = knownWindows.filter(function (win) { return win.id !== id; });
      });
      return callbackResult(promise, cb, true);
    };
    patchWindowEvent("onCreated");
    patchWindowEvent("onRemoved");
    patchWindowEvent("onFocusChanged");
    patchWindowEvent("onBoundsChanged");
  }

  if (globalThis.window === globalThis && typeof globalThis.close === "function") {
    var nativeWindowClose = globalThis.close.bind(globalThis);
    globalThis.close = function () {
      if (!bridgeRuntime || typeof bridgeRuntime.request !== "function") {
        nativeWindowClose();
        return;
      }
      bridgeRuntime.request("windows", "getCurrent", [{}]).then(
        function (win) {
          if (!win || typeof win.id !== "number") {
            nativeWindowClose();
            return;
          }
          bridgeRuntime.request("windows", "remove", [win.id]).catch(nativeWindowClose);
        },
        nativeWindowClose,
      );
    };
  }

  if (c.tabs) {
    var origQuery = c.tabs.query ? c.tabs.query.bind(c.tabs) : null;
    function queryNative(queryInfo, done) {
      if (!origQuery) {
        done([]);
        return;
      }
      var settled = false;
      var finish = function (tabs) {
        if (settled) return;
        settled = true;
        if (c.runtime) void c.runtime.lastError;
        normalizeTabWindowIds(tabs || [], queryInfo).then(done);
      };
      try {
        var result = origQuery(queryInfo, finish);
        if (result && typeof result.then === "function") result.then(finish, function () { finish([]); });
      } catch (_error) {
        finish([]);
      }
    }
    c.tabs.query = function (queryInfo, cb) {
      var wantsActive =
        queryInfo && (queryInfo.active || queryInfo.currentWindow || queryInfo.lastFocusedWindow);
      var promise = new Promise(function (resolve) {
        var fallback = function () {
          queryNative(queryInfo, function (tabs) {
            resolve(tabs);
          });
        };
        if (!wantsActive) {
          fallback();
          return;
        }
        requestActiveTab(function (tab) {
          if (!tab) {
            fallback();
            return;
          }
          normalizeTabWindowIds([tab], queryInfo).then(resolve);
        });
      });
      if (typeof cb === "function") {
        promise.then(cb);
        return;
      }
      return promise;
    };
    var origGet = c.tabs.get ? c.tabs.get.bind(c.tabs) : null;
    c.tabs.get = function (id, cb) {
      var promise = new Promise(function (resolve) {
        requestActiveTab(function (tab) {
          if (tab && (id == null || id === tab.id)) {
            resolve(tab);
            return;
          }
          if (!origGet) {
            resolve(tab);
            return;
          }
          try {
            var result = origGet(id, function (nativeTab) {
              if (c.runtime) void c.runtime.lastError;
              resolve(nativeTab || tab);
            });
            if (result && typeof result.then === "function") {
              result.then(resolve, function () { resolve(tab); });
            }
          } catch (_error) {
            resolve(tab);
          }
        });
      });
      if (typeof cb === "function") {
        promise.then(cb);
        return;
      }
      return promise;
    };
    c.tabs.create = function (info, cb) {
      var t = lastTab || { id: FAKE_WINDOW_ID, active: true };
      var opened = windowRequest("create", [info || {}], function () {
        return fallbackCreate(info || {});
      }).then(function () { return t; });
      if (typeof cb !== "function") return opened;
      opened.then(cb, function () { cb(undefined); });
    };
  }
})();
