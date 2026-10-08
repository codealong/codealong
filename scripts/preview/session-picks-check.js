// Checks picking several sessions in the Sessions view (picks.ts, SessionPickPanel.tsx, menus.ts
// `sessionsActions`, mainKeys.ts): five sessions of this device's, two waiting on you, one with a
// terminal of its own. ⌘-click picks another beside the selected one and ⇧-click a run; the
// panel says how many and what each action takes; a right-click on a picked row is the menu for
// all of them, on another row that row's own, leaving the pick. ⌘A picks every one, ⌘E marks the
// two waiting seen and keeps the pick, Esc goes back to one, ⇧↓ picks the rows below, ⌃⌘A
// archives them with one Undo, which puts them all back, and Done on the strip asks the engine
// about each in turn.
//   scripts/preview.sh sessions '' scripts/preview/session-picks-check.js
(function () {
  // This runs before the app's script: the board it gets has five sessions, made from real ones
  // on this device, and no other.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  localStorage.removeItem('codealong.sessionSort');
  localStorage.removeItem('codealong.age');
  var now = Date.now();
  var plan = [
    { status: 'unread', at: now - 30 * 60e3, live: true },
    { status: 'unread', at: now - 20 * 60e3, live: false },
    { status: 'idle', at: now - 60 * 60e3, live: true },
    { status: 'idle', at: now - 120 * 60e3, live: false },
    { status: 'idle', at: now - 180 * 60e3, live: false },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.pinned = [];
      board.lanes.forEach(function (l) {
        l.sessions = l.sessions.filter(function (s) {
          if (picked.some(function (x) { return x.key === s.key && x.lane === l.id; })) return true;
          if (l.hidden || l.device || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return false;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.at;
          s.lastActivity = p.at;
          s.lastPromptAt = p.at;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.live = p.live;
          s.surface = 'cli';
          s.pid = p.live ? 4242 + picked.length : null;
          s.snoozedUntil = undefined;
          s.title = 'Pick ' + 'ABCDE'[picked.length];
          s.ownWorktree = undefined;
          s.background = undefined;
          s.attachCommand = null;
          picked.push({ key: s.key, lane: l.id });
          return true;
        });
      });
      return board;
    });
  };

  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  // The sessions as the list has them now (marking seen moves them), by letter.
  var order = function () {
    return all('.ca-sessionlist [role=option]').map(function (el) {
      var i = picked.findIndex(function (x) { return el.id === 'row-' + x.lane + '::' + x.key; });
      return i < 0 ? '' : 'ABCDE'[i];
    }).join('');
  };
  var sorted = function (letters) { return letters.split('').sort().join(''); };
  var listed = function (i) { var r = row(i); return !!r && !r.closest('.ca-archived'); };
  var selectedRows = function () {
    return picked.map(function (_, i) { return row(i) && row(i).getAttribute('aria-selected') === 'true' ? 'ABCDE'[i] : ''; }).join('');
  };
  var press = function (i, init) { row(i).dispatchEvent(new MouseEvent('mousedown', Object.assign({ bubbles: true, cancelable: true, button: 0 }, init))); };
  var keyDown = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var pickPanel = function () { return q('.ca-sesspick'); };
  var head = function () { var h = q('.ca-sesspick-head'); return h ? h.textContent : ''; };
  var names = function () { return all('.ca-sesspick-name').map(function (b) { return b.textContent; }).join(','); };
  var hotkey = function (label) {
    return all('.ca-sesspick .ca-hotkey').filter(function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };
  var toast = function () { var t = q('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };
  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };

  say('session picks check: waiting for the list');
  until(function () { return picked.length === plan.length && row(4); }, 20000, 'five sessions listed')
    .then(function () { return wait(300); })
    .then(function () {
      press(0);
      return until(function () { return title() === 'Pick A'; }, 3000, 'Pick A’s panel');
    })
    .then(function () {
      press(2, { metaKey: true });
      return until(pickPanel, 3000, 'the pick panel');
    })
    .then(function () {
      check('⌘-click picks a session beside the selected one', selectedRows() === 'AC', selectedRows());
      check('the panel says how many', head() === '2 sessions', head());
      check('and lists them in the list’s order', names() === 'Pick A,Pick C', names());
      press(4, { shiftKey: true });
      return wait(100);
    })
    .then(function () {
      check('⇧-click picks the run from the last one picked', selectedRows() === 'CDE', selectedRows());
      var close = hotkey('Close');
      var seen = hotkey('Seen');
      check('Close says it takes one of the three', !!close && /^Close 1/.test(close.textContent) && !close.disabled, close ? close.textContent : 'none');
      check('Seen, with none waiting, is dimmed', !!seen && seen.disabled, seen ? seen.textContent : 'none');
      check('Archive takes all three, with its key', !!hotkey('Archive') && /^Archive⌃⌘A$/.test(hotkey('Archive').textContent), hotkey('Archive') ? hotkey('Archive').textContent : 'none');
      return menuOf(row(3));
    })
    .then(function (labels) {
      var want = ['Pin 3 sessions', 'Close 1 session', 'Mark 3 sessions as done', 'Archive 3 sessions', 'Copy session IDs'];
      check('a right-click on a picked row is the menu for all of them', want.every(function (l) { return labels.indexOf(l) >= 0; }) && labels.some(function (l) { return /^Snooze 3 sessions/.test(l); }), labels.join(' | '));
      check('and leaves the pick', selectedRows() === 'CDE', selectedRows());
      return menuOf(row(0));
    })
    .then(function (labels) {
      check('on another row it’s that one’s own menu', labels.indexOf('Mark as seen') >= 0 && labels.indexOf('Archive') >= 0, labels.join(' | '));
      check('and the pick holds', selectedRows() === 'CDE', selectedRows());
      keyDown({ key: 'a', code: 'KeyA', metaKey: true });
      return wait(100);
    })
    .then(function () {
      check('⌘A picks every session listed', selectedRows() === 'ABCDE' && head() === '5 sessions', selectedRows() + ' ' + head());
      check('Seen says it takes the two waiting', !!hotkey('Seen') && /^Seen 2/.test(hotkey('Seen').textContent), hotkey('Seen') ? hotkey('Seen').textContent : 'none');
      keyDown({ key: 'e', code: 'KeyE', metaKey: true });
      return wait(100);
    })
    .then(function () {
      var asked = sent('mark_seen');
      check('⌘E marks the waiting ones seen at once', asked.length === 1 && JSON.stringify(asked[0].keys) === JSON.stringify([picked[0].key, picked[1].key]), JSON.stringify(asked));
      check('and the pick stays', !!pickPanel() && head() === '5 sessions', head());
      keyDown({ key: 'Escape', code: 'Escape' });
      return wait(100);
    })
    .then(function () {
      check('Esc goes back to one', !pickPanel() && selectedRows().length === 1 && !!title(), selectedRows() + ' ' + title());
      // From the top: ⇧↓ twice picks the two below.
      press(0);
      return until(function () { return title() === 'Pick A' && document.activeElement && document.activeElement.closest('.ca-composer'); }, 3000, 'Pick A again, its box with the cursor');
    })
    .then(function () {
      // The click put the cursor in its box, where ⇧↓ selects text: Esc gives the list the keys.
      keyDown({ key: 'Escape', code: 'Escape' });
      keyDown({ key: 'ArrowDown', code: 'ArrowDown', shiftKey: true });
      // The next press reads the pick the first one made, as a key held down does.
      return wait(50).then(function () {
        keyDown({ key: 'ArrowDown', code: 'ArrowDown', shiftKey: true });
        return wait(100);
      });
    })
    .then(function () {
      var o = order();
      var run = o.slice(o.indexOf('A'), o.indexOf('A') + 3);
      check('⇧↓ picks the rows below', run.length === 3 && selectedRows() === sorted(run), selectedRows() + ' in ' + o);
      keyDown({ key: 'a', code: 'KeyA', metaKey: true, ctrlKey: true });
      return wait(150).then(function () { return { o: o, run: run }; });
    })
    .then(function (was) {
      var asked = sent('archive_session');
      var after = was.o.slice(was.o.indexOf(was.run) + 3)[0] || '';
      check('⌃⌘A archives all of them', asked.length === 3 && asked.every(function (a) { return a.archived === true; }), JSON.stringify(asked));
      check('they leave the list at once', was.run.split('').every(function (l) { return !listed('ABCDE'.indexOf(l)); }), order());
      check('the pick goes, and the row after them is selected', !pickPanel() && selectedRows() === after, selectedRows() + ', not ' + after);
      check('one toast says so, with Undo', /Archived 3 sessions\./.test(toast()) && /Undo/.test(toast()), toast());
      var undo = q('.ca-toast .ca-toast-action');
      if (undo) undo.click();
      return wait(150);
    })
    .then(function () {
      var back = sent('archive_session').filter(function (a) { return a.archived === false; });
      check('Undo puts all three back', back.length === 3, JSON.stringify(back));
      // The preview's board never shows them archived, so they stay as they were shown: Done
      // takes two of the others.
      var two = order().slice(0, 2);
      press('ABCDE'.indexOf(two[0]));
      return until(function () { return title() === 'Pick ' + two[0]; }, 3000, 'Pick ' + two[0] + '’s panel').then(function () { return two; });
    })
    .then(function (two) {
      press('ABCDE'.indexOf(two[1]), { metaKey: true });
      return until(pickPanel, 3000, 'the pick panel again').then(function () { return two; });
    })
    .then(function (two) {
      var done = hotkey('Done');
      check('Done takes both, with its key', !!done && /^Done⇧⌘E$/.test(done.textContent), done ? done.textContent : 'none');
      done.click();
      return until(function () { return sent('session_done').length >= 2; }, 3000, 'both asked').then(function () { return wait(150).then(function () { return two; }); });
    })
    .then(function (two) {
      var asked = sent('session_done').map(function (a) { return a.key; });
      var at = two.split('').map(function (l) { return 'ABCDE'.indexOf(l); });
      check('Done asks the engine about each, in the list’s order', JSON.stringify(asked) === JSON.stringify(at.map(function (i) { return picked[i].key; })), JSON.stringify(asked));
      check('they leave the list', !listed(at[0]) && !listed(at[1]), order());
      check('the toast says how many, with Undo', /Done with 2 sessions\./.test(toast()) && /Undo/.test(toast()), toast());
    })
    .then(done)
    .catch(stopped);
})();
