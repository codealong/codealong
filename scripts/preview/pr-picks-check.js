// Checks picking several PRs in the PRs view (picks.ts, PrPickPanel.tsx, menus.ts `prsActions`,
// mainKeys.ts): four open PRs of this Mac's, two of them asking for your review. ⌘-click picks
// another beside the selected one and ⇧-click a run; ⌘A picks every one, and the panel's Approve
// says it takes the two asking and approves them one after another, with one toast; a
// right-click on a picked row is the menu for all of them; P pins them, ⌘O opens each on
// GitHub, Esc goes back to one, and ⇧↑ ⇧↓ pick the row beside.
//   scripts/preview.sh prs '' scripts/preview/pr-picks-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'prs');
  // The four PRs, in the order the view lists them: two asking for your review, then yours, then
  // someone else's.
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.pinnedPrs = [];
      if (picked.length === 0) {
        var open = board.prs.filter(function (pr) { return pr.state !== 'merged' && pr.state !== 'closed'; }).slice(0, 4);
        open.forEach(function (pr, i) {
          pr.reviewRequested = i < 2;
          pr.snoozedUntil = null;
          pr.mine = i === 2;
          pr.updatedAt = Date.now() - i * 60e3;
          pr.title = 'Pick ' + 'ABCD'[i];
          picked.push(pr.key);
        });
      }
      board.prs = board.prs.filter(function (pr) { return picked.indexOf(pr.key) >= 0; });
      board.prs.forEach(function (pr) {
        var i = picked.indexOf(pr.key);
        pr.reviewRequested = i < 2;
        pr.mine = i === 2;
        pr.title = 'Pick ' + 'ABCD'[i];
      });
      return board;
    });
  };

  var row = function (i) { return document.getElementById('row-pr:' + picked[i]); };
  var selectedRows = function () {
    return picked.map(function (_, i) { return row(i) && row(i).getAttribute('aria-selected') === 'true' ? 'ABCD'[i] : ''; }).join('');
  };
  var order = function () {
    return all('.ca-prrow').map(function (el) {
      var i = picked.findIndex(function (k) { return el.id === 'row-pr:' + k; });
      return i < 0 ? '' : 'ABCD'[i];
    }).join('');
  };
  var press = function (i, init) { row(i).dispatchEvent(new MouseEvent('mousedown', Object.assign({ bubbles: true, cancelable: true, button: 0 }, init))); };
  var keyDown = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var pickPanel = function () { return q('.ca-pick'); };
  var head = function () { var h = q('.ca-pick-head'); return h ? h.textContent : ''; };
  var hotkey = function (label) {
    return all('.ca-pick .ca-hotkey').filter(function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };
  var toast = function () { var t = q('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };

  say('pr picks check: waiting for the list');
  until(function () { return picked.length === 4 && row(3); }, 20000, 'four open PRs listed')
    .then(function () { return wait(300); })
    .then(function () {
      check('the PRs are listed asking first, then yours, then others’', order() === 'ABCD', order());
      press(0);
      return wait(100);
    })
    .then(function () {
      press(2, { metaKey: true });
      return until(pickPanel, 3000, 'the pick panel');
    })
    .then(function () {
      check('⌘-click picks a PR beside the selected one', selectedRows() === 'AC' && head() === '2 PRs', selectedRows() + ' ' + head());
      press(3, { shiftKey: true });
      return wait(100);
    })
    .then(function () {
      check('⇧-click picks the run from the last one picked', selectedRows() === 'CD', selectedRows());
      check('Approve, with none asking, is dimmed', !!hotkey('Approve') && hotkey('Approve').disabled, hotkey('Approve') ? hotkey('Approve').textContent : 'none');
      keyDown({ key: 'a', code: 'KeyA', metaKey: true });
      return wait(100);
    })
    .then(function () {
      check('⌘A picks every PR listed', selectedRows() === 'ABCD' && head() === '4 PRs', selectedRows() + ' ' + head());
      var approve = hotkey('Approve');
      check('Approve says it takes the two asking', !!approve && approve.textContent === 'Approve 2' && !approve.disabled, approve ? approve.textContent : 'none');
      return menuOf(row(1));
    })
    .then(function (labels) {
      var want = ['Open 4 PRs on GitHub', 'Approve 2 PRs', 'Pin 4 PRs', 'Copy links', 'Copy branch names'];
      check('a right-click on a picked row is the menu for all of them', want.every(function (l) { return labels.indexOf(l) >= 0; }) && labels.some(function (l) { return /^Snooze 2 reviews/.test(l); }), labels.join(' | '));
      hotkey('Approve').click();
      return until(function () { return sent('review_pr').length >= 2; }, 3000, 'both approved').then(function () { return wait(100); });
    })
    .then(function () {
      var asked = sent('review_pr');
      var numbers = picked.slice(0, 2).map(function (k) { return Number(k.split('#')[1]); });
      check('Approve approves the two asking, in the list’s order', asked.length === 2 && asked.every(function (a) { return a.verdict === 'approve'; }) && asked[0].number === numbers[0] && asked[1].number === numbers[1], JSON.stringify(asked));
      check('one toast says so', /Approved 2 PRs\./.test(toast()), toast());
      keyDown({ key: 'p', code: 'KeyP' });
      return wait(100);
    })
    .then(function () {
      var pins = sent('set_pinned_prs');
      var last = pins[pins.length - 1];
      check('P pins all four', !!last && picked.every(function (k) { return last.keys.indexOf(k) >= 0; }), JSON.stringify(pins));
      check('and they stay picked', head() === '4 PRs', head());
      keyDown({ key: 'o', code: 'KeyO', metaKey: true });
      return wait(150);
    })
    .then(function () {
      var opened = sent('open_url');
      check('⌘O opens each on GitHub', opened.length === 4, JSON.stringify(opened));
      keyDown({ key: 'Escape', code: 'Escape' });
      return wait(100);
    })
    .then(function () {
      check('Esc goes back to one', !pickPanel() && selectedRows().length === 1, selectedRows());
      // From the last row ⇧↑ picks the one above; from any other, ⇧↓ the one below.
      var o = order();
      var at = o.indexOf(selectedRows());
      var up = at === o.length - 1;
      keyDown({ key: up ? 'ArrowUp' : 'ArrowDown', code: up ? 'ArrowUp' : 'ArrowDown', shiftKey: true });
      return wait(100).then(function () { return up ? o.slice(at - 1, at + 1) : o.slice(at, at + 2); });
    })
    .then(function (run) {
      check('⇧↑ ⇧↓ pick the row beside', run.length === 2 && selectedRows() === run.split('').sort().join(''), selectedRows() + ', not ' + run);
    })
    .then(done)
    .catch(stopped);
})();
