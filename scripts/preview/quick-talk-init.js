// Shows the quick panel hearing what's said with the key held to talk (Quick.tsx, talk.ts), for
// a screenshot: its microphone listening, and the words so far in its box, selected.
//   PREVIEW_VOICE=1 PREVIEW_NAME=quick-talk PREVIEW_INIT=scripts/preview/quick-talk-init.js scripts/preview.sh quick
(function () {
  window.__PREVIEW_VOICE__ = true;
  var t = setInterval(function () {
    var input = document.querySelector('.ca-quick-input textarea');
    if (!window.__quickOpen || !window.__emit || !input) return;
    clearInterval(t);
    window.__TAURI_INTERNALS__.invoke('get_board').then(function (board) {
      window.__quickOpen({ talk: true }, board);
      setTimeout(function () {
        window.__emit('talk', { id: 1, phase: 'listening', to: 'looking', text: 'Fix the login redirect on the settings', level: 0.35 });
      }, 200);
    });
  }, 60);
})();
