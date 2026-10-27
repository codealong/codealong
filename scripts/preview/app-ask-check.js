// Checks the panel for an approval the Claude app asks for itself (claude/applog.rs: its built-in
// browser's sites), which only the app can answer: its row says it needs approval, and the panel
// says what it asks, why it's answered in the app, and offers only a way there.
//   scripts/preview.sh sessions '' scripts/preview/app-ask-check.js
// As PREVIEW_INIT, after a line setting `window.__app_ask_screenshot = true`, it leaves the
// approval up for a screenshot.
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  var picked = null;
  var label = 'browser · read_console_messages · `app.northwind.dev`';
  var cannot = 'The Claude app asks this itself, not Claude Code, so only the app can answer it.';
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked.key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ id: 'app:a1b2-01', label: label, input: null, cannot: cannot });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked || l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude') return;
          // As the engine has it (board.rs `claude_session`): waiting on you, not stuck.
          s.status = 'needs';
          s.surface = 'app';
          s.statusSince = Date.now() - 60e3;
          s.asks = false;
          s.stalledSince = null;
          s.excerpt = label;
          picked = { key: s.key, lane: l.id };
        });
      });
      return board;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var approval = function () { return document.querySelector('.ca-sessinsp .ca-composer.is-approval'); };
  var button = function (text) {
    var a = approval();
    return a && Array.prototype.filter.call(a.querySelectorAll('button'), function (b) { return b.textContent.trim().indexOf(text) === 0; })[0];
  };

  say('app ask check: waiting for the list');
  until(function () { return row(); }, 20000)
    .then(function () {
      var flag = row().querySelector('.ca-insight');
      check('its row says it needs approval', !!flag && flag.textContent.trim() === 'Needs approval', flag && flag.textContent);
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return button('Open in'); }, 5000);
    })
    .then(function () {
      var a = approval();
      check('shows as answered elsewhere', a.classList.contains('is-elsewhere'));
      var head = a.querySelector('.ca-approval-head').textContent;
      check('says it wants to run something', head.indexOf('Wants to run') >= 0, head);
      var shown = a.querySelector('.ca-approval-label');
      check('says what the app asks', !!shown && shown.textContent.indexOf('read_console_messages') >= 0 && shown.textContent.indexOf('app.northwind.dev') >= 0, shown && shown.textContent);
      check('no way to answer it here', !button('Allow') && !button('Deny') && !button('Always'), a.textContent);
      check('only a way to the app', button('Open in').textContent.indexOf('Claude app') >= 0, button('Open in').textContent);
      var note = a.querySelector('.ca-composer-note').textContent;
      check('saying why, and where to answer it', note.indexOf('only the app can answer it') >= 0 && note.indexOf('Answer it in Claude app') >= 0, note);
      if (window.__app_ask_screenshot) return;
      return done();
    })
    .catch(stopped);
})();
