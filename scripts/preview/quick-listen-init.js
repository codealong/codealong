// Opens the quick panel set to listen (Quick.tsx, Listen now), for a screenshot of it: as
// `[shortcuts] listen` opens it, with the stub's calendar having a meeting going on, and with a
// name typed for it.
//   PREVIEW_NAME=quick-listen PREVIEW_INIT=scripts/preview/quick-listen-init.js scripts/preview.sh quick
(function () {
  window.__PREVIEW_VOICE__ = true;
  var t = setInterval(function () {
    var input = document.querySelector('.ca-quick-input textarea');
    if (!window.__quickOpen || !input) return;
    clearInterval(t);
    window.__TAURI_INTERNALS__.invoke('get_board').then(function (board) {
      window.__quickOpen({ listen: true }, Object.assign({}, board, { listenShortcut: 'Option+Cmd+=' }));
    });
  }, 60);
})();
