// Checks the mode of a session Codealong runs, at the left of its box's foot (ModeLabel.tsx
// `ModeMenu`): a chip, said lowercase as the label it stands for, that opens the modes its agent
// offers above the box at its left edge, the one it's in checked; picking another asks the engine
// to switch that session and shows the pick at once, then the mode its agent says it put it in;
// ⇧⇥ in the box steps to the next, as in Claude Code's terminal, mid-turn too. A session that runs elsewhere only says its mode, and ⇧⇥
// asks nothing.
//   scripts/preview.sh peek '' scripts/preview/session-mode-check.js
(function () {
  // This runs before the app's script: pick a running session on this Mac, between turns.
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
      if (!picked && s.live && s.harness === 'claude' && s.status !== 'working' && s.status !== 'needs' && !s.stalledSince && !s.archived && !l.hidden && !l.device) picked = s;
    });
  });
  if (!picked) { stopped('no running Claude Code session between turns'); return; }
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);

  // As claude-agent-acp offers them, named as the engine names Claude Code's (claude/mode.rs).
  var MODES = [
    { id: 'default', kind: 'asks', label: 'Manual', detail: 'Asks before it edits files or runs commands that do more than look.' },
    { id: 'acceptEdits', kind: 'edits', label: 'Accept edits', detail: 'Edits files without asking, and asks before other commands.' },
    { id: 'plan', kind: 'plan', label: 'Plan', detail: 'Plans without changing anything, until you take the plan.' },
    { id: 'auto', kind: 'auto', label: 'Auto', detail: 'A classifier allows what looks safe and asks you about the rest.' },
    { id: 'bypassPermissions', kind: 'skips', label: 'Bypass', detail: 'Bypasses permissions: does everything without asking.' },
  ];
  var withSession = function (change) {
    var b = JSON.parse(JSON.stringify(board));
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) change(s); }); });
    return b;
  };
  var hosted = function (mode) {
    return withSession(function (s) {
      s.hosted = true;
      s.live = true;
      s.status = 'idle';
      s.modes = MODES;
      s.mode = MODES.filter(function (m) { return m.id === mode; })[0];
    });
  };
  var chip = function () { return q('.ca-peek .ca-composer-who .ca-modemenu-chip'); };
  var field = function () { return q('.ca-peek .ca-composer-field'); };
  var box = function () { return q('.ca-peek .ca-composer-field textarea'); };
  var note = function () { var n = q('.ca-peek .ca-composer-note'); return n ? n.textContent : ''; };
  var said = function () { return chip() ? chip().innerText.trim() : ''; };
  var shiftTab = function () { box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true, cancelable: true })); };

  say('session mode check: ' + picked.key);
  until(function () { return !!field(); }, 20000, 'no box')
    .then(function () {
      window.__emit('board', hosted('default'));
      return until(function () { return !!chip(); }, 3000, 'no mode chip in the box’s foot');
    })
    .then(function () {
      check('the chip says the mode it’s in, lowercase', said() === 'manual', said());
      check('it sits at the left of the foot, before the account', chip().closest('.ca-composer-who').firstElementChild.contains(chip()), 'elsewhere');
      check('its tooltip says what the mode means and the key for the next', /Its mode: Manual\./.test(chip().title) && /⇧⇥: Accept edits/.test(chip().title), chip().title);
      chip().click();
      return until(function () { return !!q('.ca-peek .ca-modemenu .ca-picker'); }, 3000, 'no mode menu');
    })
    .then(function () {
      var menu = q('.ca-peek .ca-modemenu .ca-picker');
      var titles = all('.ca-picker-title', menu).map(function (t) { return t.textContent.trim(); });
      check('it lists the modes its agent offers, in its order', titles.join() === 'Manual,Accept edits,Plan,Auto,Bypass', titles.join(', '));
      var on = all('.ca-picker-opt', menu).filter(function (o) { return !!q('.ca-picker-check .ca-icon', o); }).map(function (o) { return q('.ca-picker-title', o).textContent.trim(); });
      check('the mode it’s in is checked', on.join() === 'Manual', on.join(', '));
      var rect = menu.getBoundingClientRect();
      var f = field().getBoundingClientRect();
      check('the menu opens upward, inside the window', rect.top >= 0 && rect.bottom <= f.top + 1, rect.top + '–' + rect.bottom);
      check('at the box’s left edge, under the chip', Math.abs(rect.left - f.left) <= 2, rect.left + ' against ' + f.left);
      var auto = all('.ca-picker-opt', menu).filter(function (o) { return q('.ca-picker-title', o).textContent.trim() === 'Auto'; })[0];
      auto.click();
      return until(function () { return calls('switch_session_mode').length === 1; }, 5000, 'no switch asked for');
    })
    .then(function () { return wait(50); })
    .then(function () {
      var asked = sent('switch_session_mode')[0];
      check('picking one switches that session to it, by its id', asked.key === picked.key && asked.mode === 'auto', JSON.stringify(asked));
      check('the menu closed', !q('.ca-peek .ca-modemenu .ca-picker'), 'still open');
      check('the pick shows at once', said() === 'auto', said());
      check('the note says what happened', /^Switched its mode/.test(note()), note());
      check('nothing was sent as a prompt', calls('send_to_session').length === 0, String(calls('send_to_session').length));
      // Mid-turn, ⇧⇥ steps on from the pick, still showing while the board catches up.
      window.__emit('board', withSession(function (s) { s.hosted = true; s.live = true; s.status = 'working'; s.modes = MODES; s.mode = MODES[0]; }));
      return wait(100);
    })
    .then(function () {
      box().focus();
      shiftTab();
      return until(function () { return calls('switch_session_mode').length === 2; }, 3000, '⇧⇥ asked for no switch');
    })
    .then(function () { return wait(50); })
    .then(function () {
      check('⇧⇥ steps to the next mode, mid-turn too', sent('switch_session_mode')[1].mode === 'bypassPermissions', JSON.stringify(sent('switch_session_mode')[1]));
      check('and the box keeps the keyboard', document.activeElement === box(), document.activeElement && document.activeElement.tagName);
      // A session that runs elsewhere only says its mode, and ⇧⇥ asks nothing.
      window.__emit('board', withSession(function (s) { s.live = true; s.hosted = false; s.status = 'idle'; delete s.modes; s.mode = { kind: 'asks', label: 'Manual', detail: 'Asks before it edits files.' }; }));
      return until(function () { return !chip(); }, 3000, 'a session that runs elsewhere has a mode chip');
    })
    .then(function () {
      var label = q('.ca-peek .ca-composer-who .ca-mode-name');
      check('elsewhere the mode is only said', !!label && label.innerText === 'manual', label ? label.innerText : 'none');
      box().focus();
      shiftTab();
      return wait(100);
    })
    .then(function () {
      check('⇧⇥ asks nothing of a session that runs elsewhere', calls('switch_session_mode').length === 2, String(calls('switch_session_mode').length));
      // Its agent puts it in another mode than the one asked for (Accept edits for Auto, on a
      // model that doesn't take it), and the board doesn't change: the chip says where it is.
      var real = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
        if (cmd === 'switch_session_mode') return real(cmd, args, options).then(function () { return { message: 'It’s in Accept edits mode: its agent didn’t take Auto.', mode: 'acceptEdits' }; });
        return real(cmd, args, options);
      };
      window.__emit('board', hosted('acceptEdits'));
      return until(function () { return !!chip(); }, 3000, 'no chip again');
    })
    .then(function () {
      chip().click();
      return until(function () { return !!q('.ca-peek .ca-modemenu .ca-picker'); }, 3000, 'no mode menu');
    })
    .then(function () {
      all('.ca-peek .ca-modemenu .ca-picker-opt').filter(function (o) { return q('.ca-picker-title', o).textContent.trim() === 'Auto'; })[0].click();
      return until(function () { return calls('switch_session_mode').length === 3 && /didn’t take/.test(note()); }, 3000, 'no answer from the switch');
    })
    .then(function () { return wait(100); })
    .then(function () {
      check('a mode its agent didn’t take doesn’t stay on the chip', said() === 'accept edits', said());
    })
    .then(done, stopped);
})();
