// Checks picking several worktrees on the Worktrees board, each whole (picks.ts,
// WorktreePickPanel.tsx, menus.ts `lanesActions`, mainKeys.ts): three worktrees of this Mac's, one
// with two agents whose turns ended, one with one, and one with none. ⌘-click on any line picks
// its worktree, every line of it, and ⌘-click on the worktree's own part leaves it out again;
// ⌘A picks every worktree; a right-click on a picked one is the menu for all of them; P pins
// them, A does nothing to them, E marks their sessions seen and they stay picked as their lines
// go; Esc goes back to one, and ⇧↑ ⇧↓ pick the worktree beside.
//   scripts/preview.sh worktrees '' scripts/preview/worktree-picks-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'worktrees');
  localStorage.removeItem('codealong.age');
  // The worktrees and sessions the check made: A and B in the first, C in the second, and a
  // third with no agent's line.
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
          s.archived = false;
          if (name) s.title = 'Pick ' + name;
        });
      });
      return board;
    });
  };

  var line = function (name) { return document.getElementById('row-' + lanes[name === 'C' ? 1 : 0] + '::' + keys[name]); };
  // A worktree's own part: its name, which on a row with lines selects (or with ⌘ picks) it.
  var nameOf = function (i) {
    return all('.ca-lane.is-worktree').map(function (row) { return q('.ca-seg-call', row); }).filter(function (cell) { return cell && cell.title.indexOf(lanes[i]) === 0; })[0];
  };
  var press = function (el, init) { el.dispatchEvent(new MouseEvent('mousedown', Object.assign({ bubbles: true, cancelable: true, button: 0 }, init))); };
  var keyDown = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var isSelected = function (el) { return !!el && el.getAttribute('aria-selected') === 'true'; };
  var selectedLines = function () { return ['A', 'B', 'C'].filter(function (n) { return isSelected(line(n)); }).join(''); };
  var pickPanel = function () { return q('.ca-pick'); };
  var head = function () { var h = q('.ca-pick-head'); return h ? h.textContent : ''; };
  var hotkey = function (label) {
    return all('.ca-pick .ca-hotkey').filter(function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };

  say('worktree picks check: waiting for the board');
  until(function () { return lanes.length === 3 && line('A') && line('B') && line('C') && nameOf(0); }, 20000, 'three worktrees: two lines in one, one in another, and one with none')
    .then(function () { return wait(300); })
    .then(function () {
      press(line('A'));
      return wait(100);
    })
    .then(function () {
      check('a click selects one line', selectedLines() === 'A', selectedLines());
      press(line('C'), { metaKey: true });
      return until(pickPanel, 3000, 'the pick panel');
    })
    .then(function () {
      check('⌘-click on a line picks its worktree beside the one selected, each whole', head() === '2 worktrees' && selectedLines() === 'ABC', head() + ' ' + selectedLines());
      press(nameOf(0), { metaKey: true });
      return wait(100);
    })
    .then(function () {
      check('⌘-click on a worktree’s own part leaves it out', !pickPanel() && selectedLines() === 'C', selectedLines());
      keyDown({ key: 'a', code: 'KeyA', metaKey: true });
      return wait(100);
    })
    .then(function () {
      check('⌘A picks every worktree', head() === '3 worktrees', head());
      var seen = hotkey('Seen');
      check('Seen marks their turns that ended', !!seen && !seen.disabled && /Mark 3 sessions as seen/.test(seen.title), seen ? seen.title : 'none');
      return menuOf(line('B'));
    })
    .then(function (labels) {
      var want = ['Mark 3 sessions as seen', 'Pin 3 worktrees', 'Copy paths', 'Copy branch names'];
      check('a right-click on a picked worktree is the menu for all of them', want.every(function (l) { return labels.indexOf(l) >= 0; }) && labels.indexOf('Archive') < 0, labels.join(' | '));
      keyDown({ key: 'p', code: 'KeyP' });
      keyDown({ key: 'a', code: 'KeyA' });
      return wait(100);
    })
    .then(function () {
      var pins = sent('set_pinned_worktrees');
      var last = pins[pins.length - 1];
      check('P pins all three', !!last && lanes.every(function (id) { return last.ids.indexOf(id) >= 0; }), JSON.stringify(pins));
      check('A does nothing to worktrees', sent('archive_session').length === 0, JSON.stringify(sent('archive_session')));
      keyDown({ key: 'e', code: 'KeyE' });
      return wait(150);
    })
    .then(function () {
      var seen = sent('mark_seen');
      var asked = seen.length === 1 ? seen[0].keys.slice().sort() : [];
      check('E marks their sessions seen', JSON.stringify(asked) === JSON.stringify([keys.A, keys.B, keys.C].sort()), JSON.stringify(seen));
      check('their lines go, and they stay picked', !line('A') && !line('C') && head() === '3 worktrees', head());
      keyDown({ key: 'Escape', code: 'Escape' });
      return wait(100);
    })
    .then(function () {
      check('Esc goes back to one', !pickPanel());
      keyDown({ key: 'ArrowDown', code: 'ArrowDown', shiftKey: true });
      return wait(100);
    })
    .then(function () {
      // From the last worktree ⇧↓ stays: then ⇧↑ picks the one above.
      if (!pickPanel()) {
        keyDown({ key: 'ArrowUp', code: 'ArrowUp', shiftKey: true });
        return wait(100);
      }
    })
    .then(function () {
      check('⇧↑ ⇧↓ pick the worktree beside', head() === '2 worktrees', head());
    })
    .then(done)
    .catch(stopped);
})();
