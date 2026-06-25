(function () {
  if (window.__VMUX_EXT__) return;
  window.__VMUX_EXT__ = true;
  var installed = Array.isArray(window.__VMUX_INSTALLED__)
    ? window.__VMUX_INSTALLED__.slice()
    : [];
  function extId() {
    var s = location.pathname.split("/");
    for (var i = 0; i < s.length; i++) {
      if (/^[a-p]{32}$/.test(s[i])) return s[i];
    }
    return null;
  }
  // The install button is the only button whose label contains the untranslated
  // brand word "chrome" (e.g. "Add to Chrome", "Ajouter à Google Chrome"). The
  // store header is a link, and the nag buttons (Yes / No thanks) lack "chrome".
  function findBtn() {
    var els = document.querySelectorAll("button, [role=button]");
    for (var i = 0; i < els.length; i++) {
      if (els[i].dataset && els[i].dataset.vmux) return els[i];
    }
    for (var j = 0; j < els.length; j++) {
      var el = els[j];
      if (el.offsetParent === null) continue;
      var t = (el.textContent || "").trim();
      if (!t || t.length > 40) continue;
      var low = t.toLowerCase();
      if (low.indexOf("chrome") !== -1 && low.indexOf("web store") === -1) {
        return el;
      }
    }
    return null;
  }
  function labelNode(root) {
    var w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
    var n,
      best = null;
    while ((n = w.nextNode())) {
      var t = (n.nodeValue || "").trim();
      if (!t) continue;
      if (t.toLowerCase().indexOf("chrome") !== -1) return n;
      if (!best) best = n;
    }
    return best;
  }
  function setLabel(btn, text) {
    var ln = labelNode(btn);
    if (ln) ln.nodeValue = text;
    else btn.textContent = text;
  }
  function relabel() {
    var id = extId();
    if (!id) return;
    var btn = findBtn();
    if (!btn) return;
    if (btn.dataset.vmux === id) return;
    btn.dataset.vmux = id;
    btn.dataset.state = "";
    setLabel(btn, installed.indexOf(id) >= 0 ? "Remove from Vmux" : "Add to Vmux");
  }
  document.addEventListener(
    "click",
    function (e) {
      var btn =
        e.target && e.target.closest ? e.target.closest("[data-vmux]") : null;
      if (!btn) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      var id = btn.dataset.vmux;
      if (btn.dataset.state === "pending") {
        try {
          cef.emit({ channel: "vmux-relaunch" });
        } catch (err) {}
        setLabel(btn, "Relaunching…");
        return;
      }
      var idx = installed.indexOf(id);
      if (idx >= 0) {
        try {
          cef.emit({ channel: "vmux-remove-extension", id: id });
        } catch (err) {}
        installed.splice(idx, 1);
        setLabel(btn, "Relaunch to apply");
      } else {
        try {
          cef.emit({ channel: "vmux-add-extension", id: id });
        } catch (err) {}
        installed.push(id);
        setLabel(btn, "Relaunch to enable");
      }
      btn.dataset.state = "pending";
    },
    true,
  );
  // Locale-independent: drop the "switch to Chrome" dialog and banner by role.
  function dismissNags() {
    var dialogs = document.querySelectorAll("[role=dialog], [role=alertdialog]");
    for (var i = 0; i < dialogs.length; i++) {
      if ((dialogs[i].textContent || "").toLowerCase().indexOf("chrome") !== -1) {
        dialogs[i].remove();
      }
    }
    var bars = document.querySelectorAll("[role=status], [role=alert]");
    for (var j = 0; j < bars.length; j++) {
      if ((bars[j].textContent || "").toLowerCase().indexOf("chrome") !== -1) {
        bars[j].style.display = "none";
      }
    }
  }
  function tick() {
    relabel();
    dismissNags();
  }
  new MutationObserver(tick).observe(document.documentElement, {
    childList: true,
    subtree: true,
  });
  tick();
})();
