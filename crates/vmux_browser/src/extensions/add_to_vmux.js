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
  // The real install CTA is a filled (colored) button whose label contains the
  // untranslated brand word "chrome" ("Add to Chrome"/"Ajouter à Google
  // Chrome"...). The top "switch to Chrome" banner's CTA also says "...Chrome"
  // but is a transparent text link, so the background filter excludes it.
  function isInstallButton(el) {
    if (el.offsetParent === null) return false;
    var t = (el.textContent || "").trim().toLowerCase();
    if (!t || t.length > 40 || t.indexOf("chrome") === -1 || t.indexOf("web store") !== -1) {
      return false;
    }
    var bg = getComputedStyle(el).backgroundColor;
    return !!bg && bg !== "transparent" && bg !== "rgba(0, 0, 0, 0)";
  }
  function findBtn() {
    var els = document.querySelectorAll("button, [role=button]");
    for (var i = 0; i < els.length; i++) {
      if (els[i].dataset && els[i].dataset.vmux) return els[i];
    }
    for (var j = 0; j < els.length; j++) {
      if (isInstallButton(els[j])) return els[j];
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
  // Remove only the small "Switch to Chrome?" nag dialog: a dialog mentioning
  // "chrome" with no input (so we never touch the search box or main content).
  function dismissNags() {
    var dialogs = document.querySelectorAll("[role=alertdialog], [role=dialog]");
    for (var i = 0; i < dialogs.length; i++) {
      var d = dialogs[i];
      var t = (d.textContent || "").toLowerCase();
      if (t.indexOf("chrome") !== -1 && !d.querySelector("input, textarea")) {
        d.remove();
      }
    }
  }
  // Hide the "Switch to Chrome to install..." banner. Start from its CTA (a
  // transparent chrome link, not the filled install button), then walk up while
  // the subtree text stays banner-sized; the last such ancestor is the bar.
  // The text cap keeps us from ever hiding the search box / page content.
  function hideBanner() {
    var els = document.querySelectorAll("a, button, [role=button]");
    for (var i = 0; i < els.length; i++) {
      var e = els[i];
      if (e.offsetParent === null || (e.dataset && e.dataset.vmux)) continue;
      if (isInstallButton(e)) continue;
      var t = (e.textContent || "").trim().toLowerCase();
      if (!t || t.length > 40 || t.indexOf("chrome") === -1 || t.indexOf("web store") !== -1) {
        continue;
      }
      var p = e;
      var banner = null;
      for (var d = 0; d < 8 && p; d++) {
        if ((p.textContent || "").trim().length > 200) break;
        banner = p;
        p = p.parentElement;
      }
      if (banner && banner !== e) banner.style.display = "none";
    }
  }
  function tick() {
    relabel();
    hideBanner();
    dismissNags();
  }
  new MutationObserver(tick).observe(document.documentElement, {
    childList: true,
    subtree: true,
  });
  tick();
})();
