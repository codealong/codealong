// A Claude Code session whose turn is over, waiting on a command you ran for it with Run in
// Codealong's own terminal, open in Sessions: for a screenshot of its row's clock and the end of
// its conversation saying the command and where what it prints goes (Working.tsx `Waiting`,
// terminal.rs `overlay_yours`).
//   PREVIEW_INIT=scripts/preview/your-command-init.js PREVIEW_NAME=your-command scripts/preview.sh sessions
//   PREVIEW_SCHEME=dark PREVIEW_INIT=scripts/preview/your-command-init.js PREVIEW_NAME=your-command scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var command = 'node scripts/publish.mjs --target staging --check-links --report build/publish-report.json';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if ((picked && s.key !== picked.key) || l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude' || !s.live) return;
          var since = Date.now() - 2 * 60e3;
          var excerpt = 'Waiting on your command · `' + command.slice(0, 79) + '…`';
          Object.assign(s, {
            status: 'working',
            statusSince: since,
            stalledSince: undefined,
            background: undefined,
            agents: undefined,
            hosted: false,
            asks: false,
            steerable: false,
            stoppable: false,
            suggestions: undefined,
            snoozedUntil: undefined,
            finished: undefined,
            insight: undefined,
            running: { terminal: 'Codealong', command: command, yours: true },
            excerpt: excerpt,
            detail: '{for:' + since + '} · ' + excerpt,
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
