// For a screenshot of a session's box holding a command led by `!`, its foot saying what ⏎ and ⌥⏎
// do with it (shell-bang-check.js checks them): the first Claude Code session, made a terminal
// one whose turn has ended, opened in the Sessions view.
//   PREVIEW_INIT=scripts/preview/shell-bang-init.js PREVIEW_NAME=shell-bang scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    // Answered at once, as the stub's own answer can come after the screenshot.
    if (cmd === 'session_shell_way') return Promise.resolve({ kind: 'typed', app: 'Ghostty' });
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      window.__madeSessions = window.__madeSessions || {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!(picked ? s.key === picked.key : !l.hidden && !l.device && !s.visitor && !s.archived && s.harness === 'claude')) return;
          s.status = 'idle';
          s.live = true;
          s.hosted = false;
          s.asks = false;
          s.surface = 'cli';
          s.attachCommand = undefined;
          s.suggestions = undefined;
          window.__madeSessions[s.key] = s;
          if (!picked) picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer:not(.is-approval) textarea'); };
  var tries = 0;
  var go = function () {
    var row = picked && document.getElementById('row-' + picked.lane + '::' + picked.key);
    if (row && !box()) {
      row.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row.click();
    }
    if (box()) {
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box(), '! npm run build');
      box().dispatchEvent(new Event('input', { bubbles: true }));
      return;
    }
    if (++tries < 100) setTimeout(go, 100);
  };
  setTimeout(go, 300);
})();
