// Checks the model and effort a session says beside Send in its box between turns (SessionReply.tsx,
// SessionModel.tsx), where its next turn goes on with them: a Codex app thread, since the app
// starts its next turn on them, and an OpenCode session, since its service runs a prompt that
// names no model on the session's. The Codex thread's are a menu, since the app takes a switch
// for its next turn (codex/ipc.rs `codex_settings_request`): picking a model or a level asks the
// engine to switch that thread, and mid-turn too; OpenCode's are no menu. A Codex thread of the
// terminal's says them only while it runs: between turns a send resumes it, on whatever that
// picks.
//   scripts/preview.sh peek '' scripts/preview/between-turns-model-check.js
(function () {
  // This runs before the app's script: pick a Codex app thread on this Mac that says its model.
  var read = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var board = read('./board.json');
  var picked = null;
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (!picked && s.harness === 'codex' && s.surface === 'app' && s.model && !s.hosted && s.status !== 'needs' && !s.archived && !l.hidden && !l.device) picked = s;
    });
  });
  if (!picked) { stopped('no Codex app thread that says its model'); return; }
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);

  var accounts = read('./start-options.json').accounts;
  var acct = accounts.filter(function (a) { return a.name === picked.account && a.harness === picked.harness; })[0];
  if (!acct || !acct.switchesInApp) { stopped('the thread’s account doesn’t say the app switches its sessions (start-options.json)'); return; }
  var said = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-said'); };
  var chip = function () { return q('.ca-peek .ca-composer-buttons .ca-modelmenu-chip'); };
  var effort = function () { var d = q('.ca-modelmenu-detail', chip() || said()); return d ? d.textContent : ''; };
  var note = function () { var n = q('.ca-peek .ca-composer-note'); return n ? n.textContent : ''; };
  // The board the box gets, and the session the stub says the way for (fake-tauri.js answers
  // `session_way` from `__madeSessions` before its board.json); the way is asked again as the
  // session's surface changes.
  var become = function (change) {
    var b = JSON.parse(JSON.stringify(board));
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) change(s); }); });
    window.__madeSessions = window.__madeSessions || {};
    window.__madeSessions[picked.key] = JSON.parse(JSON.stringify(picked));
    change(window.__madeSessions[picked.key]);
    window.__emit('board', b);
  };

  say('between turns model check: ' + picked.key + ' on ' + picked.model + (picked.effort ? ' ' + picked.effort : ''));
  // Between turns, as a thread no turn runs in is.
  become(function (s) { s.live = false; s.status = 'idle'; s.effort = 'xhigh'; });
  until(function () { return !!chip() && effort() === 'xhigh'; }, 20000, 'no model menu beside Send for a Codex app thread between turns')
    .then(function () {
      var listed = acct.models.filter(function (m) { return m.id === picked.model; })[0];
      var named = listed ? listed.label.split(':')[0] : picked.model;
      check('it says the model the thread is on, by the name its account gives it', chip().textContent.indexOf(named) === 0, chip().textContent + ' (' + named + ')');
      check('it’s a menu that switches it from its next turn', /from its next turn/.test(chip().title) && !chip().disabled, chip().title);
      chip().click();
      return until(function () { return !!q('.ca-peek .ca-modelmenu .ca-picker'); }, 3000, 'no model menu');
    })
    .then(function () {
      var menu = q('.ca-peek .ca-modelmenu .ca-picker');
      var titles = all('.ca-picker-title', menu).map(function (t) { return t.textContent.trim(); });
      check('it lists the account’s models', titles.length === acct.models.length, titles.join(', '));
      all('.ca-picker-opt', menu).pop().click();
      return until(function () { return calls('switch_session_model').length === 1; }, 5000, 'no switch asked for');
    })
    .then(function () { return wait(50); })
    .then(function () {
      var asked = sent('switch_session_model')[0];
      check('picking one switches that thread', asked.key === picked.key && acct.models.some(function (m) { return m.id === asked.model; }), JSON.stringify(asked));
      check('nothing was sent as a prompt', calls('send_to_session').length === 0, String(calls('send_to_session').length));
      // Its turn starts: the app still takes a switch, for the turn after.
      become(function (s) { s.live = true; s.status = 'working'; s.effort = 'xhigh'; });
      return wait(200);
    })
    .then(function () {
      check('mid-turn the chip still switches it, from its next turn', !!chip() && !chip().disabled && /from its next turn/.test(chip().title), chip() ? chip().title + (chip().disabled ? ' (off)' : '') : 'no chip');
      chip().click();
      return until(function () { return !!q('.ca-peek .ca-modelmenu .ca-picker-foot .ca-segctl'); }, 3000, 'no effort row under the models');
    })
    .then(function () {
      var foot = q('.ca-peek .ca-modelmenu .ca-picker-foot');
      var levels = all('[role=radio]', foot);
      check('the effort row has the account’s levels', levels.map(function (b) { return b.textContent.trim(); }).join() === acct.efforts.map(function (e) { return e.id; }).join(), levels.map(function (b) { return b.textContent.trim(); }).join(', '));
      levels.filter(function (b) { return b.textContent.trim() === 'high'; })[0].click();
      return until(function () { return calls('switch_session_effort').length === 1; }, 5000, 'no effort switch asked for');
    })
    .then(function () { return wait(50); })
    .then(function () {
      var asked = sent('switch_session_effort')[0];
      check('picking a level switches that thread’s effort', asked.key === picked.key && asked.effort === 'high', JSON.stringify(asked));
      check('the note says what happened', /^Switched its effort/.test(note()), note());
      // The same thread made in a terminal: between turns a send resumes it, so it says nothing
      // and switches nothing (the chip there is the resume's own, ResumeBar.tsx).
      become(function (s) { s.live = false; s.status = 'idle'; s.surface = 'cli'; });
      return until(function () { return !said() && !(chip() && /^Switch its/.test(chip().title)); }, 5000, 'a terminal thread between turns still says its model');
    })
    .then(function () {
      // An OpenCode session between turns, its model as OpenCode names it and its variant.
      become(function (s) { s.harness = 'opencode'; s.live = false; s.status = 'idle'; s.surface = 'app'; s.model = 'big-pickle'; s.effort = 'high'; });
      return until(function () { return !!said() && said().textContent.indexOf('big-pickle') === 0 && effort() === 'high'; }, 5000, 'an OpenCode session between turns: ' + (said() ? said().textContent : 'nothing'));
    })
    .then(function () {
      check('it’s no menu for an OpenCode session: nothing switches one from outside', !chip(), 'a menu');
    })
    .then(done, stopped);
})();
