// Checks the quick panel's worktree menu folding away the worktrees seldom started in (quick.ts
// `laneGroups`, Picker.tsx `folded`): every main checkout and reusable worktree comes first, the
// rest behind one row at the end that says how many; a click on it opens them under their repos,
// on the first of them, as ⏎ on it does; and what's typed looks through them unopened.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-worktree-fold-check.js
// The board needs a repo with its main checkout and a worktree that isn't reusable.
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var chip = function () { return all('.ca-quick-chip').filter(function (b) { return /Worktree \(⌘T\)/.test(b.title); })[0]; };
  var filter = function () { return q('.ca-picker-filter'); };
  var options = function () { return all('.ca-picker-opt'); };
  var more = function () { return options().filter(function (o) { return !!o.querySelector('.ca-picker-more'); })[0]; };
  var heads = function () { return all('.ca-picker-head').map(function (h) { return h.textContent; }); };
  var active = function () { return q('.ca-picker-opt.is-on'); };
  var open = function () {
    if (filter()) chip().click();
    return until(function () { return !filter(); }, 3000, 'the menu closed').then(function () {
      chip().click();
      return until(function () { return options().length > 0; }, 3000, 'the worktree menu');
    });
  };
  var front = 0;
  // No project and Another folder…, ahead of every worktree.
  var lead = 2;
  var buried = [];

  say('quick worktree fold check: waiting for the panel');
  until(function () { return !!input() && !!chip() && !chip().disabled; }, 20000, 'the worktree chip')
    .then(function () { return window.__TAURI_INTERNALS__.invoke('get_board'); })
    .then(function (board) {
      var lanes = board.lanes.filter(function (l) { return !l.startsChats && !l.hidden; });
      var repo = function (l) { return l.project + '\n' + l.repo; };
      var withMain = {};
      lanes.forEach(function (l) { if (l.origin === 'main') withMain[repo(l)] = true; });
      lanes.forEach(function (l) {
        if (withMain[repo(l)] && l.origin !== 'main' && !l.reuse && (board.pinnedWorktrees || []).indexOf(l.id) < 0) buried.push(l);
        else front += 1;
      });
      if (buried.length === 0) throw new Error('no worktree on the board to fold away');
      return open();
    })
    .then(function () {
      var opts = options();
      var row = more();
      check('the rest are behind one row', !!row, opts.map(function (o) { return o.textContent; }).slice(-3).join(' | '));
      check('at the end', !!row && opts[opts.length - 1] === row);
      check('that says how many', !!row && row.textContent === buried.length + ' more', row && row.textContent);
      // No project and Another folder…, then every main checkout and reusable worktree.
      check('every main checkout and reusable worktree comes before it', opts.length === lead + front + 1, (opts.length - lead - 1) + ' listed, ' + front + ' expected');
      check('none of the rest is listed', heads().every(function (h) { return !/other worktrees/.test(h); }), heads().join(' | '));
      row.click();
      return wait(100);
    })
    .then(function () {
      check('a click opens them', !more() && options().length === lead + front + buried.length, options().length + ' options');
      check('under their repos', heads().some(function (h) { return /· other worktrees$/.test(h); }), heads().join(' | '));
      var first = options()[lead + front];
      check('on the first of them', !!first && active() === first, active() && active().textContent);
      return open();
    })
    .then(function () {
      check('closed and opened again, they fold away again', !!more());
      // One press at a time: each moves from where the last one left the highlight.
      var down = function (n) { return n === 0 ? null : (key(filter(), { key: 'ArrowDown' }), wait(20).then(function () { return down(n - 1); })); };
      return down(options().length);
    })
    .then(function () {
      check('↓ reaches the row', active() === more(), active() && active().textContent);
      key(filter(), { key: 'Enter' });
      return wait(100);
    })
    .then(function () {
      check('⏎ on it opens them and picks nothing', !!filter() && !more() && options().length === lead + front + buried.length, options().length + ' options');
      return open();
    })
    .then(function () {
      var name = buried[0].callsign;
      type(filter(), name);
      return wait(100).then(function () { return name; });
    })
    .then(function (name) {
      var found = options().filter(function (o) { return o.querySelector('.ca-picker-title').textContent === name; });
      check('what’s typed finds one of them unopened', found.length > 0, name + ': ' + options().map(function (o) { return o.textContent; }).join(' | '));
      check('with no row to open them', !more());
      done();
    })
    .catch(stopped);
})();
