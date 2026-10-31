// A session in the peek whose running turn can be stopped, after one Escape in its box, for a
// screenshot of the box saying in amber that another stops it (SessionReply.tsx).
//   PREVIEW_INIT=scripts/preview/stop-again-init.js PREVIEW_NAME=stop-again scripts/preview.sh peek
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/stop-again-init.js PREVIEW_NAME=stop-again scripts/preview.sh peek
(function () {
  var xhr = new XMLHttpRequest();
  xhr.open('GET', './board.json', false);
  xhr.send();
  var board = JSON.parse(xhr.responseText);
  var picked = null;
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (!picked && !s.archived && !l.hidden) picked = s; }); });
  if (!picked) return;
  var key = picked.key;
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.draft.' + key);
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var got = real.apply(this, arguments);
    if (cmd !== 'get_board') return got;
    return got.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.key === key) Object.assign(s, { status: 'working', live: true, stoppable: true, stalledSince: null, asks: false });
        });
      });
      return b;
    });
  };
  var tries = 0;
  var escape = function () {
    var box = document.querySelector('.ca-composer textarea');
    if (!box || !document.querySelector('.ca-composer-send.is-stop')) {
      if (tries++ < 5) setTimeout(escape, 100);
      return;
    }
    box.focus();
    box.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
  };
  // The screenshot is taken 6 s in (preview.sh), and the box says it for 2 s after.
  setTimeout(escape, 5000);
})();
