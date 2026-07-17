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
  var bridgeRuntime = globalThis.__vmuxExtensionRuntime;
  function firstUrl(info) {
    var u = info && info.url;
    return Array.isArray(u) ? u[0] : u;
  }
  function openPopout(info) {
    var url = firstUrl(info);
    if (!url) return Promise.resolve();
    if (bridgeRuntime && typeof bridgeRuntime.request === "function") {
      return bridgeRuntime.request("windows", "create", [{ url: url }]);
    }
    if (!nativeTabsCreate) return Promise.reject(new Error("vmux cannot open extension window"));
    try {
      return Promise.resolve(nativeTabsCreate({ url: url }));
    } catch (e) {
      return Promise.reject(e);
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
      var w = fakeWindow();
      var opened = openPopout(info).then(function () { return w; });
      if (typeof cb !== "function") return opened;
      opened.then(cb, function () { cb(undefined); });
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
        done(tabs || []);
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
          resolve([tab]);
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
      var opened = openPopout(info).then(function () { return t; });
      if (typeof cb !== "function") return opened;
      opened.then(cb, function () { cb(undefined); });
    };
  }
})();
