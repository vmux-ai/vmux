//! What a natively-hosted page finds on `window`.
//!
//! Deliberately the same two verbs a page in the wasm bundle finds. `vmux_ui::transport` picks its
//! `PageHost` at runtime, so the page half of this is a matter of answering to whichever object is
//! present, not of teaching the pages a second protocol.
//!
//! The rest is what a natively-hosted page has to do for itself, because nothing evaluates script
//! into it: pull its own edits, apply the element requests its components queued, and put what is
//! selected on the request for the event that needs to know.

pub(crate) const WRY_HOST_SHIM: &str = r#"
(function () {
  const report = (kind, text) => {
    try { window.ipc.postMessage('log:' + kind + ':' + text); } catch (e) {}
  };
  // What is selected travels on the event's own request. A handler settles preventDefault before
  // it returns, so it cannot wait to be told, and anything posted separately can reach the host
  // after the decision it was meant to inform. dioxus sends that request from a module-local
  // function, so there is no method to override and the headers go on the XHR itself — which is
  // also what makes them the event's own, since `send` runs inside the dispatch.
  const encoder = new TextEncoder();
  const nativeOpen = XMLHttpRequest.prototype.open;
  const nativeSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    this.__vmuxEvent = String(url).endsWith('/__events');
    return nativeOpen.call(this, method, url, ...rest);
  };
  XMLHttpRequest.prototype.send = function (body) {
    if (this.__vmuxEvent) {
      // A field's own selection and the document's are separate facts, and a page asks both: one
      // decides whether Up moves the caret or recalls, the other whether Ctrl+C copies.
      const selected = !(document.getSelection() || { isCollapsed: true }).isCollapsed;
      this.setRequestHeader('x-vmux-selected', selected ? '1' : '0');
      const el = document.activeElement;
      // An id outside this set would not survive a header, and a field the host cannot name is one
      // it cannot answer about anyway. Offsets in UTF-8 bytes, the unit Rust counts in.
      if (el && typeof el.selectionStart === 'number' && /^[\w:.-]+$/.test(el.id)) {
        const bytes = (upto) => encoder.encode(el.value.slice(0, upto)).length;
        this.setRequestHeader(
          'x-vmux-caret',
          el.id + ':' + bytes(el.selectionStart) + ':' + bytes(el.selectionEnd),
        );
      }
    }
    return nativeSend.call(this, body);
  };
  window.addEventListener('error', (e) => {
    report('error', (e.message || 'error') + ' @ ' + (e.filename || '?') + ':' + (e.lineno || 0));
  });
  window.addEventListener('unhandledrejection', (e) => {
    report('reject', String((e.reason && e.reason.stack) || e.reason));
  });
  for (const level of ['error', 'warn', 'log']) {
    const original = console[level].bind(console);
    console[level] = (...args) => {
      report(level, args.map((a) => {
        if (a instanceof Error) return a.stack || a.message;
        if (typeof a === 'object') { try { return JSON.stringify(a); } catch (e) { return String(a); } }
        return String(a);
      }).join(' '));
      original(...args);
    };
  }
  const listeners = new Map();
  function toBase64(buffer) {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.length; i += 1) binary += String.fromCharCode(bytes[i]);
    return btoa(binary);
  }
  function fromBase64(text) {
    const binary = atob(text);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
    return bytes.buffer;
  }
  // Everything the host may ask to be done to an element, and nothing else. The host queues these
  // as data and the page collects them here once a batch has landed, so no statement composed on
  // the Rust side is ever evaluated.
  // Four numbers, or none at all. The interpreter's own getters, so the node ids agree with the
  // ones the host queued and nothing here has to find an element for itself.
  const measureNode = (node, what) => {
    const i = window.interpreter;
    if (what === 'rect') {
      const rect = i.getClientRect(node);
      return rect ? [rect.origin[0], rect.origin[1], rect.size[0], rect.size[1]] : [];
    }
    const pair =
      what === 'scrollSize'
        ? [i.getScrollWidth(node), i.getScrollHeight(node)]
        : [i.getScrollLeft(node), i.getScrollTop(node)];

    return pair.some((n) => n === undefined) ? [] : [pair[0], pair[1], 0, 0];
  };
  // Which character of an element's text a point falls on. Counted in code points, the unit Rust
  // counts in, and scoped to the element the host named — the engine reports the caret against
  // whichever descendant text node holds it, and what the page asked for is the offset in the run.
  const textOffsetAtPoint = (element, x, y) => {
    const el = document.getElementById(element);
    if (!el) return [];
    let node = null;
    let offset = 0;
    if (document.caretPositionFromPoint) {
      const position = document.caretPositionFromPoint(x, y);
      if (position) { node = position.offsetNode; offset = position.offset; }
    } else if (document.caretRangeFromPoint) {
      const range = document.caretRangeFromPoint(x, y);
      if (range) { node = range.startContainer; offset = range.startOffset; }
    }
    if (node && el.contains(node)) {
      const range = document.createRange();
      range.selectNodeContents(el);
      range.setEnd(node, offset);
      return [[...range.cloneContents().textContent].length, 0, 0, 0];
    }
    // The point missed the text: past the end of a short line, or over the padding beside it. How
    // far along the box it sits is the best answer left, and for proportional text only an estimate.
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0) return [0, 0, 0, 0];
    const length = [...(el.textContent || '')].length;
    const ratio = Math.min(Math.max((x - rect.left) / rect.width, 0), 1);
    return [Math.round(ratio * length), 0, 0, 0];
  };
  const applyDomRequest = (request) => {
    // A request naming a node rather than an id came from a component holding a MountedData, and
    // what it wants is a method the interpreter already has for the node it assigned.
    switch (request.kind) {
      case 'focusNode':
        window.interpreter.setFocus(request.node, request.focus);
        return;
      case 'scrollNode':
        window.interpreter.scroll(request.node, request.x, request.y, request.behavior);
        return;
      case 'revealNode':
        window.interpreter.scrollTo(request.node, {
          behavior: request.behavior,
          block: request.block,
          inline: request.inline,
        });
        return;
      // The only request that answers. An empty list says the node was gone by the time we looked,
      // which the host turns back into a refusal rather than into a measurement of zero.
      case 'measureNode':
        window.ipc.postMessage(
          'measured:' + request.token + ':' + measureNode(request.node, request.what).join(','),
        );
        return;
      // Answers the same way, and an empty list means the same thing.
      case 'textOffsetAtPoint':
        window.ipc.postMessage(
          'measured:' +
            request.token +
            ':' +
            textOffsetAtPoint(request.element, request.x, request.y).join(','),
        );
        return;
    }
    const el = document.getElementById(request.element);
    if (!el) return;
    switch (request.kind) {
      case 'focus':
        el.focus();
        break;
      case 'scrollIntoView':
        el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        break;
      case 'revealElement':
        el.scrollIntoView({ block: request.block, inline: 'nearest' });
        break;
      case 'selectAll':
        el.setSelectionRange(0, el.value.length);
        break;
      // A frame later than the rest, because focusing a field may move the selection itself.
      case 'offerText':
        requestAnimationFrame(() => {
          el.focus();
          el.setSelectionRange(0, el.value.length);
          el.scrollLeft = 0;
        });
        break;
      // The offset is in UTF-8 bytes and `setSelectionRange` counts UTF-16 units, so the value is
      // re-encoded and cut where the host cut it. The cut is on a character boundary already —
      // `TextCaret::place` floors it — so the decode cannot land mid-character.
      case 'placeCaret': {
        const bytes = new TextEncoder().encode(el.value).slice(0, request.byte);
        const index = new TextDecoder().decode(bytes).length;
        el.setSelectionRange(index, index);
        break;
      }
    }
  };
  // The page asks for its own frames rather than having them evaluated into it. The host holds the
  // request until a render produces one, so this loop costs one idle connection and no polling.
  //
  // A frame is [u32 le: requests length][requests json][edits], so the prefix is what says where
  // the edits begin. The requests are applied after the batch, so an element a component asked to
  // focus exists to be found, and before the acknowledgement, which is what releases the render
  // that would replace it. A frame with no edits still arrives: a request for the caret gives the
  // page nothing to draw.
  const pumpEdits = async () => {
    let applied = false;
    for (;;) {
      try {
        // Asking for the next frame is how the last one is acknowledged: the two always happened
        // together, and sending them apart left the host waking itself to notice the ack.
        const response = await fetch('/__edits', {
          headers: { 'x-vmux-applied': applied ? '1' : '0' },
        });
        applied = false;
        if (!response.ok) { await new Promise((r) => setTimeout(r, 50)); continue; }
        const frame = await response.arrayBuffer();
        if (frame.byteLength < 4) continue;
        const length = new DataView(frame).getUint32(0, true);
        const edits = frame.slice(4 + length);
        if (edits.byteLength) window.interpreter.run_from_bytes(edits);
        if (length) {
          const json = new TextDecoder().decode(new Uint8Array(frame, 4, length));
          for (const queued of JSON.parse(json)) applyDomRequest(queued);
        }
        applied = edits.byteLength > 0;
      } catch (e) {
        await new Promise((r) => setTimeout(r, 50));
      }
    }
  };
  window.vmuxWry = {
    // Asking for a frame is what tells the host the page can take one, so the shell calls this
    // once the interpreter holds a root and never before.
    start: pumpEdits,
    binEmit(buffer) { window.ipc.postMessage(toBase64(buffer)); },
    binListen(id, callback) {
      const existing = listeners.get(id) || [];
      existing.push(callback);
      listeners.set(id, existing);
    },
    _dispatch(id, base64) {
      const buffer = fromBase64(base64);
      for (const callback of listeners.get(id) || []) callback(buffer);
    },
  };
})();
"#;
