// A session a PR's or todo's panel lists shows in the peek, beside the panel (MainPanel.tsx
// `peekFromPanel`, PanelSession.tsx, peek.rs `beside`), not in the panel's place: a click on its
// chip asks for the peek ready to answer, by the panel's sides and the line of chips the session's
// worktree has, and the PR's or todo's panel stays, its row still selected. With the details
// open, a click on a session's row waits a moment for a second before it asks, by that row, and a
// double-click opens the session where it runs without the peek (in Sessions, for one Codealong
// runs). In the peek's own page, an open with a search's words opens its find bar on them.
//   scripts/preview.sh prs '' scripts/preview/panel-peek-check.js
//   PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/panel-peek-check.js
//   scripts/preview.sh peek '' scripts/preview/panel-peek-check.js
(function () {
  localStorage.removeItem('codealong.prDetailsOpen');
  localStorage.removeItem('codealong.todoPlacesOpen');
  localStorage.removeItem('codealong.prPanelMax');
  // On two lines, a PR's row names its sessions, so the check can pick one that has some.
  localStorage.setItem('codealong.prRows', 'two');
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var twice = function (el) {
    press(el);
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0, detail: 2 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 2 }));
    el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true, detail: 2 }));
  };
  var near = function (a, b) { return Math.abs(a - b) < 1; };
  var titleOf = function (el) { var t = q('.ca-sesschip-title, .ca-psess-title', el); return t ? t.textContent : null; };
  var board = null;
  var keysTitled = function (title) {
    var keys = [];
    board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.title === title) keys.push(s.key); }); });
    return keys;
  };
  // The peek asked for since `from` asks: one, beside `panel`, level with `line`, for a session
  // titled as `clicked` is. The window takes points, which the page's pixels are at its zoom.
  var asked = function (what, from, panel, line, clicked) {
    var opens = sent('peek_open');
    var p = opens[opens.length - 1];
    var z = (board.zoom || 100) / 100;
    var scaled = function (r) { return { left: r.left * z, width: r.width * z, top: r.top * z, height: r.height * z }; };
    var side = scaled(panel.getBoundingClientRect());
    var at = scaled(line.getBoundingClientRect());
    check(what + ' asks for the peek, once', opens.length === from + 1, String(opens.length - from));
    if (!p) return;
    check('for that session', keysTitled(titleOf(clicked)).indexOf(p.key) >= 0, p.key + ' for “' + titleOf(clicked) + '”');
    check('beside the panel, ready to answer', p.beside === true && p.focus === true, JSON.stringify(p));
    check('by the panel’s sides', !!p.pill && near(p.pill.x, side.left) && near(p.pill.w, side.width), JSON.stringify(p.pill) + ' / ' + Math.round(side.left) + '+' + Math.round(side.width));
    check('level with what was clicked', !!p.pill && near(p.pill.y, at.top) && near(p.pill.h, at.height), JSON.stringify(p.pill) + ' / ' + Math.round(at.top) + '+' + Math.round(at.height));
  };

  // `kind`: 'pr' or 'todo'; `panel` the panel, `row` the list's row it's for.
  var run = function (kind, panel, row) {
    var places = q('.ca-places', panel());
    var chip = places && all('.ca-sesschip', places)[0];
    if (!chip) return stopped('the ' + kind + '’s panel lists no session');
    var from = sent('peek_open').length;
    press(chip);
    return wait(400)
      .then(function () {
        asked('a click on a session’s chip', from, panel(), chip.closest('.ca-places-group'), chip);
        check('the ' + kind + '’s panel stays', !!panel() && !q('.ca-panelback') && !q('.ca-sessinsp'));
        check('its row still selected', row().getAttribute('aria-selected') === 'true');
        press(q('.ca-detailsbtn', panel()));
        return until(function () { return q('.ca-psess-main', panel()); }, 3000, 'the tree, with the details open');
      })
      .then(function (main) {
        from = sent('peek_open').length;
        press(main);
        check('a click on a session’s row waits for a second', sent('peek_open').length === from);
        return wait(400).then(function () {
          asked('then it', from, panel(), main, main);
          var opens = q('.ca-psess-open', main.closest('.ca-psess'));
          if (opens.disabled) return note('its first session is on another Mac: a double-click wasn’t checked');
          // One Codealong runs lives in Sessions' panel, which is where it opens.
          var hosted = keysTitled(titleOf(main)).some(function (k) {
            return board.lanes.some(function (l) { return l.sessions.some(function (s) { return s.key === k && /\|hosted$/.test(s.openTarget || ''); }); });
          });
          from = sent('peek_open').length;
          var opened = sent('open_target').length;
          twice(main);
          return wait(400).then(function () {
            if (hosted) check('a double-click shows one Codealong runs in Sessions, where it lives', !panel() && !!q('.ca-sessinsp'));
            else check('a double-click opens it where it runs', sent('open_target').length === opened + 1, String(sent('open_target').length - opened));
            check('without the peek', sent('peek_open').length === from, String(sent('peek_open').length - from));
          });
        });
      })
      .then(function () {
        if (panel()) press(q('.ca-detailsbtn', panel()));
      });
  };

  var prs = function () {
    var rows = all('.ca-prrow').filter(function (r) { return !!q('.ca-prrow-agents', r); });
    if (rows.length === 0) return stopped('no PR on this board has a session on this Mac');
    var row = rows[0];
    var rowOf = function () { return row.closest('[role="option"]') || row; };
    press(row);
    return until(function () { return q('.ca-prinsp-head .ca-places'); }, 5000, 'the PR panel’s chips').then(function () {
      return run('PR', function () { return q('.ca-prpanel'); }, rowOf);
    });
  };

  var todos = function () {
    var row = q('.ca-todorow');
    var rowOf = function () { return row.closest('[role="option"]') || row; };
    press(row);
    return until(function () { return q('.ca-todoinsp'); }, 5000, 'the todo panel').then(function () {
      return run('todo', function () { return q('.ca-todoinsp'); }, rowOf);
    });
  };

  // The peek's page, opened as the main window asks it with a search's words: its conversation's
  // find bar opens on them, leaving the box the keyboard.
  var peek = function () {
    var field = function () { return q('.ca-peek .ca-thread-find input'); };
    return until(function () { return q('.ca-peek .ca-sessinsp-head'); }, 20000, 'the peek’s session').then(function () {
      check('opened from the bar, no find bar', !field());
      var lane = board.lanes.filter(function (l) { return l.sessions.length > 0; })[0];
      var s = lane.sessions[0];
      window.__emit('peek-open', { id: 7, lane: lane, session: s, terminal: board.apps.terminal, apps: board.apps, theme: board.theme, zoom: board.zoom, room: 700, below: true, focus: true, rename: false, find: 'quokka bridge' });
      return until(function () { return !!field(); }, 5000, 'the find bar').then(function () {
        check('opened with a search’s words, its find bar is on them', field().value === 'quokka bridge', field().value);
        check('without the keyboard', document.activeElement !== field());
      });
    });
  };

  say('panel peek check: waiting for the list');
  window.__TAURI_INTERNALS__
    .invoke('get_board')
    .then(function (b) {
      board = b;
      return until(function () { return q('.ca-prrow') || q('.ca-todorow') || q('.ca-peek-root') || null; }, 20000, 'a PR, a todo or the peek');
    })
    .then(function () { return q('.ca-peek-root') ? peek() : q('.ca-prrow') ? prs() : todos(); })
    .then(done)
    .catch(stopped);
})();
