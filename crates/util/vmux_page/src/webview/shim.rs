use crate::HistorySwipe;
use vmux_ui::components::popup::Surface;
use vmux_ui::motion::Motion;

pub(super) struct Shim;

impl Shim {
    pub(super) fn source() -> String {
        WRY_HOST_SHIM
            .replace(
                "__VMUX_HISTORY_SWIPE__",
                &HistorySwipe::script("postMessage('history:' + direction);"),
            )
            .replace("__VMUX_POPUP_RUNTIME__", &Surface::runtime())
            .replace(
                "__VMUX_MOTION_CONTENT_MS__",
                &Motion::CONTENT_MS.to_string(),
            )
            .replace("__VMUX_MOTION_LAYOUT_MS__", &Motion::LAYOUT_MS.to_string())
            .replace("__VMUX_MOTION_MAX_MS__", &Motion::MAX_MS.to_string())
    }
}

pub(crate) const WRY_HOST_SHIM: &str = r#"
(function () {
  __VMUX_POPUP_RUNTIME__
  const documentIdentity = document;
  globalThis.__vmuxIpcEpoch = 1;
  let navigationRevision = null;
  let applyingRevision = null;
  let applyingEpoch = null;
  let appliedFrame = null;
  let failedFrame = null;
  let failedRevision = null;
  const applyingMessages = [];
  const applyingMetadata = new Map();
  const queueMessage = (messages, metadata, message) => {
    const kind = message.body.startsWith('title:')
      ? 'title'
      : message.body.startsWith('favicon:')
        ? 'favicon'
        : message.body.startsWith('log:')
          ? 'log'
          : null;
    if (kind) {
      metadata.set(kind, message);
      return;
    }
    messages.push(message);
  };
  const sendMessage = (message) => {
    window.ipc.postMessage(message.epoch + ':' + message.revision + ':' + message.body);
  };
  const flushMessages = (messages, metadata) => {
    for (const message of metadata.values()) {
      sendMessage(message);
    }
    metadata.clear();
    for (const message of messages.splice(0)) {
      sendMessage(message);
    }
  };
  const postMessage = (body) => {
    const revision = applyingRevision ?? navigationRevision;
    if (revision === null) return;
    const message = {
      epoch: applyingEpoch ?? globalThis.__vmuxIpcEpoch,
      revision,
      body,
    };
    if (applyingRevision !== null) {
      queueMessage(applyingMessages, applyingMetadata, message);
      return;
    }
    sendMessage(message);
  };
  const commitNavigationRevision = (revision) => {
    const changed = navigationRevision !== revision;
    navigationRevision = revision;
    applyingRevision = null;
    applyingEpoch = null;
    flushMessages(applyingMessages, applyingMetadata);
    return changed;
  };
  __VMUX_HISTORY_SWIPE__
  const abandonApplyingRevision = () => {
    applyingRevision = null;
    applyingEpoch = null;
    applyingMessages.length = 0;
    applyingMetadata.clear();
  };
  const report = (kind, text) => {
    try { postMessage('log:' + kind + ':' + text); } catch (e) {}
  };
  const encoder = new TextEncoder();
  const nativeOpen = XMLHttpRequest.prototype.open;
  const nativeSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function (method, url, ...rest) {
    this.__vmuxEvent = String(url).endsWith('/__events');
    return nativeOpen.call(this, method, url, ...rest);
  };
  XMLHttpRequest.prototype.send = function (body) {
    if (!this.__vmuxEvent) {
      return nativeSend.call(this, body);
    }
    const selected = !(document.getSelection() || { isCollapsed: true }).isCollapsed;
    this.setRequestHeader('x-vmux-selected', selected ? '1' : '0');
    this.setRequestHeader('x-vmux-ipc-epoch', applyingEpoch ?? globalThis.__vmuxIpcEpoch);
    const el = document.activeElement;
    if (el && typeof el.selectionStart === 'number' && /^[\w:.-]+$/.test(el.id)) {
      const bytes = (upto) => encoder.encode(el.value.slice(0, upto)).length;
      this.setRequestHeader(
        'x-vmux-caret',
        el.id + ':' + bytes(el.selectionStart) + ':' + bytes(el.selectionEnd),
      );
    }
    const revision = applyingRevision ?? navigationRevision;
    if (revision !== null) {
      this.setRequestHeader('x-vmux-navigation', revision);
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
  let caretRuler = null;
  const scrollCaretIntoView = (el, index) => {
    if (el.scrollWidth <= el.clientWidth) return;
    const style = getComputedStyle(el);
    caretRuler = caretRuler || document.createElement('canvas');
    const pen = caretRuler.getContext('2d');
    pen.font = style.font;
    const caret = pen.measureText(el.value.slice(0, index)).width;
    const view =
      el.clientWidth - (parseFloat(style.paddingLeft) || 0) - (parseFloat(style.paddingRight) || 0);
    const margin = view / 4;
    if (caret < el.scrollLeft) el.scrollLeft = Math.max(0, caret - margin);
    else if (caret > el.scrollLeft + view) el.scrollLeft = caret - view + margin;
  };
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
    const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
    let nearest = null;
    let distance = Infinity;
    for (let text = walker.nextNode(); text; text = walker.nextNode()) {
      const range = document.createRange();
      range.selectNodeContents(text);
      for (const rect of range.getClientRects()) {
        if (rect.width <= 0 || rect.height <= 0) continue;
        const dx = Math.max(rect.left - x, x - rect.right, 0);
        const dy = Math.max(rect.top - y, y - rect.bottom, 0);
        const squared = dx * dx + dy * dy;
        if (squared < distance) { distance = squared; nearest = rect; }
      }
    }
    if (nearest) {
      const px = Math.min(Math.max(x, nearest.left + 0.25), nearest.right - 0.25);
      const py = Math.min(Math.max(y, nearest.top + 0.25), nearest.bottom - 0.25);
      if (document.caretPositionFromPoint) {
        const position = document.caretPositionFromPoint(px, py);
        if (position) { node = position.offsetNode; offset = position.offset; }
      } else if (document.caretRangeFromPoint) {
        const range = document.caretRangeFromPoint(px, py);
        if (range) { node = range.startContainer; offset = range.startOffset; }
      }
      if (node && el.contains(node)) {
        const range = document.createRange();
        range.selectNodeContents(el);
        range.setEnd(node, offset);
        return [[...range.cloneContents().textContent].length, 0, 0, 0];
      }
    }
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0) return [0, 0, 0, 0];
    const length = [...(el.textContent || '')].length;
    const ratio = Math.min(Math.max((x - rect.left) / rect.width, 0), 1);
    return [Math.round(ratio * length), 0, 0, 0];
  };
  const prepareRemount = () => {
    const previous = window.interpreter;
    const root = previous.root.cloneNode(false);
    const interpreter = window.vmuxCreateInterpreter();
    interpreter.initialize(root);
    return { previousRoot: previous.root, root, interpreter };
  };
  const sectionAtTop = (element) => {
    const container = document.getElementById(element);
    if (!container) return [];
    const bounds = container.getBoundingClientRect();
    if (bounds.height <= 0) return [];
    const top = bounds.top + 32;
    let selected = null;
    for (const section of container.querySelectorAll('[data-vmux-scroll-section]')) {
      const rect = section.getBoundingClientRect();
      if (rect.height <= 0) continue;
      if (selected === null || rect.top <= top) selected = section;
      if (rect.top > top) break;
    }
    if (selected === null) return [];
    return [Number(selected.dataset.vmuxScrollSection), 0, 0, 0];
  };
  const commitRemount = (remount, revision) => {
    remount.previousRoot.replaceWith(remount.root);
    window.interpreter = remount.interpreter;
    return commitNavigationRevision(revision);
  };
  const visibleScrollAnchor = (container, viewport) => {
    const x = Math.min(viewport.right - 1, viewport.left + 8);
    const y = Math.min(viewport.bottom - 1, viewport.top + 1);
    const anchor = document.elementsFromPoint(x, y)
      .map(element => element.closest('[data-vmux-scroll-anchor]'))
      .find(element => element && container.contains(element));
    if (!anchor) return null;
    const rect = anchor.getBoundingClientRect();
    return anchor.id ? { id: anchor.id, top: rect.top, scrollTop: container.scrollTop } : null;
  };
  const captureScrollAnchors = () => {
    const captures = [];
    const containers = document.querySelectorAll('[data-vmux-preserve-scroll]');
    for (let containerIndex = 0; containerIndex < containers.length; containerIndex += 1) {
      const container = containers[containerIndex];
      const viewport = container.getBoundingClientRect();
      const anchor = visibleScrollAnchor(container, viewport);
      if (anchor) captures.push({ container, id: anchor.id, top: anchor.top, scrollTop: anchor.scrollTop });
    }
    return captures;
  };
  const restoreScrollAnchors = async (captures) => {
    if (!captures.length) return;
    const changed = captures.some(capture => {
      if (!capture.container.isConnected) return false;
      if (Math.abs(capture.container.scrollTop - capture.scrollTop) > 0.5) return false;
      const anchor = document.getElementById(capture.id);
      return anchor && capture.container.contains(anchor)
        && Math.abs(anchor.getBoundingClientRect().top - capture.top) > 0.5;
    });
    if (!changed) return;
    await new Promise((resolve) => requestAnimationFrame(resolve));
    for (const capture of captures) {
      if (!capture.container.isConnected) continue;
      if (Math.abs(capture.container.scrollTop - capture.scrollTop) > 0.5) continue;
      const anchor = document.getElementById(capture.id);
      if (!anchor || !capture.container.contains(anchor)) continue;
      const delta = anchor.getBoundingClientRect().top - capture.top;
      if (Math.abs(delta) <= 0.5) continue;
      capture.container.scrollTop += delta;
    }
  };
  const requestMatchesDocument = (request) =>
    String(request.epoch) === String(globalThis.__vmuxIpcEpoch)
      && String(request.revision) === String(applyingRevision ?? navigationRevision);
  class HeightAnimation {
    static active = new WeakMap();
    static capture(requests) {
      const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
      const captures = [];
      for (const request of requests) {
        if (request.kind !== 'animateHeight') continue;
        const element = document.getElementById(request.element);
        if (!element?.isConnected) continue;
        const rect = element.getBoundingClientRect();
        const body = element.querySelector('[data-vmux-height-body]');
        const bodyRect = body?.getBoundingClientRect();
        const container = element.closest('[data-vmux-preserve-scroll]');
        const viewport = container?.getBoundingClientRect();
        const header = body?.previousElementSibling;
        const headerRect = header?.getBoundingClientRect();
        const pinned = headerRect && viewport && Math.abs(headerRect.top - viewport.top) <= 2;
        const bodyTop = bodyRect ? bodyRect.top - rect.top : 0;
        const visibleOffset = bodyRect && viewport ? Math.max(0, viewport.top + bodyTop - bodyRect.top) : 0;
        const previous = this.active.get(element);
        if (previous) {
          previous.animation.cancel();
          this.finish(element, previous);
        }
        captures.push({ request, element, height: rect.height, body: reduced ? null : body?.cloneNode(true), bodyTop, visibleOffset, container, header, pinned, viewportHeight: viewport?.height, motion: !reduced && typeof element.animate === 'function' });
      }
      return captures;
    }
    static finish(element, state) {
      state.overlay?.remove();
      if (this.active.get(element) !== state) return;
      element.style.position = state.position;
      element.style.overflow = state.overflow;
      this.active.delete(element);
      state.animation.cancel();
    }
    static align(capture) {
      const { element, container, header, pinned } = capture;
      if (!container?.isConnected || !header?.isConnected) return;
      const top = Math.max(0, element.getBoundingClientRect().top - container.getBoundingClientRect().top + container.scrollTop);
      if (Math.abs(top - container.scrollTop) <= 1) return;
      if (pinned) container.scrollTop = top;
      else container.scrollTo({ top, behavior: 'instant' });
    }
    static apply(captures) {
      for (const capture of captures) {
        const { element, request } = capture;
        if (!element.isConnected || !requestMatchesDocument(request)) continue;
        const height = element.getBoundingClientRect().height;
        if (!Number.isFinite(height)) continue;
        if (!capture.motion || Math.abs(height - capture.height) < 0.5) {
          this.align(capture);
          continue;
        }
        const state = { position: element.style.position, overflow: element.style.overflow, overlay: null, animation: null };
        try {
          if (height < capture.height && capture.body) {
            const overlay = capture.body;
            overlay.removeAttribute('data-vmux-height-body');
            for (const node of [overlay, ...overlay.querySelectorAll('[id], [data-dioxus-id]')]) {
              node.removeAttribute('id');
              node.removeAttribute('data-dioxus-id');
            }
            overlay.inert = true;
            overlay.setAttribute('aria-hidden', 'true');
            Object.assign(overlay.style, { position: 'absolute', top: capture.bodyTop + 'px', left: '0', width: '100%', pointerEvents: 'none', transform: 'translateY(-' + capture.visibleOffset + 'px)' });
            element.appendChild(overlay);
            state.overlay = overlay;
          }
          element.style.position = 'relative';
          element.style.overflow = 'clip';
          const limit = Math.max(36, capture.viewportHeight || Math.max(capture.height, height));
          state.animation = element.animate([{ height: Math.min(capture.height, limit) + 'px' }, { height: Math.min(height, limit) + 'px' }], {
            duration: Math.min(__VMUX_MOTION_MAX_MS__, request.duration_ms), easing: 'cubic-bezier(0.2, 0.7, 0.2, 1)', fill: 'both',
          });
          this.active.set(element, state);
          state.animation.finished.then(() => this.finish(element, state), () => this.finish(element, state));
          this.align(capture);
        } catch (_) {
          state.overlay?.remove();
          element.style.position = state.position;
          element.style.overflow = state.overflow;
        }
      }
    }
  }
  class LayoutMovement {
    static active = new WeakMap();
    static capture(requests) {
      if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return [];
      const captures = [];
      for (const request of requests) {
        if (request.kind !== 'animateLayout') continue;
        const container = document.getElementById(request.element);
        if (!container) continue;
        const rects = new Map();
        for (const element of container.querySelectorAll('[data-vmux-stack-row]')) {
          const rect = element.getBoundingClientRect();
          rects.set(element.dataset.vmuxStackRow, { left: rect.left, top: rect.top });
          this.active.get(element)?.cancel();
          this.active.delete(element);
        }
        captures.push({ request, rects });
      }
      return captures;
    }
    static apply(captures) {
      for (const { request, rects } of captures) {
        if (!requestMatchesDocument(request)) continue;
        const container = document.getElementById(request.element);
        if (!container) continue;
        for (const element of container.querySelectorAll('[data-vmux-stack-row]')) {
          const previous = rects.get(element.dataset.vmuxStackRow);
          if (!previous || typeof element.animate !== 'function') continue;
          const next = element.getBoundingClientRect();
          const dx = previous.left - next.left;
          const dy = previous.top - next.top;
          if (Math.abs(dx) + Math.abs(dy) < 0.5) continue;
          const animation = element.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'translate(0px, 0px)' }], { duration: Math.min(__VMUX_MOTION_MAX_MS__, request.duration_ms), easing: 'cubic-bezier(0.2, 0, 0, 1)' });
          this.active.set(element, animation);
          animation.finished.finally(() => { if (this.active.get(element) === animation) this.active.delete(element); }).catch(() => {});
        }
      }
    }
  }
  const applyDomRequest = (request) => {
    if (!requestMatchesDocument(request)) return;
    switch (request.kind) {
      case 'remount':
        return;
      case 'preserveScroll':
      case 'animateHeight':
      case 'animateLayout':
        return;
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
      case 'measureNode':
        postMessage(
          'measured:' + request.token + ':' + measureNode(request.node, request.what).join(','),
        );
        return;
      case 'revealElement':
        requestAnimationFrame(() => {
          if (!requestMatchesDocument(request)) return;
          for (const id of request.elements) {
            const target = document.getElementById(id);
            if (target) {
              target.scrollIntoView({ block: request.block, inline: 'nearest', behavior: 'instant' });
              break;
            }
          }
        });
        return;
      case 'textOffsetAtPoint':
        postMessage(
          'measured:' +
            request.token +
            ':' +
            textOffsetAtPoint(request.element, request.x, request.y).join(','),
        );
        return;
      case 'sectionAtTop':
        postMessage('measured:' + request.token + ':' + sectionAtTop(request.element).join(','));
        return;
      case 'scrollSection':
        requestAnimationFrame(() => {
          if (!requestMatchesDocument(request)) return;
          const container = document.getElementById(request.container);
          const element = document.getElementById(request.element);
          if (!container || !element || !container.contains(element)) return;
          const top = element.getBoundingClientRect().top - container.getBoundingClientRect().top + container.scrollTop - 16;
          container.scrollTo({ top: Math.max(0, top), behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : request.behavior });
        });
        return;
      case 'runScript':
        Function(request.script)();
        return;
    }
    const el = document.getElementById(request.element);
    if (!el) return;
    switch (request.kind) {
      case 'focus':
        el.focus({ preventScroll: true });
        break;
      case 'showPopover':
        if (el.isConnected && !el.matches(':popover-open')) {
          try { el.showPopover(); } catch (e) {}
        }
        break;
      case 'toggleMedia':
        if (el.paused) { el.play(); } else { el.pause(); }
        break;
      case 'scrollIntoView':
        el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
        break;
      case 'scrollIntoViewVertical': {
        const horizontal = [];
        for (let node = el.parentElement; node; node = node.parentElement) {
          if (node.scrollWidth > node.clientWidth) horizontal.push([node, node.scrollLeft]);
        }
        el.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'instant' });
        for (const [node, left] of horizontal) node.scrollLeft = left;
        break;
      }
      case 'scrollTo':
        el.scrollTo({ top: request.top, behavior: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : request.behavior });
        break;
      case 'selectAll':
        el.setSelectionRange(0, el.value.length);
        break;
      case 'offerText':
        requestAnimationFrame(() => {
          if (!requestMatchesDocument(request)) return;
          el.focus();
          el.setSelectionRange(0, el.value.length);
          el.scrollLeft = 0;
        });
        break;
      case 'placeCaret': {
        const bytes = new TextEncoder().encode(el.value).slice(0, request.byte);
        const index = new TextDecoder().decode(bytes).length;
        el.setSelectionRange(index, index);
        scrollCaretIntoView(el, index);
        break;
      }
      case 'caretToEnd':
        requestAnimationFrame(() => {
          if (!requestMatchesDocument(request)) return;
          const end = el.value.length;
          el.setSelectionRange(end, end);
          el.scrollLeft = el.scrollWidth;
          el.scrollTop = el.scrollHeight;
        });
        break;
    }
  };
  document.addEventListener('paste', (event) => {
    const el = event.target;
    if (!(el instanceof HTMLInputElement) && !(el instanceof HTMLTextAreaElement)) return;
    const data = event.clipboardData;
    if (!data) return;
    const items = Array.from(data.items || []);
    const carriesImage = items.some((item) => item.type.startsWith('image/'))
      || (data.files && data.files.length > 0);
    if (carriesImage) event.preventDefault();
  }, true);
  document.addEventListener('contextmenu', (event) => {
    const target = event.target;
    if (!(target instanceof Element)) {
      event.preventDefault();
      return;
    }
    const editable = target.closest('input, textarea, [contenteditable]:not([contenteditable="false"])');
    if (!editable) event.preventDefault();
  }, true);
  const pumpEdits = async () => {
    let retryDelay = 50;
    let lastError = '';
    for (;;) {
      let frameSequence = null;
      let frameRevision = null;
      try {
        const headers = {
          'x-vmux-applied-frame': appliedFrame || '0',
          'x-vmux-navigation': failedRevision ?? navigationRevision ?? '0',
          'x-vmux-frame-epoch': String(globalThis.__vmuxIpcEpoch),
          'x-vmux-frame-capability': String(globalThis.__vmuxFrameCapability),
        };
        if (failedFrame !== null) {
          headers['x-vmux-failed-frame'] = failedFrame;
        }
        const response = await fetch('/__edits', {
          headers,
        });
        if (document !== documentIdentity) return;
        const revision = response.headers.get('x-vmux-navigation');
        const responseEpoch = response.headers.get('x-vmux-frame-epoch');
        const responseCapability = response.headers.get('x-vmux-frame-capability');
        if (response.status === 412) {
          if (
            revision !== null
            && responseEpoch !== null
            && responseCapability === String(globalThis.__vmuxFrameCapability)
          ) {
            navigationRevision = revision;
            globalThis.__vmuxIpcEpoch = Number(responseEpoch);
            abandonApplyingRevision();
            const pendingFrame = response.headers.get('x-vmux-frame');
            if (appliedFrame !== pendingFrame) appliedFrame = null;
            if (failedFrame !== pendingFrame) {
              failedFrame = null;
              failedRevision = null;
            } else if (failedFrame !== null) {
              failedRevision = revision;
            }
            if (appliedFrame === null && failedFrame === null && pendingFrame !== null && pendingFrame !== '0') {
              failedFrame = pendingFrame;
              failedRevision = revision;
            }
            retryDelay = 50;
            continue;
          }
          throw new Error('edits resynchronization omitted its identity');
        }
        if (response.status === 409) {
          report('error', 'page frame failed after repeated attempts');
          abandonApplyingRevision();
          failedFrame = null;
          failedRevision = null;
          await new Promise((resolve) => setTimeout(resolve, retryDelay));
          retryDelay = Math.min(retryDelay * 2, 2000);
          continue;
        }
        if (!response.ok) throw new Error('edits request failed with ' + response.status);
        appliedFrame = null;
        if (!revision) continue;
        if (responseCapability !== String(globalThis.__vmuxFrameCapability)) {
          throw new Error('edits response belongs to a stale document');
        }
        frameRevision = revision;
        frameSequence = response.headers.get('x-vmux-frame');
        if (!frameSequence) throw new Error('edits response omitted its frame sequence');
        const frame = await response.arrayBuffer();
        if (document !== documentIdentity) return;
        failedFrame = null;
        failedRevision = null;
        if (frame.byteLength < 4) throw new Error('edits frame is shorter than its header');
        const length = new DataView(frame).getUint32(0, true);
        if (length > frame.byteLength - 4) throw new Error('edits frame has an invalid request length');
        const edits = frame.slice(4 + length);
        let requests = [];
        let remount = null;
        if (length) {
          const json = new TextDecoder().decode(new Uint8Array(frame, 4, length));
          requests = JSON.parse(json);
          if (!Array.isArray(requests)) throw new Error('edits requests are not an array');
          requests = requests.filter(
            (request) =>
              String(request.revision) === String(revision)
                && (request.kind === 'remount' || String(request.epoch) === String(globalThis.__vmuxIpcEpoch)),
          );
          for (const queued of requests) {
            if (queued.kind === 'remount') remount = prepareRemount();
          }
        }
        const preserveScroll = requests.some((request) => request.kind === 'preserveScroll');
        const heightAnimations = remount ? [] : HeightAnimation.capture(requests);
        const popupCaptures = edits.byteLength || remount ? globalThis.__vmuxPopupPresentation.capture() : [];
        const layoutMovements = remount ? [] : LayoutMovement.capture(requests);
        const scrollAnchors = remount || !edits.byteLength || !preserveScroll
          ? []
          : captureScrollAnchors();
        applyingRevision = revision;
        applyingEpoch = responseEpoch;
        let navigated = false;
        if (remount) {
          if (edits.byteLength) remount.interpreter.run_from_bytes(edits);
          navigated = commitRemount(remount, revision);
        } else if (edits.byteLength) {
          window.interpreter.run_from_bytes(edits);
        }
        globalThis.__vmuxPopupPresentation.apply(popupCaptures);
        HeightAnimation.apply(heightAnimations);
        LayoutMovement.apply(layoutMovements);
        await restoreScrollAnchors(scrollAnchors);
        if (document !== documentIdentity) return;
        if (responseCapability !== String(globalThis.__vmuxFrameCapability)) {
          throw new Error('edits response became stale while applying');
        }
        if (responseEpoch !== String(globalThis.__vmuxIpcEpoch)) {
          abandonApplyingRevision();
          appliedFrame = frameSequence;
          failedFrame = null;
          failedRevision = null;
          retryDelay = 50;
          lastError = '';
          continue;
        }
        for (const queued of requests) {
          if (queued.kind === 'remount') continue;
          try {
            applyDomRequest(queued);
          } catch (error) {
            report('error', 'dom request ' + queued.kind + ': ' + String(error));
          }
        }
        if (!remount) navigated = commitNavigationRevision(revision);
        if (navigated) reportMetadata();
        appliedFrame = frameSequence;
        failedFrame = null;
        failedRevision = null;
        retryDelay = 50;
        lastError = '';
      } catch (e) {
        abandonApplyingRevision();
        if (frameSequence !== null) {
          failedFrame = frameSequence;
          failedRevision = frameRevision;
        }
        const interpreter = window.interpreter;
        const operation = interpreter ? interpreter.op & 255 : null;
        const operand = interpreter && interpreter.u32buf ? interpreter.u32buf[interpreter.u32bufp - 1] : null;
        const message = [e && e.name, e && e.message, e && e.stack].filter(Boolean).join(': ') +
          ' frame=' + frameSequence + ' revision=' + frameRevision + ' operation=' + operation + ' operand=' + operand;
        if (message !== lastError) {
          report('error', 'pumpEdits: ' + message);
          lastError = message;
        }
        await new Promise((resolve) => setTimeout(resolve, retryDelay));
        retryDelay = Math.min(retryDelay * 2, 2000);
      }
    }
  };
  const sameDocumentFragment = (href) => {
    const upToFragment = (url) => url.split('#')[0];
    try {
      const resolved = new URL(href, document.baseURI).href;
      return resolved.includes('#') && upToFragment(resolved) === upToFragment(location.href);
    } catch (e) {
      return false;
    }
  };
  const holdLinks = () => {
    window.interpreter.intercept_link_redirects = false;
    document.addEventListener('click', (event) => {
      if (event.defaultPrevented) return;
      const anchor = event.target instanceof Element ? event.target.closest('a') : null;
      if (!anchor) return;
      const href = anchor.getAttribute('href');
      if (href === null) return;
      if (sameDocumentFragment(href)) return;
      event.preventDefault();
      postMessage('link:' + href);
    });
  };
  const reportInteractions = () => {
    document.addEventListener('pointerdown', () => {
      postMessage('interaction:');
    }, true);
  };
  let lastTitle;
  let lastIcon;
  const reportMetadata = () => {
    const title = document.title;
    const icon = document.querySelector('link[rel~="icon"][href]');
    const iconHref = icon ? icon.href : '';
    if (title !== lastTitle) {
      lastTitle = title;
      postMessage('title:' + title);
    }
    if (iconHref !== lastIcon) {
      lastIcon = iconHref;
      postMessage('favicon:' + iconHref);
    }
  };
  const watchMetadata = () => {
    let scheduled = false;
    const schedule = () => {
      if (scheduled) return;
      scheduled = true;
      queueMicrotask(() => {
        scheduled = false;
        reportMetadata();
      });
    };
    reportMetadata();
    window.__vmuxMetadataObserver = new MutationObserver(schedule);
    window.__vmuxMetadataObserver.observe(document.head, {
      attributes: true,
      childList: true,
      characterData: true,
      subtree: true,
    });
  };
  let started = false;
  window.vmuxWry = {
    start() {
      if (started) return;
      started = true;
      holdLinks();
      reportInteractions();
      watchMetadata();
      pumpEdits();
    },
    binEmit(buffer) { postMessage(toBase64(buffer)); },
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
