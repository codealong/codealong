// A Claude Code session whose turn ended with three agents it started still running in the
// background, open in Sessions, for a screenshot of its row and the end of its conversation
// waiting on them with a clock rather than a turn's spinner (Working.tsx `Waiting`).
//   PREVIEW_INIT=scripts/preview/agents-waiting-init.js PREVIEW_NAME=agents-waiting scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/agents-waiting-init.js PREVIEW_NAME=agents-waiting scripts/preview.sh sessions
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
          var since = Date.now() - 4 * 60e3;
          Object.assign(s, {
            status: 'working',
            statusSince: since,
            stalledSince: undefined,
            background: undefined,
            running: undefined,
            hosted: false,
            asks: false,
            steerable: false,
            stoppable: false,
            suggestions: undefined,
            snoozedUntil: undefined,
            finished: undefined,
            agents: ['Draft the release notes', 'Check the migration on a copy of the database', 'Review the settings screen'],
            excerpt: 'Waiting on 3 background agents',
            detail: '{for:' + since + '} · Waiting on 3 background agents',
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
