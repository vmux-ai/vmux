(function () {
  var killRing = "";

  function isTextField(el) {
    if (!el || el.nodeType !== 1) return false;
    var tag = el.tagName;
    if (tag === "TEXTAREA") {
      return !el.readOnly && !el.disabled;
    }
    if (tag !== "INPUT") return false;
    var type = (el.type || "text").toLowerCase();
    if (
      type === "checkbox" ||
      type === "radio" ||
      type === "button" ||
      type === "submit" ||
      type === "reset" ||
      type === "file" ||
      type === "hidden" ||
      type === "image"
    ) {
      return false;
    }
    return !el.readOnly && !el.disabled;
  }

  function lineStart(v, pos) {
    var i = v.lastIndexOf("\n", pos - 1);
    return i < 0 ? 0 : i + 1;
  }

  function lineEnd(v, pos) {
    var i = v.indexOf("\n", pos);
    return i < 0 ? v.length : i;
  }

  function prevLinePos(v, pos) {
    if (pos <= 0) return 0;
    var ls = lineStart(v, pos);
    if (ls === 0) return 0;
    var col = pos - ls;
    var prevLs = lineStart(v, ls - 1);
    var prevLe = ls - 1;
    return Math.min(prevLs + col, prevLe);
  }

  function nextLinePos(v, pos) {
    var le = lineEnd(v, pos);
    if (le >= v.length) return v.length;
    var col = pos - lineStart(v, pos);
    var nls = le + 1;
    var nle = lineEnd(v, nls);
    return Math.min(nls + col, nle);
  }

  function prevWord(v, pos) {
    var p = pos;
    while (p > 0 && /\s/.test(v.charAt(p - 1))) p--;
    while (p > 0 && !/\s/.test(v.charAt(p - 1))) p--;
    return p;
  }

  function setCaret(el, i) {
    el.setSelectionRange(i, i, "forward");
  }

  function bumpInput(el) {
    try {
      el.dispatchEvent(new Event("input", { bubbles: true }));
    } catch (_) {}
  }

  function controlDown(ev) {
    if (ev.metaKey || ev.altKey) return false;
    if (ev.ctrlKey) return true;
    if (typeof ev.getModifierState === "function" && ev.getModifierState("Control")) {
      return true;
    }
    return false;
  }

  function emacsLetter(ev) {
    var k = ev.key;
    if (k && k.length === 1) return k.toLowerCase();
    var c = ev.code;
    if (c && c.length === 4 && c.indexOf("Key") === 0) return c.charAt(3).toLowerCase();
    return "";
  }

  function onKeydown(ev) {
    if (ev.isComposing) return;
    var el = ev.target;
    if (!isTextField(el)) return;
    if (el.selectionStart == null || el.selectionEnd == null) return;

    // macOS: Cmd+A = select all. Native behavior is often missing or flaky in CEF; apply explicitly.
    if (ev.metaKey && !ev.ctrlKey && !ev.altKey) {
      var keyIsA =
        (ev.key && ev.key.length === 1 && ev.key.toLowerCase() === "a") || ev.code === "KeyA";
      if (keyIsA) {
        ev.preventDefault();
        ev.stopPropagation();
        try {
          el.setSelectionRange(0, el.value.length, "forward");
        } catch (_) {}
        return;
      }
    }

    if (!controlDown(ev)) return;

    var ch = emacsLetter(ev);
    if (!ch) return;

    var v = el.value;
    var a = el.selectionStart;
    var b = el.selectionEnd;
    var pos = a;

    function apply() {
      ev.preventDefault();
      ev.stopPropagation();
    }

    if (a !== b) {
      if (ch === "h") {
        apply();
        el.value = v.slice(0, a) + v.slice(b);
        setCaret(el, a);
        bumpInput(el);
        return;
      }
      if (ch === "d") {
        apply();
        el.value = v.slice(0, a) + v.slice(b);
        setCaret(el, a);
        bumpInput(el);
        return;
      }
    }

    if (ch === "a") {
      apply();
      setCaret(el, lineStart(v, pos));
      return;
    }
    if (ch === "e") {
      apply();
      setCaret(el, lineEnd(v, pos));
      return;
    }
    if (ch === "b") {
      if (pos <= 0) return;
      apply();
      setCaret(el, pos - 1);
      return;
    }
    if (ch === "f") {
      if (pos >= v.length) return;
      apply();
      setCaret(el, pos + 1);
      return;
    }
    if (ch === "n") {
      apply();
      setCaret(el, nextLinePos(v, pos));
      return;
    }
    if (ch === "p") {
      apply();
      setCaret(el, prevLinePos(v, pos));
      return;
    }
    if (ch === "d") {
      if (pos >= v.length) return;
      apply();
      el.value = v.slice(0, pos) + v.slice(pos + 1);
      setCaret(el, pos);
      bumpInput(el);
      return;
    }
    if (ch === "h") {
      if (pos <= 0) return;
      apply();
      el.value = v.slice(0, pos - 1) + v.slice(pos);
      setCaret(el, pos - 1);
      bumpInput(el);
      return;
    }
    if (ch === "k") {
      apply();
      var le = lineEnd(v, pos);
      if (pos < le) {
        killRing = v.slice(pos, le);
        el.value = v.slice(0, pos) + v.slice(le);
        setCaret(el, pos);
        bumpInput(el);
        return;
      }
      if (le < v.length && v.charAt(le) === "\n") {
        killRing = "\n";
        el.value = v.slice(0, le) + v.slice(le + 1);
        setCaret(el, pos);
        bumpInput(el);
        return;
      }
      killRing = "";
      return;
    }
    if (ch === "u") {
      apply();
      var ls = lineStart(v, pos);
      killRing = v.slice(ls, pos);
      el.value = v.slice(0, ls) + v.slice(pos);
      setCaret(el, ls);
      bumpInput(el);
      return;
    }
    if (ch === "w") {
      if (pos <= 0) return;
      apply();
      var pw = prevWord(v, pos);
      killRing = v.slice(pw, pos);
      el.value = v.slice(0, pw) + v.slice(pos);
      setCaret(el, pw);
      bumpInput(el);
      return;
    }
    if (ch === "y") {
      if (!killRing) return;
      apply();
      el.value = v.slice(0, pos) + killRing + v.slice(pos);
      setCaret(el, pos + killRing.length);
      bumpInput(el);
      return;
    }
  }

  document.addEventListener("keydown", onKeydown, true);
})();
