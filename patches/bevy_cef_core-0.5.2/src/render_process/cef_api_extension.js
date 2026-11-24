
var cef;
if (!cef) cef = {};
(function() {
  native function __cef_emit();
  native function __cef_listen();
  native function __cef_bin_emit();
  native function __cef_history_swipe();
  cef.emit = __cef_emit;
  cef.listen = __cef_listen;
  cef.binEmit = __cef_bin_emit;
  cef.binListen = __cef_listen;
  cef.historySwipe = __cef_history_swipe;
})();
