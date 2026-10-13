// Checks the panel for a command an agent asks to run in your terminal (terminal.rs,
// `Pending.terminal`): a session waits on it under a heading that says so, the command shows as
// code with no line above it repeating it, the note says which terminal and folder it runs in and
// as whom, there's no Always (it's asked every time), and Allow answers `allow` for the run's id.
//   PREVIEW_WAIT=30000 scripts/preview.sh sessions '' scripts/preview/terminal-run-check.js
// As PREVIEW_INIT, after a line setting `window.__terminal_screenshot = true`, it leaves the
// request waiting for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var command = 'gcloud auth login --update-adc';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ id: 'run-mgabc123', label: 'Run in your terminal · `' + command + '`', input: command, cannot: null, terminal: { cwd: '~/projects/app', app: 'Ghostty' } });
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve('Opened a Ghostty tab and ran it there. What it prints goes back to its agent.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked || l.hidden || l.device || s.visitor || s.archived) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.hosted = false;
          s.excerpt = 'Run in your terminal · `' + command + '`';
          picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var approval = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-approval'); };
  var button = function (text) {
    // A button's text is its label, then any keys it says (Deny⌘⌫).
    var a = approval();
    return a && Array.prototype.filter.call(a.querySelectorAll('button'), function (b) { return b.textContent.trim().indexOf(text) === 0; })[0];
  };
  var answered = function () { return window.__calls.filter(function (c) { return c[0] === 'session_answer'; }); };

  say('terminal run check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return button('Allow'); }, 5000);
    })
    .then(function () {
      var a = approval();
      var head = a.querySelector('.ca-approval-head').textContent;
      check('says it wants your terminal', head.indexOf('Wants to run in your terminal') >= 0, head);
      var input = a.querySelector('.ca-approval-input');
      check('the command shows as code', !!input && input.textContent.indexOf(command) >= 0, input && input.textContent);
      check('no line above it saying it again', !a.querySelector('.ca-approval-label'));
      check('allow and deny, no always', !!button('Allow') && !!button('Deny') && !button('Always'));
      var note = a.querySelector('.ca-composer-note').textContent;
      check('says where and as whom', note.indexOf('new Ghostty tab in ~/projects/app') >= 0 && note.indexOf('as you') >= 0 && note.indexOf('outside any sandbox') >= 0, note);
      if (window.__terminal_screenshot) return;
      button('Allow').click();
      return until(function () { return answered().length > 0; }, 3000).then(function () {
        var c = answered()[0][1];
        check('allow answers allow for the run', c.answer === 'allow' && c.id === 'run-mgabc123' && c.key === picked.key, JSON.stringify(c));
      });
    })
    .then(done)
    .catch(stopped);
})();
