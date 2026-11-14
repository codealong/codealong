// A session a PR's or todo's panel lists shows in the peek, beside the panel (MainPanel.tsx
// `peekFromPanel`, PanelSession.tsx `usePeekOnHover`, peek.rs `beside`), while the pointer rests
// on it: resting on its chip a moment asks for the peek without the keyboard, as a hover over a
// pill does, by the panel's sides and the chip's line, and the PR's or todo's panel stays, its row
// still selected; a pointer that leaves or presses before then asks for nothing. With the details
// open, resting on a session's row asks by that row, and a double-click opens the session where
// it runs without the peek (in Sessions, for one Codealong runs), putting the peek away. A click
// on a session's chip shows it in Sessions, as the peek's Main window button does, putting the
// peek away and asking for none. What another window asks the main window to say shows as a
// toast. In the peek's own page, an open with a search's words opens its find bar on them, and
// what goes wrong in a peek opened beside the main window is said there as a toast, where one
// opened from a pill says it on the bar.
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
  // The pointer coming onto something and leaving it, from and to outside the page, and the
  // button going down on it without a click.
  var enter = function (el) { el.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, cancelable: true, pointerType: 'mouse', buttons: 0 })); };
  var leave = function (el) { el.dispatchEvent(new PointerEvent('pointerout', { bubbles: true, cancelable: true, pointerType: 'mouse', buttons: 0 })); };
  var down = function (el) { el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, pointerType: 'mouse', button: 0, buttons: 1 })); };
  // A view's keys (mainKeys.ts `VIEW_CHORDS`): ⇧⌘ and its letter.
  var go = function (letter) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: letter, code: 'Key' + letter, metaKey: true, shiftKey: true })); };
  var shown = function () { var m = q('main.ca-board'); return m ? m.getAttribute('aria-label') : null; };
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
    check('beside the panel, without the keyboard', p.beside === true && p.focus === false, JSON.stringify(p));
    check('by the panel’s sides', !!p.pill && near(p.pill.x, side.left) && near(p.pill.w, side.width), JSON.stringify(p.pill) + ' / ' + Math.round(side.left) + '+' + Math.round(side.width));
    check('level with what it rests on', !!p.pill && near(p.pill.y, at.top) && near(p.pill.h, at.height), JSON.stringify(p.pill) + ' / ' + Math.round(at.top) + '+' + Math.round(at.height));
  };

  // The panel's chips, its details closed: they stay open for the next PR or todo once opened.
  var chips = function (panel) {
    if (q('.ca-tree', panel())) press(q('.ca-detailsbtn', panel()));
    return until(function () { return all('.ca-places .ca-sesschip', panel())[0]; }, 3000, 'the panel’s session chips');
  };

  // `kind`: 'pr' or 'todo'; `panel` the panel, `row` the list's row it's for, `select` its view
  // with the row selected again, from wherever a click went.
  var run = function (kind, panel, row, select) {
    var chip = q('.ca-places .ca-sesschip', panel());
    if (!chip) return stopped('the ' + kind + '’s panel lists no session');
    var from = sent('peek_open').length;
    enter(chip);
    check('resting on a session’s chip asks for nothing at once', sent('peek_open').length === from);
    return wait(400)
      .then(function () {
        asked('resting on a session’s chip', from, panel(), chip, chip);
        check('the ' + kind + '’s panel stays', !!panel() && !q('.ca-panelback') && !q('.ca-sessinsp'));
        check('its row still selected', row().getAttribute('aria-selected') === 'true');
        leave(chip);
        // Gone before the moment's up, then back and pressed.
        from = sent('peek_open').length;
        enter(chip);
        return wait(100);
      })
      .then(function () {
        leave(chip);
        enter(chip);
        down(chip);
        return wait(400);
      })
      .then(function () {
        check('a pointer that leaves or presses before the moment’s up asks for nothing', sent('peek_open').length === from, String(sent('peek_open').length - from));
        press(q('.ca-detailsbtn', panel()));
        return until(function () { return q('.ca-psess-main', panel()); }, 3000, 'the tree, with the details open');
      })
      .then(function (main) {
        from = sent('peek_open').length;
        enter(main);
        return wait(400).then(function () {
          asked('resting on a session’s row', from, panel(), main, main);
          leave(main);
          var opens = q('.ca-psess-open', main.closest('.ca-psess'));
          if (opens.disabled) return note('its first session is on another Mac: a double-click wasn’t checked');
          // One Codealong runs lives in Sessions' panel, which is where it opens.
          var hosted = keysTitled(titleOf(main)).some(function (k) {
            return board.lanes.some(function (l) { return l.sessions.some(function (s) { return s.key === k && /\|hosted$/.test(s.openTarget || ''); }); });
          });
          from = sent('peek_open').length;
          var opened = sent('open_target').length;
          var hid = sent('peek_hide').length;
          twice(main);
          return wait(400).then(function () {
            if (hosted) check('a double-click shows one Codealong runs in Sessions, where it lives', !panel() && !!q('.ca-sessinsp'));
            else check('a double-click opens it where it runs', sent('open_target').length === opened + 1, String(sent('open_target').length - opened));
            check('without the peek, putting it away', sent('peek_open').length === from && sent('peek_hide').length > hid, (sent('peek_open').length - from) + ' opens, ' + (sent('peek_hide').length - hid) + ' hides');
          });
        });
      })
      .then(function () { return panel() ? null : select(); })
      .then(function () { return chips(panel); })
      .then(function (chip2) {
        var title = titleOf(chip2);
        from = sent('peek_open').length;
        var hid = sent('peek_hide').length;
        press(chip2);
        var heading = function () { var h = q('.ca-sessinsp .ca-sessinsp-title'); return h ? h.textContent : null; };
        // Sessions lists it whatever it is, a finished review of the PR too (boardLists.ts
        // `revealing`), so it's that session's panel there, not the top row's.
        return until(function () { return !panel() && heading() === title; }, 3000)
          .catch(function () {})
          .then(function () {
            check('a click on a session’s chip shows it in Sessions', shown() === 'Sessions', shown());
            check('that session', heading() === title, title + ' / ' + heading());
            check('putting the peek away, asking for none', sent('peek_open').length === from && sent('peek_hide').length > hid, (sent('peek_open').length - from) + ' opens, ' + (sent('peek_hide').length - hid) + ' hides');
          });
      })
      .then(function () {
        // What the peek says went wrong, as it asks this window to say it.
        window.__emit('toast', 'Didn’t send to it, so it’s back in its box: Ghostty said no.');
        return until(function () { return q('.ca-toast'); }, 3000, 'the toast');
      })
      .then(function (t) {
        check('what the peek says went wrong shows as a toast here', t.textContent.indexOf('Ghostty said no.') >= 0, t.textContent);
      });
  };

  // The view (its key's letter), from wherever the window is, with `pick`'s row selected and its
  // panel up.
  var selecting = function (letter, list, pick, panel) {
    return function () {
      var there = q(list) ? Promise.resolve() : (go(letter), until(function () { return q(list); }, 5000, 'the view again'));
      return there.then(function () {
        press(pick());
        return until(panel, 5000, 'the panel');
      });
    };
  };

  var prs = function () {
    var pick = function () { return all('.ca-prrow').filter(function (r) { return !!q('.ca-prrow-agents', r); })[0]; };
    if (!pick()) return stopped('no PR on this board has a session on this Mac');
    var panel = function () { return q('.ca-prpanel'); };
    var rowOf = function () { var row = pick(); return row.closest('[role="option"]') || row; };
    var select = selecting('P', '.ca-prrow', pick, panel);
    return select()
      .then(function () { return chips(panel); })
      .then(function () { return run('PR', panel, rowOf, select); });
  };

  var todos = function () {
    var pick = function () { return q('.ca-todorow'); };
    var panel = function () { return q('.ca-todoinsp'); };
    var rowOf = function () { var row = pick(); return row.closest('[role="option"]') || row; };
    var select = selecting('T', '.ca-todorow', pick, panel);
    return select()
      .then(function () { return chips(panel); })
      .then(function () { return run('todo', panel, rowOf, select); });
  };

  // Keeping the session on the desktop fails, for something the peek says went wrong.
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd !== 'keep_session') return real.apply(this, arguments);
    window.__calls.push([cmd, args, options]);
    return Promise.reject('There’s no room on the desktop.');
  };
  var emitted = function (event) { return calls('plugin:event|emit_to').filter(function (c) { return c[1] && c[1].event === event; }).map(function (c) { return c[1]; }); };

  // The peek's page, opened as the main window asks it, with a search's words: its conversation's
  // find bar opens on them, leaving the box the keyboard; and what goes wrong is said in the main
  // window as a toast, where one opened from a pill says it on the bar.
  var peek = function () {
    var field = function () { return q('.ca-peek .ca-thread-find input'); };
    var keep = function () { return q('.ca-peek .ca-sessinsp-head button[aria-label="Keep on the desktop"]'); };
    return until(function () { return q('.ca-peek .ca-sessinsp-head'); }, 20000, 'the peek’s session')
      .then(function () {
        check('opened from the bar, no find bar', !field());
        keep().click();
        return until(function () { return emitted('bar-note').length > 0; }, 3000, 'the bar’s note');
      })
      .then(function () {
        check('opened from the bar, what goes wrong goes on the bar', emitted('bar-note')[0].payload.message === 'There’s no room on the desktop.' && emitted('toast').length === 0, JSON.stringify(emitted('bar-note')));
        var lane = board.lanes.filter(function (l) { return l.sessions.length > 0; })[0];
        var s = lane.sessions[0];
        window.__emit('peek-open', { id: 7, lane: lane, session: s, terminal: board.apps.terminal, apps: board.apps, theme: board.theme, zoom: board.zoom, room: 700, below: true, focus: true, rename: false, find: 'quokka bridge', beside: 'main' });
        return until(function () { return !!field(); }, 5000, 'the find bar');
      })
      .then(function () {
        check('opened with a search’s words, its find bar is on them', field().value === 'quokka bridge', field().value);
        check('without the keyboard', document.activeElement !== field());
        var notes = emitted('bar-note').length;
        keep().click();
        return until(function () { return emitted('toast').length > 0; }, 3000, 'the toast').then(function () {
          var t = emitted('toast')[0];
          check('opened beside the main window, it’s said there as a toast', t.target && (t.target.label || t.target) && JSON.stringify(t.target).indexOf('main') >= 0 && t.payload === 'There’s no room on the desktop.', JSON.stringify(t));
          check('and not on the bar', emitted('bar-note').length === notes);
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
