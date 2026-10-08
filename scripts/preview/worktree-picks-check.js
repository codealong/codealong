// Checks picking several lines on the Worktrees board (picks.ts, SessionPickPanel.tsx, menus.ts
// `boardActions`, mainKeys.ts): three worktrees of this Mac's, one with two agents waiting on you,
// one with one, and one with none, which is picked whole. ⌘-click picks a worktree beside a line;
// ⌘A picks every line, and the panel says how many sessions and worktrees, and what Close takes;
// a right-click on a picked line is the menu for all of them, with their worktrees' Pin, Reuse
// and copies; P pins their worktrees; Esc goes back to one; ⇧-click picks two lines of one
// worktree, A archives both with one Undo and the pick goes; and E marks a line seen with a
// worktree picked beside it.
//   scripts/preview.sh worktrees '' scripts/preview/worktree-picks-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'worktrees');
  localStorage.removeItem('codealong.age');
  // The lanes and sessions the check made, by name: A and B in one worktree, C in another, and a
  // worktree with no agent line.
  var lanes = [];
  var keys = {};
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.pinnedWorktrees = [];
      var own = function (l) { return l.sessions.filter(function (s) { return !s.visitor && !s.archived; }); };
      if (lanes.length === 0) {
        var mine = board.lanes.filter(function (l) { return !l.hidden && !l.device && l.project !== 'Elsewhere'; });
        var two = mine.find(function (l) { return own(l).length >= 2; });
        var rest = mine.filter(function (l) { return l !== two && own(l).length >= 1; });
        if (!two || rest.length < 2) return board;
        lanes = [two.id, rest[0].id, rest[1].id];
        keys = { A: own(two)[0].key, B: own(two)[1].key, C: own(rest[0])[0].key };
      }
      var now = Date.now();
      board.lanes = board.lanes.filter(function (l) { return lanes.indexOf(l.id) >= 0; });
      board.lanes.forEach(function (l) {
        var i = lanes.indexOf(l.id);
        l.bay = i === 2 ? 'idle' : 'unread';
        l.lastActivity = now - i * 60e3;
        l.sessions.forEach(function (s) {
          var name = Object.keys(keys).filter(function (k) { return keys[k] === s.key; })[0];
          s.status = name ? 'unread' : 'idle';
          s.statusSince = now - 600e3;
          s.lastActivity = now - (name ? 'ABC'.indexOf(name) : 9) * 60e3;
          s.snoozedUntil = undefined;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.surface = 'cli';
          s.live = name === 'A' || name === 'C';
          s.pid = s.live ? 4242 : null;
          s.attachCommand = null;
          if (name) s.title = 'Pick ' + name;
        });
      });
      return board;
    });
  };

  var line = function (name) { return document.getElementById('row-' + lanes[name === 'C' ? 1 : 0] + '::' + keys[name]); };
  var bare = function () { return document.getElementById('row-' + lanes[2]); };
  var press = function (el, init) { el.dispatchEvent(new MouseEvent('mousedown', Object.assign({ bubbles: true, cancelable: true, button: 0 }, init))); };
  var keyDown = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var isSelected = function (el) { return !!el && el.getAttribute('aria-selected') === 'true'; };
  var selectedNow = function () {
    return ['A', 'B', 'C'].filter(function (n) { return isSelected(line(n)); }).join('') + (isSelected(bare()) ? 'W' : '');
  };
  var pickPanel = function () { return q('.ca-pick'); };
  var head = function () { var h = q('.ca-pick-head'); return h ? h.textContent : ''; };
  var hotkey = function (label) {
    return all('.ca-pick .ca-hotkey').filter(function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };
  var toast = function () { var t = q('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };

  say('worktree picks check: waiting for the board');
  until(function () { return lanes.length === 3 && line('A') && line('B') && line('C') && bare(); }, 20000, 'three worktrees: two lines in one, one in another, and one with none')
    .then(function () { return wait(300); })
    .then(function () {
      press(line('A'));
      return wait(100);
    })
    .then(function () {
      press(bare(), { metaKey: true });
      return until(pickPanel, 3000, 'the pick panel');
    })
    .then(function () {
      check('⌘-click picks a worktree with no line beside a line', selectedNow() === 'AW', selectedNow());
      check('the panel says what’s picked', head() === '1 session and 1 worktree', head());
      keyDown({ key: 'a', code: 'KeyA', metaKey: true });
      return wait(100);
    })
    .then(function () {
      check('⌘A picks every line', selectedNow() === 'ABCW' && head() === '3 sessions and 1 worktree', selectedNow() + ' ' + head());
      var close = hotkey('Close');
      check('Close says it takes the two with a terminal', !!close && /^Close 2/.test(close.textContent), close ? close.textContent : 'none');
      check('Pin is there for their worktrees', !!hotkey('Pin') && !hotkey('Pin').disabled, hotkey('Pin') ? hotkey('Pin').textContent : 'none');
      return menuOf(line('B'));
    })
    .then(function (labels) {
      var want = ['Mark 3 sessions as seen', 'Close 2 sessions', 'Mark 3 sessions as done', 'Archive 3 sessions', 'Pin 3 worktrees', 'Copy session IDs', 'Copy paths'];
      check('a right-click on a picked line is the menu for all of them', want.every(function (l) { return labels.indexOf(l) >= 0; }), labels.join(' | '));
      keyDown({ key: 'p', code: 'KeyP' });
      return wait(100);
    })
    .then(function () {
      var pins = sent('set_pinned_worktrees');
      var last = pins[pins.length - 1];
      check('P pins their three worktrees', !!last && lanes.every(function (id) { return last.ids.indexOf(id) >= 0; }), JSON.stringify(pins));
      check('and they stay picked', head() === '3 sessions and 1 worktree', head());
      keyDown({ key: 'Escape', code: 'Escape' });
      return wait(100);
    })
    .then(function () {
      check('Esc goes back to one', !pickPanel() && selectedNow().length === 1, selectedNow());
      press(line('A'));
      return wait(50);
    })
    .then(function () {
      press(line('B'), { shiftKey: true });
      return wait(100);
    })
    .then(function () {
      check('⇧-click picks the run of lines', selectedNow() === 'AB', selectedNow());
      keyDown({ key: 'a', code: 'KeyA' });
      return wait(150);
    })
    .then(function () {
      var asked = sent('archive_session').map(function (a) { return a.key; });
      check('A archives both', asked.length === 2 && asked.indexOf(keys.A) >= 0 && asked.indexOf(keys.B) >= 0, JSON.stringify(asked));
      check('their lines leave, and the pick with them', !line('A') && !line('B') && !pickPanel());
      check('one toast says so, with Undo', /Archived 2 sessions\./.test(toast()) && /Undo/.test(toast()), toast());
      press(line('C'));
      return wait(50);
    })
    .then(function () {
      press(bare(), { metaKey: true });
      return wait(100);
    })
    .then(function () {
      check('a line and a worktree picked again', selectedNow() === 'CW', selectedNow());
      keyDown({ key: 'e', code: 'KeyE' });
      return wait(100);
    })
    .then(function () {
      var seen = sent('mark_seen');
      check('E marks the line’s session seen', seen.length === 1 && JSON.stringify(seen[0].keys) === JSON.stringify([keys.C]), JSON.stringify(seen));
    })
    .then(done)
    .catch(stopped);
})();
