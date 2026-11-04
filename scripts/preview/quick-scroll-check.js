// Checks that a search's list you've scrolled down stays where you scrolled it as the list sorts
// itself again (Quick.tsx, where the selected row is kept in view; Quick.css,
// `.ca-quick-scroll`): a session that works moves up among the sessions found, which changes the
// row that leads, and that row is selected but not scrolled to. A row you choose with the keys,
// and a list that starts afresh as what's typed changes, are still brought into view; and until
// you scroll, the selected row is kept in view whatever changes it, as when the row you'd chosen
// goes and the one that leads, out of view, takes its place.
//   scripts/preview.sh quick '' scripts/preview/quick-scroll-check.js
// The board needs a few sessions in worktrees that aren't hidden, and PRs.
(function () {
  // This runs before the app's script: some of the board's sessions and PRs get words nothing
  // else has, so a search for either lists them all, more than the list shows at once, and with
  // the rest of the sessions shown, much more. One word each: two would go to Jev, whose rows
  // would lead.
  localStorage.setItem('codealong.quick.project', 'All projects');
  var WORD = 'zebrafinch';
  var OTHER = 'marmoset';
  var HOUR = 3600 * 1000;
  var named = [];
  var bump = null;
  // A PR that no longer matches, once it's named.
  var gone = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var base = Math.max.apply(null, board.lanes.flatMap(function (l) { return l.sessions.map(function (s) { return s.lastActivity; }); }).concat([0]));
      board.lanes.forEach(function (l) {
        if (l.hidden) return;
        l.sessions.forEach(function (s) {
          if (s.visitor || s.archived) return;
          var i = named.indexOf(s.key);
          if (i < 0 && named.length < 12) i = named.push(s.key) - 1;
          if (i < 0) return;
          s.title = 'Teach the ' + WORD + ' and the ' + OTHER + ' song ' + (i + 1);
          // Older down the list, so the first found is the first named, until one works.
          s.lastActivity = bump === s.key ? base + HOUR : base - i * HOUR;
        });
      });
      (board.prs || []).slice(0, 4).forEach(function (pr, i) {
        if ('pr:' + pr.key !== gone) pr.title = 'The ' + WORD + ' and ' + OTHER + ' PR ' + (i + 1);
      });
      return board;
    });
  };

  var input = function () { return q('.ca-quick-input textarea'); };
  var scroller = function () { return q('.ca-quick-scroll'); };
  // The row selected among what the search found: one a list left over above them isn't it.
  var activeRow = function () { return q('.ca-quick-group[aria-label="Sessions"] .is-active, .ca-quick-group[aria-label="PRs"] .is-active'); };
  var active = function () { var a = activeRow(); return a ? a.getAttribute('data-row') : null; };
  var prs = function () { return all('.ca-quick-row[data-row^="pr:"]'); };
  var bottom = function (el) { return el.scrollHeight - el.clientHeight; };
  var seen = function (el) {
    var r = el.getBoundingClientRect();
    var box = scroller().getBoundingClientRect();
    return r.top >= box.top - 1 && r.bottom <= box.bottom + 1;
  };
  // You scroll the list to its bottom: the wheel's event, then where it takes the list (a
  // script's wheel event scrolls nothing).
  var scrollDown = function () {
    q('.ca-quick-list').dispatchEvent(new WheelEvent('wheel', { bubbles: true, deltaY: 400 }));
    scroller().scrollTop = bottom(scroller());
  };
  var push = function () { return window.__TAURI_INTERNALS__.invoke('get_board').then(function (b) { window.__emit('board', b); }); };
  // ↓ until the row `at` picks is selected.
  var downTo = function (at, left) {
    var row = at();
    if (!row || active() === row.getAttribute('data-row') || left === 0) return Promise.resolve();
    key(input(), { key: 'ArrowDown' });
    return wait(60).then(function () { return downTo(at, left - 1); });
  };
  var more = function () { return q('.ca-quick-row[data-row="more:found"]'); };
  var lastPr = function () { return prs()[prs().length - 1]; };

  say('quick scroll check: waiting for the list');
  until(function () { return calls('quick_ready').length > 0 && !!input(); }, 30000, 'the quick panel')
    .then(function () {
      check('the board has sessions and PRs to name', named.length >= 3, 'named ' + named.length);
      type(input(), WORD);
      return until(function () { return prs().length > 0; }, 5000, 'the PRs found');
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('the first session found leads', active() === named[0], JSON.stringify({ active: active(), named: named }));
      var el = scroller();
      check('the list scrolls', bottom(el) > 40, JSON.stringify({ scrollHeight: el.scrollHeight, clientHeight: el.clientHeight }));
      scrollDown();
      // A session works: it moves to the top of those found, and leads now.
      bump = named[named.length - 1];
      return push();
    })
    .then(function () { return until(function () { return active() === bump; }, 5000, 'the session that worked to lead'); })
    .then(function () { return wait(300); })
    .then(function () {
      var el = scroller();
      check('the list stays scrolled down as it sorts itself again', bottom(el) - el.scrollTop <= 2, JSON.stringify({ scrollTop: el.scrollTop, bottom: bottom(el), active: active() }));
      check('the last PR is still in view', prs().length > 0 && seen(prs()[prs().length - 1]), '');
      // ↓ chooses the next row, which is brought into view.
      key(input(), { key: 'ArrowDown' });
      return wait(200);
    })
    .then(function () {
      var row = activeRow();
      check('a row chosen with the keys is brought into view', !!row && row.getAttribute('data-row') !== bump && seen(row), JSON.stringify({ active: active(), scrollTop: scroller().scrollTop }));
      scrollDown();
      // What's typed changes: the list starts afresh, from the row that leads.
      type(input(), OTHER);
      return wait(400);
    })
    .then(function () {
      var row = activeRow();
      check('a fresh search brings the row that leads into view', !!row && seen(row), JSON.stringify({ active: active(), scrollTop: scroller().scrollTop, bottom: bottom(scroller()) }));
      // The rest of the sessions, so the list is a good deal taller than the panel.
      return downTo(more, 20).then(function () {
        key(input(), { key: 'Enter' });
        return until(function () { return !more(); }, 5000, 'the rest of the sessions');
      });
    })
    .then(function () { return downTo(lastPr, 40); })
    .then(function () {
      var last = lastPr();
      check('↓ reaches the last PR and keeps it in view', !!last && active() === last.getAttribute('data-row') && seen(last), JSON.stringify({ active: active(), scrollTop: scroller().scrollTop }));
      check('the list is taller than the panel by more than the PR', scroller().scrollTop > 2 * last.offsetHeight, JSON.stringify({ scrollTop: scroller().scrollTop, row: last.offsetHeight }));
      // That PR no longer matches: the row that leads, up out of view, is selected in its place.
      gone = active();
      return push();
    })
    .then(function () { return until(function () { return !q('[data-row="' + gone + '"]'); }, 5000, 'the PR to go'); })
    .then(function () { return wait(300); })
    .then(function () {
      var row = activeRow();
      check('with no scroll of yours, the row that leads in its place is brought into view', !!row && seen(row), JSON.stringify({ active: active(), scrollTop: scroller().scrollTop }));
      done();
    })
    .catch(stopped);
})();
