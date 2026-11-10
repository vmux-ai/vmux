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
                "__VMUX_MOTION_FEEDBACK_MS__",
                &Motion::FEEDBACK_MS.to_string(),
            )
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
  class ScrollEvents {
    static installed = new WeakSet();
    static install(interpreter) {
      if (this.installed.has(interpreter)) return;
      this.installed.add(interpreter);
      const synchronous = interpreter.sendSerializedEvent.bind(interpreter);
      const pending = new Map();
      let scheduled = false;
      interpreter.sendSerializedEvent = (body) => {
        if (body.name !== 'scroll' && body.name !== 'scrollend') return synchronous(body);
        const key = body.name + ':' + body.element;
        pending.delete(key);
        pending.set(key, {
          body,
          revision: applyingRevision ?? navigationRevision,
          epoch: applyingEpoch ?? globalThis.__vmuxIpcEpoch,
        });
        if (!scheduled) {
          scheduled = true;
          requestAnimationFrame(() => {
            scheduled = false;
            const events = [...pending.values()];
            pending.clear();
            if (window.interpreter !== interpreter) return;
            for (const event of events) {
              if (!requestMatchesDocument(event)) continue;
              const xhr = new XMLHttpRequest();
              xhr.open('POST', interpreter.eventsPath, true);
              xhr.setRequestHeader('Content-Type', 'application/json');
              xhr.setRequestHeader('dioxus-data', toBase64(encoder.encode(JSON.stringify(event.body))));
              xhr.send();
            }
          });
        }
        return { preventDefault: false, stopPropagation: false };
      };
    }
  }
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
  class ComposerHandoff {
    static active = new WeakMap();
    static capture(root) {
      if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return null;
      const composer = root.querySelector?.('.vmux-agent-composer-shared');
      if (!composer) return null;
      const rect = composer.getBoundingClientRect();
      this.active.get(composer)?.cancel();
      this.active.delete(composer);
      if (![rect.left, rect.top, rect.width, rect.height].every(Number.isFinite) || rect.width <= 0 || rect.height <= 0) return null;
      return rect;
    }
    static apply(previous, root) {
      if (!previous || window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
      const composer = root.querySelector?.('.vmux-agent-composer-shared');
      if (!composer || typeof composer.animate !== 'function') return;
      const next = composer.getBoundingClientRect();
      if (![next.left, next.top, next.width, next.height].every(Number.isFinite) || next.width <= 0 || next.height <= 0) return;
      const dx = previous.left - next.left;
      const dy = previous.top - next.top;
      const sx = previous.width / next.width;
      const sy = previous.height / next.height;
      if (Math.abs(dx) + Math.abs(dy) + Math.abs(previous.width - next.width) + Math.abs(previous.height - next.height) < 0.5) return;
      this.active.get(composer)?.cancel();
      const animation = composer.animate([
        { transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`, transformOrigin: 'top left' },
        { transform: 'translate(0px, 0px) scale(1, 1)', transformOrigin: 'top left' },
      ], { duration: __VMUX_MOTION_LAYOUT_MS__, fill: 'backwards', easing: 'cubic-bezier(0.2, 0, 0, 1)' });
      this.active.set(composer, animation);
      animation.finished.finally(() => { if (this.active.get(composer) === animation) this.active.delete(composer); }).catch(() => {});
    }
  }
  const prepareRemount = () => {
    const previous = window.interpreter;
    let composer = null;
    try { composer = ComposerHandoff.capture(previous.root); } catch (e) {}
    const root = previous.root.cloneNode(false);
    const interpreter = window.vmuxCreateInterpreter();
    interpreter.initialize(root);
    ScrollEvents.install(interpreter);
    return { previousRoot: previous.root, root, interpreter, composer };
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
    const committed = commitNavigationRevision(revision);
    try { ComposerHandoff.apply(remount.composer, remount.root); } catch (e) {}
    return committed;
  };
  const visibleScrollAnchor = (container, viewport) => {
    for (const y of [viewport.top + 1, viewport.top + 24, viewport.top + (viewport.bottom - viewport.top) / 4]) {
      for (const x of [(viewport.left + viewport.right) / 2, viewport.left + 8]) {
        const anchor = document.elementsFromPoint(x, y)
          .map(element => element.closest('[data-vmux-scroll-anchor]'))
          .find(element => element && container.contains(element));
        if (!anchor?.id) continue;
        return { id: anchor.id, top: anchor.getBoundingClientRect().top, scrollTop: container.scrollTop };
      }
    }
    return null;
  };
  const captureScrollAnchors = (explicit) => {
    const captures = [];
    const containers = document.querySelectorAll('[data-vmux-preserve-scroll]');
    for (let containerIndex = 0; containerIndex < containers.length; containerIndex += 1) {
      const container = containers[containerIndex];
      if (!explicit && container.dataset.vmuxPreserveScroll !== 'auto') continue;
      const viewport = container.getBoundingClientRect();
      const anchor = visibleScrollAnchor(container, viewport);
      if (anchor) captures.push({ container, id: anchor.id, top: anchor.top, scrollTop: anchor.scrollTop });
    }
    return captures;
  };
  class ScrollPlacement {
    static states = new Map();
    constructor(element) {
      this.element = element;
      this.applied = false;
      this.initialTarget = null;
      this.following = false;
      this.sawUnfollowed = false;
      this.cancelled = false;
      this.content = null;
      this.measured = new Map();
      this.known = new WeakSet();
      this.measurePending = false;
      this.resizePending = false;
      this.disposed = false;
      this.observer = typeof ResizeObserver === 'function'
        ? new ResizeObserver(() => this.schedule())
        : null;
      this.observer?.observe(element);
      const cancel = () => {
        const top = element.scrollTop;
        const atBottom = element.scrollHeight - element.clientHeight - top <= 0.5;
        this.cancelled = true;
        requestAnimationFrame(() => {
          if (this.disposed || !element.isConnected) return;
          const remainedAtBottom = atBottom && element.scrollTop >= top - 0.5;
          const reachedBottom = element.scrollHeight - element.clientHeight - element.scrollTop <= 0.5;
          if (!remainedAtBottom && !reachedBottom) return;
          this.cancelled = false;
          this.schedule();
        });
      };
      for (const event of ['wheel', 'touchmove']) {
        element.addEventListener?.(event, cancel, { passive: true });
      }
      element.addEventListener?.('pointerdown', event => {
        if (event.target?.closest?.('button, a, input, textarea, select, [role="button"]')) return;
        cancel();
      }, { passive: true });
      element.addEventListener?.('keydown', event => {
        if (event.target?.closest?.('input, textarea, [contenteditable]')) return;
        if (['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End', ' '].includes(event.key)) cancel();
      });
      element.addEventListener?.('scroll', () => {
        if (element.scrollHeight - element.clientHeight - element.scrollTop <= 0.5) this.cancelled = false;
      }, { passive: true });
    }
    static apply() {
      for (const [element, state] of this.states) {
        if (element.isConnected) continue;
        state.dispose();
        this.states.delete(element);
      }
      for (const element of document.querySelectorAll('[data-vmux-initial-scroll]')) {
        if (!element.isConnected) continue;
        let state = this.states.get(element);
        if (!state) {
          state = new ScrollPlacement(element);
          this.states.set(element, state);
        }
        state.update();
      }
    }
    update() {
      if (this.disposed || !this.element.isConnected) return;
      const requested = this.element.dataset.vmuxFollowScroll === 'true';
      if (requested && !this.following) this.cancelled = false;
      if (!requested) this.sawUnfollowed = true;
      this.following = requested;
      const content = this.element.firstElementChild;
      if (content !== this.content) {
        if (this.content) this.observer?.unobserve(this.content);
        this.content = content;
        if (content) this.observer?.observe(content);
      }
      if (!this.cancelled && (!this.applied || this.following)) this.measure();
      this.place();
    }
    schedule() {
      if (this.resizePending) return;
      this.resizePending = true;
      requestAnimationFrame(() => {
        this.resizePending = false;
        this.update();
      });
    }
    dispose() {
      this.disposed = true;
      this.observer?.disconnect();
      for (const [node, visibility] of this.measured) node.style.contentVisibility = visibility;
      this.measured.clear();
      this.measurePending = false;
    }
    measure() {
      for (const node of this.element.querySelectorAll?.('[data-vmux-scroll-layout]') || []) {
        if (this.known.has(node) || this.measured.has(node)) continue;
        this.measured.set(node, node.style.contentVisibility);
        node.style.contentVisibility = 'visible';
      }
      if (this.measurePending || this.measured.size === 0) return;
      this.measurePending = true;
      const batch = [...this.measured.keys()];
      const release = () => {
        if (this.disposed) return;
        if (!this.element.isConnected) { this.dispose(); return; }
        for (const node of batch) {
          node.style.contentVisibility = this.measured.get(node) || '';
          this.known.add(node);
          this.measured.delete(node);
        }
        this.measurePending = false;
        if (this.measured.size) this.measure();
        this.place();
        if (PagePresentation.current) PagePresentation.present(PagePresentation.current);
      };
      const paint = () => {
        requestAnimationFrame(() => requestAnimationFrame(() => {
          release();
        }));
      };
      paint();
    }
    place() {
      const element = this.element;
      if (!element.isConnected || element.clientHeight <= 0 || this.cancelled) return;
      const target = element.dataset.vmuxInitialScroll;
      if (!target) return;
      const maximum = Math.max(0, element.scrollHeight - element.clientHeight);
      if (!this.applied) {
        const top = target === 'bottom' ? maximum : Number(target);
        if (!Number.isFinite(top)) return;
        element.scrollTop = Math.min(Math.max(0, top), maximum);
        this.initialTarget = target;
        this.applied = true;
      }
      if (this.following && (this.initialTarget === 'bottom' || this.sawUnfollowed)
        && Math.abs(element.scrollTop - maximum) > 0.5) {
        element.scrollTop = maximum;
      }
    }
  }
  class PagePresentation {
    static current = null;
    static presented = null;
    static applied(revision, epoch) {
      if (this.presented?.revision === revision && this.presented?.epoch === epoch) return;
      let ticket = this.current;
      if (!ticket || ticket.revision !== revision || ticket.epoch !== epoch) {
        ticket = { revision, epoch, scheduled: false };
        this.current = ticket;
      }
      this.present(ticket);
    }
    static pending() {
      for (const element of document.querySelectorAll('[data-vmux-presentation-pending]')) {
        if (element.closest?.('[aria-hidden="true"]')) continue;
        const style = getComputedStyle(element);
        if (style.display === 'none' || style.visibility === 'hidden') continue;
        if (element.getClientRects && element.getClientRects().length === 0 && style.display !== 'contents') continue;
        return true;
      }
      return false;
    }
    static present(ticket) {
      if (ticket.scheduled || this.current !== ticket) return;
      if (this.pending()) return;
      ticket.scheduled = true;
      requestAnimationFrame(() => requestAnimationFrame(() => {
        ticket.scheduled = false;
        if (this.current !== ticket || document !== documentIdentity) return;
        if (!requestMatchesDocument(ticket)) return;
        if (this.pending()) return;
        ScrollPlacement.apply();
        this.presented = ticket;
        this.current = null;
        postMessage('presented:' + ticket.revision);
      }));
    }
  }
  const restoreScrollAnchors = (captures) => {
    for (const capture of captures) {
      if (!capture.container.isConnected) continue;
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
    static items(container) {
      return container.querySelectorAll('[data-vmux-motion-item], [data-vmux-stack-row]');
    }
    static key(element) {
      return element.dataset.vmuxMotionItem ?? element.dataset.vmuxStackRow;
    }
    static capture(requests) {
      if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return [];
      const captures = [];
      for (const request of requests) {
        if (request.kind !== 'animateLayout') continue;
        const container = document.getElementById(request.element);
        const rects = new Map();
        if (!container) {
          captures.push({ request, rects });
          continue;
        }
        const bounds = container.getBoundingClientRect();
        for (const element of this.items(container)) {
          const rect = element.getBoundingClientRect();
          const visible = rect.right > bounds.left && rect.left < bounds.right && rect.bottom > bounds.top && rect.top < bounds.bottom;
          const snapshot = element.dataset.vmuxMotionItem && visible ? element.cloneNode(true) : null;
          rects.set(this.key(element), { rect, bounds, snapshot });
          this.active.get(element)?.cancel();
          this.active.delete(element);
        }
        captures.push({ request, rects });
      }
      return captures;
    }
    static retire(container, capture) {
      const { snapshot: ghost, rect, bounds } = capture;
      if (!ghost || typeof ghost.animate !== 'function') return;
      for (const node of [ghost, ...ghost.querySelectorAll('[id], [data-dioxus-id], [data-vmux-motion-item], [data-vmux-stack-row]')]) {
        for (const attribute of ['id', 'data-dioxus-id', 'data-vmux-motion-item', 'data-vmux-stack-row']) node.removeAttribute(attribute);
      }
      ghost.inert = true;
      ghost.setAttribute('aria-hidden', 'true');
      Object.assign(ghost.style, {
        position: 'fixed', left: rect.left + 'px', top: rect.top + 'px', right: 'auto', bottom: 'auto',
        width: rect.width + 'px', height: rect.height + 'px', margin: '0', pointerEvents: 'none', zIndex: '30',
        transformOrigin: 'center', transition: 'none',
        clipPath: `inset(${Math.max(0, bounds.top - rect.top)}px ${Math.max(0, rect.right - bounds.right)}px ${Math.max(0, rect.bottom - bounds.bottom)}px ${Math.max(0, bounds.left - rect.left)}px)`,
      });
      container.appendChild(ghost);
      const animation = ghost.animate([{ opacity: 1, transform: 'scale(1)' }, { opacity: 0, transform: 'scale(0.96)' }], { duration: __VMUX_MOTION_FEEDBACK_MS__, easing: 'ease-out' });
      animation.finished.finally(() => ghost.remove()).catch(() => {});
    }
    static apply(captures) {
      for (const { request, rects } of captures) {
        if (!requestMatchesDocument(request)) continue;
        const container = document.getElementById(request.element);
        if (!container) {
          if (document.body) {
            for (const capture of rects.values()) this.retire(document.body, capture);
          }
          continue;
        }
        const items = [...this.items(container)];
        const present = new Set(items.map(element => this.key(element)));
        let retired = false;
        for (const [key, capture] of rects) {
          if (present.has(key) || !capture.snapshot) continue;
          this.retire(container, capture);
          retired = true;
        }
        const duration = Math.min(__VMUX_MOTION_MAX_MS__, request.duration_ms);
        const delay = retired ? Math.min(__VMUX_MOTION_FEEDBACK_MS__ / 2, __VMUX_MOTION_MAX_MS__ - duration) : 0;
        for (const element of items) {
          const previous = rects.get(this.key(element))?.rect;
          if (typeof element.animate !== 'function') continue;
          if (!previous) {
            const animation = element.animate([
              { opacity: 0, transform: 'translateY(4px) scale(0.96)' },
              { opacity: 1, transform: 'translateY(0px) scale(1)' },
            ], { duration, fill: 'backwards', easing: 'cubic-bezier(0.2, 0, 0, 1)' });
            this.active.set(element, animation);
            animation.finished.finally(() => { if (this.active.get(element) === animation) this.active.delete(element); }).catch(() => {});
            continue;
          }
          const next = element.getBoundingClientRect();
          const dx = previous.left - next.left;
          const dy = previous.top - next.top;
          if (Math.abs(dx) + Math.abs(dy) < 0.5) continue;
          const animation = element.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'translate(0px, 0px)' }], { duration, delay, fill: 'backwards', easing: 'cubic-bezier(0.2, 0, 0, 1)' });
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
  const initialResponse = (frame) => {
    const headers = {
      'x-vmux-navigation': frame.revision,
      'x-vmux-frame': frame.sequence,
      'x-vmux-frame-epoch': frame.epoch,
      'x-vmux-frame-capability': frame.capability,
    };
    return {
      status: 200,
      ok: true,
      headers: { get: name => headers[name] ?? null },
      arrayBuffer: async () => fromBase64(frame.body),
    };
  };
  const pumpEdits = async (initialFrame) => {
    let initial = initialFrame ? initialResponse(initialFrame) : null;
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
        const response = initial ?? await fetch('/__edits', { headers });
        initial = null;
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
            const pendingFrame = response.headers.get('x-vmux-frame');
            const retainedLayout = revision === navigationRevision
              && appliedFrame !== null && appliedFrame === pendingFrame;
            navigationRevision = revision;
            globalThis.__vmuxIpcEpoch = Number(responseEpoch);
            abandonApplyingRevision();
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
            if (retainedLayout) PagePresentation.applied(revision, responseEpoch);
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
        const scrollAnchors = remount || !edits.byteLength
          ? []
          : captureScrollAnchors(preserveScroll);
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
        restoreScrollAnchors(scrollAnchors);
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
          ScrollPlacement.apply();
          if (commitNavigationRevision(revision)) reportMetadata();
          PagePresentation.applied(revision, String(globalThis.__vmuxIpcEpoch));
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
        ScrollPlacement.apply();
        if (!remount) navigated = commitNavigationRevision(revision);
        if (navigated) reportMetadata();
        PagePresentation.applied(revision, responseEpoch);
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
      const initialFrame = globalThis.__vmuxInitialFrame;
      delete globalThis.__vmuxInitialFrame;
      if (initialFrame) {
        globalThis.__vmuxIpcEpoch = Number(initialFrame.epoch);
        if (document.body) document.body.inert = !initialFrame.interactive;
      }
      ScrollEvents.install(window.interpreter);
      holdLinks();
      reportInteractions();
      watchMetadata();
      pumpEdits(initialFrame);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    struct RuntimeProbe;

    impl RuntimeProbe {
        fn verify(script: &str) {
            let mut child = Command::new("node")
                .args(["-e", script])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Node is required for the page runtime test");
            child
                .stdin
                .take()
                .unwrap()
                .write_all(Shim::source().as_bytes())
                .unwrap();
            let result = child.wait_with_output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }

    #[test]
    fn closing_a_middle_tab_fades_that_tab_and_moves_its_successor_without_delaying_layout() {
        RuntimeProbe::verify(include_str!("layout_movement_test.js"));
    }

    #[test]
    fn scroll_notifications_do_not_wait_for_native_replies_and_coalesce_per_frame() {
        RuntimeProbe::verify(include_str!("scroll_events_test.js"));
    }

    #[test]
    fn content_growth_and_scroll_clamping_preserve_position_before_the_next_input() {
        RuntimeProbe::verify(include_str!("scroll_retention_test.js"));
    }

    #[test]
    fn first_layout_presents_without_waiting_for_background_data_or_fonts() {
        RuntimeProbe::verify(include_str!("presentation_test.js"));
    }

    #[test]
    fn composer_handoff_preserves_final_layout_and_does_not_delay_navigation() {
        RuntimeProbe::verify(include_str!("composer_handoff_test.js"));
    }

    #[test]
    fn composer_handoff_uses_native_browser_layout_and_animation() {
        RuntimeProbe::verify(include_str!("composer_handoff_browser_test.js"));
    }
}
