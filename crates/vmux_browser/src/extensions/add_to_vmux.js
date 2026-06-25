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
  function findBtn() {
    var els = document.querySelectorAll("button, a, [role=button]");
    for (var i = 0; i < els.length; i++) {
      var el = els[i];
      if (el.offsetParent === null) continue;
      var t = (el.textContent || "").trim();
      if (t === "Add to Chrome" || t === "Remove from Chrome") return el;
      if (el.dataset && el.dataset.vmux) return el;
    }
    return null;
  }
  function labelNode(root) {
    var w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
    var n,
      best = null;
    while ((n = w.nextNode())) {
      var t = (n.nodeValue || "").trim();
      if (
        t === "Add to Chrome" ||
        t === "Remove from Chrome" ||
        t === "Add to Vmux" ||
        t === "Remove from Vmux"
      ) {
        return n;
      }
      if (!best && t) best = n;
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
  function dismissNags() {
    var els = document.querySelectorAll("button, a, [role=button]");
    for (var i = 0; i < els.length; i++) {
      var t = (els[i].textContent || "").trim();
      if (t === "No thanks") {
        els[i].click();
      } else if (t === "Install Chrome") {
        var p = els[i];
        for (var d = 0; d < 6 && p; d++) {
          if ((p.textContent || "").indexOf("Switch to Chrome to install") >= 0) {
            p.style.display = "none";
            break;
          }
          p = p.parentElement;
        }
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
