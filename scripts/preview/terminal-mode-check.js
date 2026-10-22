// Checks the mode of a running session in a terminal, in its box's foot (ModeLabel.tsx), where its
// harness steps through its modes with Shift+Tab (cycle.rs): it's a chip, lowercase, whose menu
// lists the modes in its harness's order with the one it's in checked; ⇧⇥ in the box asks the
// engine for the next of them; picking one asks for that one; and a mode Codealong expects after
// pressing ⇧⇥, which its harness writes only with the next prompt, is said in italics and its
// tooltip says so.
//   scripts/preview.sh peek '' scripts/preview/terminal-mode-check.js
(function () {
  // This runs before the app's script: pick a session on this Mac that's running in a terminal
  // whose modes Codealong can step through.
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
      if (!picked && s.live && !s.hosted && s.surface === 'cli' && s.status !== 'needs' && s.mode && s.mode.id && (s.modes || []).length > 1 && !s.archived && !l.hidden && !l.device) picked = s;
    });
  });
  if (!picked) { stopped('no running terminal session whose modes Codealong steps through'); return; }
  localStorage.setItem('codealong.preview.peek', picked.key);
  localStorage.removeItem('codealong.draft.' + picked.key);

  var ids = picked.modes.map(function (m) { return m.id; });
  var at = ids.indexOf(picked.mode.id);
  var next = picked.modes[(at + 1) % ids.length];
  var chip = function () { return q('.ca-peek .ca-composer-who .ca-modemenu-chip'); };
  var box = function () { return q('.ca-peek .ca-composer-field textarea'); };
  var withSession = function (change) {
    var b = JSON.parse(JSON.stringify(board));
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) change(s); }); });
    return b;
  };
  var asked = function () { return sent('switch_session_mode'); };

  say('terminal mode check: ' + picked.key + ' in ' + picked.mode.label + ', of ' + ids.join(', '));
  until(function () { return !!chip(); }, 20000, 'no mode chip in the box’s foot')
    .then(function () {
      var name = q('span', chip());
      check('it says the mode it’s in, lowercase', name.innerText === picked.mode.label.toLowerCase(), name.innerText);
      check('its tooltip says ⇧⇥ steps to the next', chip().title.indexOf(next.label) > 0, chip().title);
      check('it isn’t said as expected', !q('.ca-mode-expected', chip()), 'italic');
      box().focus();
      box().dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: 'Tab', code: 'Tab', shiftKey: true }));
      return until(function () { return asked().length === 1; }, 5000, 'no switch asked for on ⇧⇥');
    })
    .then(function () {
      check('⇧⇥ asks for the next mode in its order', asked()[0].key === picked.key && asked()[0].mode === next.id, JSON.stringify(asked()[0]));
      check('the box keeps the keyboard', document.activeElement === box(), document.activeElement && document.activeElement.className);
      // Claude Code writes the mode only with the next prompt: the engine says it's expected.
      window.__emit('board', withSession(function (s) { s.mode = Object.assign({}, next, { expected: true }); }));
      return until(function () { return !!q('.ca-mode-expected', chip()); }, 3000, 'an expected mode isn’t said as one');
    })
    .then(function () {
      var name = q('.ca-mode-expected', chip());
      check('an expected mode is said in italics', getComputedStyle(name).fontStyle === 'italic', getComputedStyle(name).fontStyle);
      check('its tooltip says Codealong expects it', /as Codealong expects/.test(chip().title), chip().title);
      chip().click();
      return until(function () { return !!q('.ca-peek .ca-modemenu .ca-picker'); }, 3000, 'no mode menu');
    })
    .then(function () {
      var menu = q('.ca-peek .ca-modemenu .ca-picker');
      var lines = all('.ca-picker-opt', menu);
      var titles = lines.map(function (b) { return q('.ca-picker-title', b).textContent.trim(); });
      check('the menu lists its modes in its harness’s order', titles.join() === picked.modes.map(function (m) { return m.label; }).join(), titles.join(', '));
      var checked = lines.filter(function (b) { return !!q('.ca-picker-check .ca-icon', b); }).map(function (b) { return q('.ca-picker-title', b).textContent.trim(); });
      check('the one it’s in is checked', checked.join() === next.label, checked.join(', '));
      var other = lines[(at + 2) % ids.length];
      var want = picked.modes[(at + 2) % ids.length];
      other.click();
      return until(function () { return asked().length === 2; }, 5000, 'no switch asked for on a pick').then(function () { return want; });
    })
    .then(function (want) {
      check('picking one asks for that one', asked()[1].mode === want.id, JSON.stringify(asked()[1]));
      check('nothing was sent as a prompt', calls('send_to_session').length === 0, String(calls('send_to_session').length));
    })
    .then(done, stopped);
})();
