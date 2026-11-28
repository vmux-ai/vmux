(function(id, css) {
  var selector = 'style[data-vmux-hot-reload="' + id + '"]';
  var node = document.head.querySelector(selector);
  if (!node) {
    node = document.createElement('style');
    node.setAttribute('data-vmux-hot-reload', id);
    document.head.appendChild(node);
  }
  node.textContent = css;
})(__VMUX_HOT_RELOAD_ID__, __VMUX_HOT_RELOAD_CSS__);
