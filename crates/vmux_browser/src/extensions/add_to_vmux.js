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
  function isInstallText(el) {
    if (el.offsetParent === null) return false;
    var t = (el.textContent || "").trim().toLowerCase();
    return (
      !!t &&
      t.length <= 40 &&
      t.indexOf("chrome") !== -1 &&
      t.indexOf("web store") === -1
    );
  }
  // The real install button lives in the extension header next to the <h1>
  // title; the top-of-page "switch to Chrome" banner has a separate CTA that
  // also says "...Chrome", so scope the search to the title's container.
  function headerBtn() {
    var h1 = document.querySelector("h1");
    if (!h1) return null;
    var c = h1;
    for (var u = 0; u < 5 && c.parentElement; u++) c = c.parentElement;
    var els = c.querySelectorAll("button, [role=button]");
    for (var i = 0; i < els.length; i++) {
      if (els[i].dataset && els[i].dataset.vmux) return els[i];
      if (isInstallText(els[i])) return els[i];
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
    var btn = headerBtn();
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
  // Hide the "switch to Chrome" banner (its CTA also says "...Chrome", so find
  // that CTA — not the header button — and hide its full-width container) and
  // remove the "Switch to Chrome?" dialog.
  function dismissNags() {
    var real = headerBtn();
    var els = document.querySelectorAll("button, [role=button], a");
    for (var i = 0; i < els.length; i++) {
      var e = els[i];
      if (e === real || (e.dataset && e.dataset.vmux)) continue;
      if (!isInstallText(e)) continue;
      var p = e;
      for (var d = 0; d < 6 && p; d++) {
        var w = p.getBoundingClientRect ? p.getBoundingClientRect().width : 0;
        if (w >= 600) {
          p.style.display = "none";
          break;
        }
        p = p.parentElement;
      }
    }
    var dialogs = document.querySelectorAll("[role=dialog], [role=alertdialog]");
    for (var k = 0; k < dialogs.length; k++) {
      if ((dialogs[k].textContent || "").toLowerCase().indexOf("chrome") !== -1) {
        dialogs[k].remove();
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
