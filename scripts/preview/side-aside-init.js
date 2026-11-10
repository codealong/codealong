// Sessions with one set aside for its side conversation (board.rs `apply_aside`), selected, for a
// screenshot of its row under Working and its header:
//   PREVIEW_INIT=scripts/preview/side-aside-init.js PREVIEW_NAME=side-aside scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !l.device && !s.visitor && !s.archived && s.harness === 'claude') picked = { key: s.key, lane: l.id };
          if (!picked || s.key !== picked.key) return;
          var now = Date.now();
          Object.assign(s, { status: 'needs', need: 'question', statusSince: now - 20 * 60e3, aside: now - 4 * 60e3, excerpt: 'Asking on the side · Are we restoring for real, or only testing the restore?', asks: false, snoozedUntil: undefined, raised: undefined });
        });
      });
      return board;
    });
  };
  var tries = 0;
  var pick = function () {
    var row = picked && document.getElementById('row-' + picked.lane + '::' + picked.key);
    if (row) row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    else if (tries++ < 100) setTimeout(pick, 100);
  };
  pick();
})();
