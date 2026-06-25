(function () {
  if (window.__VMUX_EXT__) return;
  window.__VMUX_EXT__ = true;
  function extId() {
    var s = location.pathname.split("/");
    for (var i = 0; i < s.length; i++) {
      if (/^[a-p]{32}$/.test(s[i])) return s[i];
    }
    return null;
  }
  function findAddBtn() {
    var els = document.querySelectorAll("button, a, [role=button]");
    for (var i = 0; i < els.length; i++) {
      var el = els[i];
      if (el.id === "vmux-add-ext" || el.offsetParent === null) continue;
      var t = (el.textContent || "").trim();
      if (t === "Add to Chrome" || t === "Remove from Chrome") return el;
    }
    return null;
  }
  function labelNode(root) {
    var w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
    var n,
      best = null;
    while ((n = w.nextNode())) {
      var t = (n.nodeValue || "").trim();
      if (t === "Add to Chrome" || t === "Remove from Chrome") return n;
      if (!best && t) best = n;
    }
    return best;
  }
  function place() {
    var id = extId();
    var existing = document.getElementById("vmux-add-ext");
    if (!id) {
      if (existing) existing.remove();
      return;
    }
    if (existing && existing.dataset.extId === id) return;
    var anchor = findAddBtn();
    if (!anchor || !anchor.parentNode) return;
    if (existing) existing.remove();
    var b = anchor.cloneNode(true);
    b.id = "vmux-add-ext";
    b.dataset.extId = id;
    b.removeAttribute("disabled");
    b.removeAttribute("href");
    var ln = labelNode(b);
    if (ln) ln.nodeValue = "Add to Vmux";
    else b.textContent = "Add to Vmux";
    b.addEventListener("click", function (ev) {
      ev.preventDefault();
      ev.stopPropagation();
      try {
        cef.emit({ channel: "vmux-add-extension", id: id });
      } catch (e) {}
      if (ln) ln.nodeValue = "Added — relaunch";
      else b.textContent = "Added — relaunch";
    });
    anchor.style.display = "none";
    anchor.parentNode.insertBefore(b, anchor);
  }
  new MutationObserver(place).observe(document.documentElement, {
    childList: true,
    subtree: true,
  });
  place();
})();
