// Checks the model a running session is on, beside Send in its box (SessionModel.tsx,
// ModelMenu.tsx): it says the model its latest reply came from; where Codealong types into its
// terminal, its account's models are a menu, without the account's own (`/model` takes a name),
// and picking one asks the engine to switch that session; the box stays the height it was; and
// a session in the middle of a turn can't be switched until it ends. After the model, quieter,
// is the effort it's on as its harness writes it, which follows the session, and the longest of
// both isn't cut off; the mode under the box is lowercase.
//   scripts/preview.sh peek '' scripts/preview/session-model-check.js
(function () {
  // This runs before the app's script: pick a session on this Mac that's running in a terminal,
  // between turns, whose account switches models.
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
      // The preview's stand-in says a running terminal session is typed into (fake-tauri.js).
      if (!picked && s.live && s.model && !s.hosted && !s.attachCommand && s.surface === 'cli' && s.status !== 'working' && s.status !== 'needs' && !s.stalledSince && !s.archived && !l.hidden && !l.device && a && a.switchesModel && (a.models || []).length > 1) picked = s;
    });
  });
  if (!picked) { stopped('no running terminal session between turns on an account that switches models'); return; }
  var acct = account(picked);
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);

  var chip = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'); };
  var effort = function () { var d = q('.ca-modelmenu-detail', chip()); return d ? d.textContent : ''; };
  var withSession = function (change) {
    var b = JSON.parse(JSON.stringify(board));
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) change(s); }); });
    return b;
  };
  var field = function () { return q('.ca-peek .ca-composer-field'); };
  var note = function () { var n = q('.ca-peek .ca-composer-note'); return n ? n.textContent : ''; };
  var other = acct.models.filter(function (m) { return m.id.indexOf('haiku') === 0; })[0] || acct.models[acct.models.length - 1];
  var name = function (m) { return m.label.split(':')[0].replace(/ \(1M context\)$/, ' 1M'); };

  say('session model check: ' + picked.key + ' on ' + picked.model);
  until(function () { return !!chip(); }, 20000, 'no model chip beside Send')
    .then(function () {
      check('the chip says the model it’s on', chip().textContent.trim().length > 0 && chip().textContent.indexOf('Model') < 0, chip().textContent);
      check('the box has no place to go on in beside it', !q('.ca-placemenu', field()), 'a place chip');
      check('the chip sits beside Send', !!q('.ca-composer-send', chip().closest('.ca-composer-buttons')), 'apart');
      if (picked.effort) check('after the model, the effort its latest reply was made at, as its harness writes it', effort() === picked.effort, effort() || 'none');
      var mode = q('.ca-peek .ca-composer-under .ca-mode-name');
      if (mode) check('under the box the mode is said lowercase', mode.innerText === mode.innerText.toLowerCase(), mode.innerText);
      // `/effort` set another, on the longest name a model has.
      window.__emit('board', withSession(function (s) { s.effort = 'xhigh'; s.model = 'claude-fable-5-1[1m]'; }));
      return until(function () { return effort() === 'xhigh'; }, 3000, 'the effort didn’t follow the session: ' + effort());
    })
    .then(function () {
      var cut = all('span', chip()).filter(function (s) { return s.scrollWidth > s.clientWidth + 1; });
      check('the longest model and effort aren’t cut off', cut.length === 0, cut.map(function (s) { return s.textContent; }).join(', '));
      var detail = getComputedStyle(q('.ca-modelmenu-detail', chip())).color;
      check('the effort is quieter than the model', detail !== getComputedStyle(chip()).color, detail);
      window.__emit('board', withSession(function (s) { delete s.effort; }));
      return until(function () { return !q('.ca-modelmenu-detail', chip()); }, 3000, 'an effort stayed after the session stopped saying one');
    })
    .then(function () {
      window.__emit('board', board);
      chip().click();
      return until(function () { return !!q('.ca-peek .ca-modelmenu .ca-picker'); }, 3000, 'no model menu');
    })
    .then(function () {
      var menu = q('.ca-peek .ca-modelmenu .ca-picker');
      var titles = all('.ca-picker-title', menu).map(function (t) { return t.textContent.trim(); });
      check('it lists the account’s models by name, without its own', titles.length === acct.models.length && titles.every(function (t) { return t.indexOf('Default') !== 0; }), titles.join(', '));
      check('none is checked: a reply doesn’t say which of the names it’s on', !q('.ca-picker-check .ca-icon', menu), 'checked');
      var rect = menu.getBoundingClientRect();
      check('the menu opens upward, inside the window', rect.top >= 0 && rect.bottom <= field().getBoundingClientRect().top + 1, rect.top + '–' + rect.bottom);
      var opt = all('.ca-picker-opt', menu).filter(function (b) { return b.querySelector('.ca-picker-title').textContent.trim() === name(other); })[0];
      opt.click();
      return until(function () { return calls('switch_session_model').length === 1; }, 5000, 'no switch asked for');
    })
    .then(function () { return wait(50); })
    .then(function () {
      var asked = sent('switch_session_model')[0];
      check('picking one switches that session to it', asked.key === picked.key && asked.model === other.id, JSON.stringify(asked));
      check('the note says what happened', note().indexOf('Switched') === 0, note());
      check('nothing was sent as a prompt', calls('send_to_session').length === 0, String(calls('send_to_session').length));
      // Its turn starts: it can't be switched until it ends.
      window.__emit('board', withSession(function (s) { s.status = 'working'; }));
      return until(function () { return chip() && chip().disabled; }, 3000, 'a working session’s chip stays on');
    })
    .then(function () {
      check('mid-turn the chip waits for the turn to end', chip().disabled && /once its turn ends/.test(chip().title), chip().title);
    })
    .then(done, stopped);
})();
