// Checks the chip beside Send for a Claude Code background job (SessionReply.tsx, SessionAgent.tsx):
// Codealong hands a job `/model` and `/effort` through its account's background service as it
// hands it a prompt (send.rs `Route::as_typed`), so between turns the chip's menu lists the
// account's models and its levels, and a pick asks the engine to switch that job, sending no
// prompt; mid-turn too, since Claude Code runs the command at once, for the turn's next step.
//   scripts/preview.sh peek '' scripts/preview/background-model-check.js
(function () {
  // This runs before the app's script: pick a Claude Code session of this Mac's to make a job of.
  var read = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var board = read('./board.json');
  var accounts = read('./start-options.json').accounts;
  var account = function (s) { return accounts.filter(function (a) { return a.name === s.account && a.harness === s.harness; })[0]; };
  var picked = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      var a = account(s);
      if (!picked && s.harness === 'claude' && s.surface === 'cli' && !s.hosted && !s.archived && !l.hidden && !l.device && a && a.switchesModel && (a.models || []).length > 0) picked = s;
    });
  });
  if (!picked) { stopped('no Claude Code session whose account switches models (start-options.json)'); return; }
  var acct = account(picked);
  if (!acct.switchesMidTurn) { stopped('the job’s account doesn’t say it switches mid-turn (start-options.json)'); return; }
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);

  var chip = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'); };
  var menu = function () { return q('.ca-peek .ca-modelmenu .ca-picker'); };
  var own = function () { return all('.ca-picker-section', menu()).filter(function (x) { var n = q('.ca-quick-acct-name', x); return n && n.textContent === picked.account; })[0]; };
  var models = function () { return all('.ca-picker-opt', own()); };
  var placeholder = function () { var t = q('.ca-peek .ca-composer textarea'); return t ? t.getAttribute('aria-placeholder') || '' : ''; };
  // The board the box gets, and the session the stub says the way for (fake-tauri.js answers
  // `session_way` from `__madeSessions`: a live session with an attach command is a job's).
  var become = function (change) {
    var b = JSON.parse(JSON.stringify(board));
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) change(s); }); });
    window.__madeSessions = window.__madeSessions || {};
    window.__madeSessions[picked.key] = JSON.parse(JSON.stringify(picked));
    change(window.__madeSessions[picked.key]);
    window.__emit('board', b);
  };
  var job = function (status) {
    return function (s) { s.live = true; s.status = status; s.asks = false; s.attachCommand = s.account + ' attach 6e07c149'; s.pid = s.pid || 4242; s.model = 'claude-sonnet-5-5'; s.effort = 'medium'; };
  };

  say('background model check: ' + picked.key + ' as a background job');
  become(job('idle'));
  until(function () { return !!chip() && /background job/.test(placeholder()); }, 5000, 'no model menu beside Send for a background job between turns: ' + placeholder())
    .then(function () {
      check('it’s a menu that switches that job alone', /for this session alone/.test(chip().title) && !chip().disabled, chip().title);
      chip().click();
      return until(function () { return !!menu() && !!own(); }, 3000, 'no section for its own account in the menu');
    })
    .then(function () {
      var titles = all('.ca-picker-title', own()).map(function (t) { return t.textContent.trim(); });
      check('it lists the account’s models', titles.length > 0 && models().every(function (o) { return !o.disabled; }), titles.join(', '));
      check('its effort levels are under them', !!q('.ca-peek .ca-modelmenu .ca-picker-foot .ca-segctl'), 'no effort row');
      models().pop().click();
      return until(function () { return calls('switch_session_model').length === 1; }, 5000, 'no switch asked for');
    })
    .then(function () { return wait(50); })
    .then(function () {
      var asked = sent('switch_session_model')[0];
      check('picking one switches that job', asked.key === picked.key && acct.models.some(function (m) { return m.id === asked.model; }), JSON.stringify(asked));
      check('nothing was sent as a prompt', calls('send_to_session').length === 0, String(calls('send_to_session').length));
      // Its turn runs: Claude Code runs the command typed at once (`switchesMidTurn`), for the
      // turn's next step, so the models still switch it.
      become(job('working'));
      return wait(200);
    })
    .then(function () {
      check('mid-turn the chip still switches that job alone', !!chip() && /for this session alone/.test(chip().title) && !chip().disabled, chip() ? chip().title : 'no chip');
      chip().click();
      return until(function () { return !!menu() && !!own(); }, 3000, 'no menu mid-turn');
    })
    .then(function () {
      check('mid-turn its models still switch it', models().length > 0 && models().every(function (o) { return !o.disabled; }), models().map(function (o) { return o.disabled ? 'off' : 'on'; }).join());
      check('and so do its levels', !!q('.ca-peek .ca-modelmenu .ca-picker-foot .ca-segctl'), 'no effort row');
    })
    .then(done, stopped);
})();
