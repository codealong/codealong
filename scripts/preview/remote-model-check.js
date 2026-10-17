// Checks the box of a session on another of your Macs (SessionReply.tsx, SessionModel.tsx,
// startAccounts.ts `useDeviceStartAccounts`): a Codex app thread there says its model as a menu of
// that Mac's account's models, asked of that Mac (`device_start_options`), and picking one asks
// the engine to switch the thread there, by its key from here; one that Mac runs through Codealong
// switches its mode from here too.
//   scripts/preview.sh peek '' scripts/preview/remote-model-check.js
(function () {
  // This runs before the app's script: pick a Codex app thread on this Mac that says its model,
  // and show a copy of it as if it were on another Mac, as that Mac's feed would.
  var read = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var board = read('./board.json');
  var accounts = read('./start-options.json').accounts;
  var picked = null;
  var lane = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (!picked && s.harness === 'codex' && s.surface === 'app' && s.model && !s.hosted && s.status !== 'needs' && !s.archived && !l.hidden && !l.device) {
        picked = s;
        lane = l;
      }
    });
  });
  if (!picked) { stopped('no Codex app thread that says its model'); return; }
  var acct = accounts.filter(function (a) { return a.name === picked.account && a.harness === picked.harness; })[0];
  if (!acct || !acct.switchesInApp) { stopped('the thread’s account doesn’t say the app switches its sessions (start-options.json)'); return; }

  var SHORT = 'b7c8d9e0f1';
  var key = '@' + SHORT + ':' + picked.key;
  // The board with the copy in a lane of that Mac's, as fake-tauri.js's `devices` makes them.
  var onMini = function (change) {
    var l = JSON.parse(JSON.stringify(lane));
    l.id = '@' + SHORT + ':' + lane.id;
    l.device = { name: 'Mac mini', short: SHORT, color: 'gray', online: true, lastSeen: null };
    l.targets = [];
    l.defaultTarget = null;
    var s = JSON.parse(JSON.stringify(picked));
    s.key = key;
    s.openTarget = '';
    s.pid = null;
    s.resumeCommand = null;
    s.live = false;
    s.status = 'idle';
    if (change) change(s);
    l.sessions = [s];
    window.__madeSessions = window.__madeSessions || {};
    window.__madeSessions[key] = s;
    var b = JSON.parse(JSON.stringify(board));
    b.lanes.push(l);
    return b;
  };
  var first = onMini();
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd !== 'get_board') return real.apply(this, arguments);
    window.__calls.push([cmd, args, options]);
    return Promise.resolve(JSON.parse(JSON.stringify(first)));
  };
  localStorage.setItem('codealong.preview.peek', key);
  localStorage.removeItem('codealong.draft.' + key);

  var chip = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'); };
  var modeChip = function () { return q('.ca-peek .ca-composer-who .ca-modemenu-chip'); };

  say('remote model check: ' + key + ' on ' + picked.model);
  until(function () { return !!chip(); }, 20000, 'no model menu beside Send for a thread on another Mac')
    .then(function () {
      var asked = sent('device_start_options');
      check('its models are asked of the Mac it’s on', asked.length > 0 && asked.every(function (a) { return a.device === SHORT; }), JSON.stringify(asked));
      check('it switches the thread from its next turn', /from its next turn/.test(chip().title) && !chip().disabled, chip().title);
      chip().click();
      return until(function () { return !!q('.ca-peek .ca-modelmenu .ca-picker'); }, 3000, 'no model menu');
    })
    .then(function () {
      var titles = all('.ca-peek .ca-modelmenu .ca-picker-title').map(function (t) { return t.textContent.trim(); });
      check('it lists that Mac’s account’s models', titles.length === acct.models.length, titles.join(', '));
      all('.ca-peek .ca-modelmenu .ca-picker-opt').pop().click();
      return until(function () { return calls('switch_session_model').length === 1; }, 5000, 'no switch asked for');
    })
    .then(function () {
      var asked = sent('switch_session_model')[0];
      check('picking one switches the thread on that Mac, by its key from here', asked.key === key && acct.models.some(function (m) { return m.id === asked.model; }), JSON.stringify(asked));
      // The same session run there through Codealong, which offers its modes.
      window.__emit('board', onMini(function (s) {
        s.hosted = true;
        s.live = true;
        s.modes = [
          { id: 'default', kind: 'asks', label: 'Default', detail: 'Asks before it edits files outside its workspace.' },
          { id: 'full-access', kind: 'skips', label: 'Full Access', detail: 'Edits files anywhere and uses the internet without asking.' },
        ];
        s.mode = s.modes[0];
      }));
      return until(function () { return !!modeChip(); }, 3000, 'no mode menu for a session another Mac runs through Codealong');
    })
    .then(function () {
      modeChip().click();
      return until(function () { return !!q('.ca-peek .ca-modemenu .ca-picker'); }, 3000, 'no mode menu');
    })
    .then(function () {
      all('.ca-peek .ca-modemenu .ca-picker-opt').filter(function (o) { return q('.ca-picker-title', o).textContent.trim() === 'Full Access'; })[0].click();
      return until(function () { return calls('switch_session_mode').length === 1; }, 5000, 'no mode switch asked for');
    })
    .then(function () {
      var asked = sent('switch_session_mode')[0];
      check('picking a mode switches it on that Mac', asked.key === key && asked.mode === 'full-access', JSON.stringify(asked));
    })
    .then(done, stopped);
})();
