// Checks the model a running session is on, in the chip beside Send in its box (SessionAgent.tsx,
// ModelMenu.tsx): after its account, quieter, it says the model its latest reply came from; where
// Codealong types into its terminal, its account's models are lines of the chip's menu under its
// account, without the account's own (`/model` takes a name), and picking one asks the engine to
// switch that session; the box stays the height it was; and a session in the middle of a turn
// can't be switched until it ends. After the model is the effort it's on as its harness writes it,
// which follows the session, and the longest of both isn't cut off; the mode under the box is
// lowercase. Under the models are the account's levels, its own not among them, the session's
// checked, and picking another switches that session's effort and closes the menu; there's no
// row for where it runs, which a running session has already.
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
  // The chip says the account, then, quieter, `model · effort`.
  var detail = function () { var d = q('.ca-modelmenu-detail', chip()); return d ? d.textContent : ''; };
  var effort = function () { return detail().split(' · ')[1] || ''; };
  // Its own account's section of the menu, as against the accounts it could move to.
  var ownSection = function (menu) { return all('.ca-picker-section', menu).filter(function (x) { var n = q('.ca-quick-acct-name', x); return n && n.textContent === picked.account; })[0]; };
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
      check('the chip says its account, then the model it’s on', chip().textContent.indexOf(picked.account) === 0 && detail().length > 0 && detail().indexOf('Model') < 0, chip().textContent);
      check('the chip sits beside Send', !!q('.ca-composer-send', chip().closest('.ca-composer-buttons')), 'apart');
      if (picked.effort) check('after the model, the effort its latest reply was made at, as its harness writes it', effort() === picked.effort, effort() || 'none');
      var mode = q('.ca-peek .ca-composer-who .ca-mode-name');
      if (mode) check('in the box’s foot the mode is said lowercase', mode.innerText === mode.innerText.toLowerCase(), mode.innerText);
      // `/effort` set another, on the longest name a model has.
      window.__emit('board', withSession(function (s) { s.effort = 'xhigh'; s.model = 'claude-fable-5-1[1m]'; }));
      return until(function () { return effort() === 'xhigh'; }, 3000, 'the effort didn’t follow the session: ' + effort());
    })
    .then(function () {
      var cut = all('span', chip()).filter(function (s) { return s.scrollWidth > s.clientWidth + 1; });
      check('the longest model and effort aren’t cut off', cut.length === 0, cut.map(function (s) { return s.textContent; }).join(', '));
      var quiet = getComputedStyle(q('.ca-modelmenu-detail', chip())).color;
      check('the model and effort are quieter than the account', quiet !== getComputedStyle(chip()).color, quiet);
      window.__emit('board', withSession(function (s) { delete s.effort; }));
      return until(function () { return detail().indexOf(' · ') < 0; }, 3000, 'an effort stayed after the session stopped saying one');
    })
    .then(function () {
      window.__emit('board', board);
      chip().click();
      return until(function () { return !!q('.ca-peek .ca-modelmenu .ca-picker'); }, 3000, 'no model menu');
    })
    .then(function () {
      var menu = q('.ca-peek .ca-modelmenu .ca-picker');
      var own = ownSection(menu);
      var titles = own ? all('.ca-picker-title', own).map(function (t) { return t.textContent.trim(); }) : [];
      check('it lists the account’s models by name under it, without its own', titles.length === acct.models.length && titles.every(function (t) { return t.indexOf('Default') !== 0; }), titles.join(', '));
      check('none is checked: a reply doesn’t say which of the names it’s on', !q('.ca-picker-check .ca-icon', menu), 'checked');
      check('there’s no row for where it runs: it runs somewhere already', !/Runs in/.test((q('.ca-picker-foot', menu) || {}).textContent || ''), q('.ca-picker-foot', menu) ? q('.ca-picker-foot', menu).textContent : '');
      var rect = menu.getBoundingClientRect();
      check('the menu opens upward, inside the window', rect.top >= 0 && rect.bottom <= field().getBoundingClientRect().top + 1, rect.top + '–' + rect.bottom);
      var opt = all('.ca-picker-opt', own).filter(function (b) { return b.querySelector('.ca-picker-title').textContent.trim() === name(other); })[0];
      opt.click();
      return until(function () { return calls('switch_session_model').length === 1; }, 5000, 'no switch asked for');
    })
    .then(function () { return wait(50); })
    .then(function () {
      var asked = sent('switch_session_model')[0];
      check('picking one switches that session to it', asked.key === picked.key && asked.model === other.id, JSON.stringify(asked));
      check('the note says what happened', note().indexOf('Switched') === 0, note());
      check('nothing was sent as a prompt', calls('send_to_session').length === 0, String(calls('send_to_session').length));
      if (!acct.switchesEffort) return window.note('its account doesn’t switch effort: the effort row isn’t checked');
      window.__emit('board', withSession(function (s) { s.effort = 'xhigh'; }));
      return until(function () { return effort() === 'xhigh'; }, 3000, 'no effort to start from')
        .then(function () {
          chip().click();
          return until(function () { return !!q('.ca-peek .ca-modelmenu .ca-picker-foot .ca-segctl'); }, 3000, 'no effort row under the models');
        })
        .then(function () {
          var foot = q('.ca-peek .ca-modelmenu .ca-picker-foot');
          var levels = all('[role=radio]', foot);
          var words = levels.map(function (b) { return b.textContent.trim(); });
          var ids = acct.efforts.map(function (e) { return e.id; });
          check('the effort row has the account’s levels, as the chip says them, and not its own', words.join() === ids.join(), words.join(', '));
          var on = levels.filter(function (b) { return b.getAttribute('aria-checked') === 'true'; }).map(function (b) { return b.textContent.trim(); });
          check('the level it’s on is checked', on.join() === 'xhigh', on.join(', '));
          check('the row says a pick is for this session only', /This session only/.test(foot.textContent), foot.textContent);
          levels.filter(function (b) { return b.textContent.trim() === 'high'; })[0].click();
          return until(function () { return calls('switch_session_effort').length === 1; }, 5000, 'no effort switch asked for');
        })
        .then(function () { return wait(50); })
        .then(function () {
          var asked = sent('switch_session_effort')[0];
          check('picking a level switches that session’s effort to it', asked.key === picked.key && asked.effort === 'high', JSON.stringify(asked));
          check('and closes the menu', !q('.ca-peek .ca-modelmenu .ca-picker'), 'still open');
          check('the note says what happened', /^Switched its effort/.test(note()), note());
        });
    })
    .then(function () {
      // Its turn starts: it can't be switched until it ends; it can still be moved.
      window.__emit('board', withSession(function (s) { s.status = 'working'; }));
      return until(function () { return chip() && /once its turn ends/.test(chip().title); }, 3000, 'a working session’s chip doesn’t say its switches wait: ' + (chip() && chip().title));
    })
    .then(function () {
      chip().click();
      return until(function () { return !!q('.ca-peek .ca-modelmenu .ca-picker'); }, 3000, 'no menu mid-turn');
    })
    .then(function () {
      var menu = q('.ca-peek .ca-modelmenu .ca-picker');
      var lines = all('.ca-picker-opt', ownSection(menu));
      check('mid-turn its models wait for the turn to end', lines.length > 0 && lines.every(function (b) { return b.disabled && /once its turn ends/i.test(b.title); }), lines.map(function (b) { return b.disabled + ' ' + b.title; }).join(', '));
      check('and there’s no effort row to switch', !q('.ca-picker-foot .ca-segctl', menu), 'an effort row');
    })
    .then(done, stopped);
})();
