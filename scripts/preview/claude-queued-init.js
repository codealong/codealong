// A Claude Code session whose running turn holds two messages, open in Sessions, for a screenshot
// of them queued at the end of its conversation with Send now (SessionThread.tsx).
//   PREVIEW_INIT=scripts/preview/claude-queued-init.js PREVIEW_NAME=claude-queued scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/claude-queued-init.js PREVIEW_NAME=claude-queued scripts/preview.sh sessions
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
          if ((picked && s.key !== picked.key) || l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude' || !s.live) return;
          var at = Date.now();
          Object.assign(s, {
            status: 'working',
            statusSince: at - 4 * 60e3,
            hosted: false,
            asks: false,
            steerable: true,
            suggestions: undefined,
            snoozedUntil: undefined,
            queuedAt: at - 5e3,
            queued: [
              { id: 'q1', at: at - 20e3, text: 'There is also the main app in ../atlas-web' },
              { id: 'q2', at: at - 5e3, text: 'Keep the header as it is' },
            ],
          });
          picked = picked || { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };
  var tries = 0;
  var open = function () {
    var row = picked && document.getElementById('row-' + picked.lane + '::' + picked.key);
    if (row) row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    else if (tries++ < 50) setTimeout(open, 100);
  };
  setTimeout(open, 300);
})();
