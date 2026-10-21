// Checks Clean up worktree… (menus.ts `cleanUpItem`, ui/CleanUpDialog.tsx) on worktrees'
// headings in Sessions grouped by worktree, with two of this Mac's worktrees the board says can go
// and one it says stays: a right-click on one that can go offers it, with an ellipsis, beside
// Reuse for new work; one that stays has it dimmed, saying why. Picked, the dialog lists what
// would run (a dry run of `remove_worktree`, answered here) and removes nothing until asked;
// Remove sends the removal and the window's toast says what happened. Several picked offer it
// for the ones that can go.
//   scripts/preview.sh sessions '' scripts/preview/cleanup-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.setItem('codealong.sessionGroup', 'worktree');
  localStorage.removeItem('codealong.age');
  // The worktrees the check took: the first two can go, the third stays.
  var lanes = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd === 'remove_worktree') {
      var name = args.laneId.split('/').pop();
      var steps = ['git -C ~/r worktree remove ' + args.laneId, 'git -C ~/r branch -d ' + name];
      return p.then(function () {
        return args.dryRun ? { removed: true, message: name + ' can go: nothing in it would be lost.', steps: steps } : { removed: true, message: 'Removed the worktree ' + name + ' and its branch, ' + name + '.', steps: steps };
      });
    }
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      // Nothing pinned: a pinned session is listed with the pinned ones, not under its worktree.
      board.pinnedWorktrees = [];
      board.pinned = [];
      if (lanes.length === 0) {
        // With a session listed, so Sessions has its heading.
        var mine = board.lanes.filter(function (l) { return !l.hidden && !l.device && l.project !== 'Elsewhere' && l.origin !== 'main' && l.sessions.some(function (s) { return !s.archived && !s.visitor; }); });
        if (mine.length < 3) return board;
        lanes = mine.slice(0, 3).map(function (l) { return l.id; });
      }
      board.lanes = board.lanes.filter(function (l) { return lanes.indexOf(l.id) >= 0; });
      board.lanes.forEach(function (l) {
        l.cleanup = lanes.indexOf(l.id) < 2 ? { ok: true } : { ok: false, why: '2 commits on no remote' };
      });
      return board;
    });
  };

  // A worktree's heading: a row of its own, by its id.
  var row = function (i) { return document.getElementById('row-' + lanes[i]); };
  var item = function (label) { return lastMenu().filter(function (i) { return i.text === label; })[0]; };
  var dialog = function () { return q('.ca-dialog.ca-cleanup'); };
  var button = function (text) { return all('.ca-dialog.ca-cleanup button').filter(function (b) { return b.textContent.trim() === text; })[0]; };

  say('cleanup check: waiting for the board');
  until(function () { return lanes.length === 3 && row(0) && row(1) && row(2); }, 20000, 'three worktrees of this Mac’s outside a main checkout, with sessions')
    .then(function () { return menuOf(row(2)); })
    .then(function (labels) {
      var stays = item('Clean up worktree (2 commits on no remote)');
      check('one that stays has it dimmed, saying why', !!stays && stays.enabled === false, labels.join(' | '));
      return menuOf(row(0));
    })
    .then(function (labels) {
      var at = labels.indexOf('Clean up worktree…');
      check('one that can go offers it, after Reuse for new work', at > 0 && labels[at - 1] === 'Reuse for new work' && item('Clean up worktree…').enabled !== false, labels.join(' | '));
      menuItem('Clean up worktree…')();
      return until(function () { return dialog() && q('.ca-cleanup-step'); }, 3000, 'the dialog with what would run');
    })
    .then(function () {
      var name = lanes[0].split('/').pop();
      var steps = all('.ca-cleanup-step').map(function (s) { return s.textContent; });
      check('it lists what would run, from a dry run', steps.length === 2 && steps[0].indexOf('worktree remove') > 0 && JSON.stringify(sent('remove_worktree')) === JSON.stringify([{ laneId: lanes[0], dryRun: true }]), JSON.stringify(sent('remove_worktree')));
      check('it says it can’t be undone', /can’t be undone/.test(q('.ca-dialog-lede').textContent));
      check('and names the worktree', q('.ca-cleanup-name').textContent.indexOf(name) === 0, q('.ca-cleanup-name').textContent);
      button('Remove worktree').click();
      return until(function () { return !dialog(); }, 3000, 'the dialog to close');
    })
    .then(function () {
      var asked = sent('remove_worktree');
      check('Remove sends the removal', JSON.stringify(asked[asked.length - 1]) === JSON.stringify({ laneId: lanes[0], dryRun: false }), JSON.stringify(asked));
      return until(function () { var t = q('.ca-toast'); return t && /Removed the worktree/.test(t.textContent) ? t : null; }, 3000, 'the toast');
    })
    .then(function (toast) {
      note(toast.textContent);
      // With a heading selected, ⌘A picks every worktree listed: the menu takes only the ones
      // that can go.
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
      return wait(150).then(function () {
        (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, key: 'a', code: 'KeyA', metaKey: true }));
        return wait(150);
      });
    })
    .then(function () { return menuOf(row(1)); })
    .then(function (labels) {
      check('several picked offer it for the ones that can go', labels.indexOf('Clean up 2 worktrees…') >= 0, labels.join(' | '));
    })
    .then(done)
    .catch(stopped);
})();
