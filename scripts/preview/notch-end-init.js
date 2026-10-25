// Opens the bar docked in the notch on the end of what waits, as the shortcut does with nothing
// waiting, for a screenshot of it (preview.sh's notch view): the bar keeps only what's running.
// The shape takes each state at once: a screenshot's time can run out while it springs open.
//   PREVIEW_SIZE=1512,420 PREVIEW_INIT=scripts/preview/notch-end-init.js scripts/preview.sh notch
(function () {
  var style = document.createElement('style');
  style.textContent = '.ca-notch * { transition: none !important; }';
  document.head.appendChild(style);
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './bar-feed.json', false);
  xhr.send();
  var f = JSON.parse(xhr.responseText);
  var step = 0;
  var t = setInterval(function () {
    if (!document.querySelector('.ca-notch-shape') || !window.__emit) return;
    if (step === 0) {
      step = 1;
      window.__emit('bar-feed', Object.assign({}, f, { loading: false, items: f.items.filter(function (i) { return i.group === 'working'; }), reviews: [] }));
      return;
    }
    clearInterval(t);
    window.__emit('waiting-shortcut', null);
  }, 40);
})();
