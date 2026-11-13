// Checks where the quick panel starts a session: a repo, then a worktree in it (quick.ts
// `repoChoices`, `repoWorktrees`, `startLaneIn`; Quick.tsx). ⌘T's menu lists No project and
// Another folder…, then each repo once and no worktree, until what's typed names no repo, when it
// lists the worktrees it names, a pick of one starting there. A repo picked starts where new work
// goes in it: a reusable worktree ready for it, else its main checkout. ⌘B's menu lists that
// repo's worktrees, its main checkout first, then New worktree, the ones ready for new work and
// the pinned ones, and the rest behind one row that says how many, which opens them under Work
// under way and Other worktrees; New worktree, where the account makes one, and main each say so
// on the chip and on the start row, and a start with New worktree goes from the main checkout.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-repo-check.js
// The board needs a repo with its main checkout and a worktree that isn't reusable.
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var input = function () { return q('.ca-quick-input textarea'); };
  var chipFor = function (keys) { return function () { return all('.ca-quick-chip').filter(function (b) { return b.title.indexOf(keys) >= 0; })[0]; }; };
  var repoChip = chipFor('Repo (⌘T)');
  var treeChip = chipFor('Worktree (⌘B)');
  var said = function (chip) { var c = chip(); return c ? c.querySelector('span').textContent : null; };
  var filter = function () { return q('.ca-picker-filter'); };
  var options = function () { return all('.ca-picker-opt'); };
  var titles = function () { return options().map(function (o) { return o.querySelector('.ca-picker-title').textContent; }); };
  var named = function (title) { return options().filter(function (o) { return o.querySelector('.ca-picker-title').textContent === title; })[0]; };
  var more = function () { return options().filter(function (o) { return !!o.querySelector('.ca-picker-more'); })[0]; };
  var heads = function () { return all('.ca-picker-head').map(function (h) { return h.textContent; }); };
  var startTitle = function () { var r = all('.ca-quick-row-title').filter(function (t) { return /^Start /.test(t.textContent); }); return r.length ? r[r.length - 1].textContent : null; };
  var press = function (letter) { key(input(), { key: letter, code: 'Key' + letter.toUpperCase(), metaKey: true }); };
  var closed = function () { return until(function () { return !filter(); }, 3000, 'the menu closed'); };
  var opened = function () { return until(function () { return options().length > 0; }, 3000, 'a menu'); };
  var lanes = [];
  var repo = null;
  var pinned = [];

  say('quick repo check: waiting for the panel');
  until(function () { return !!input() && !!repoChip() && !repoChip().disabled && calls('quick_ready').length > 0; }, 20000, 'the repo chip')
    .then(function () { return window.__TAURI_INTERNALS__.invoke('get_board'); })
    .then(function (board) {
      pinned = board.pinnedWorktrees || [];
      lanes = board.lanes.filter(function (l) { return !l.startsChats && !l.hidden && !l.device; });
      var of = function (r) { return lanes.filter(function (l) { return l.project === r.project && l.repo === r.repo; }); };
      // A repo with its main checkout and a worktree that's neither reusable nor pinned.
      repo = lanes.filter(function (l) { return l.origin === 'main' && of(l).some(function (w) { return w.origin !== 'main' && !w.reuse && pinned.indexOf(w.id) < 0; }); })[0];
      if (!repo) throw new Error('no repo on the board with its main checkout and a worktree kept for one piece of work');
      repo = { project: repo.project, repo: repo.repo, lanes: of(repo) };
      check('no New worktree box under the panel', !q('.ca-quick-newtree'));
      press('t');
      return opened();
    })
    .then(function () {
      var listed = titles();
      check('No project and Another folder… first', listed[0] === 'No project' && listed[1] === 'Another folder…', listed.slice(0, 2).join(' | '));
      var repos = listed.slice(2);
      var once = repos.filter(function (t, i) { return repos.indexOf(t) === i; });
      check('then each repo once', heads().every(function (h) { return h !== 'Worktrees'; }) && repos.length > 0 && once.length === repos.length, repos.length + ' rows, ' + once.length + ' names');
      check('no worktree but a repo’s main checkout is listed', !listed.some(function (t) { return /\(main checkout\)$/.test(t); }), listed.join(' | '));
      // A worktree's name that names no repo.
      var buried = repo.lanes.filter(function (l) { return l.origin !== 'main' && !l.reuse && pinned.indexOf(l.id) < 0; });
      var hidden = buried.filter(function (l) { return !lanes.some(function (r) { return (r.repo + ' ' + r.project).toLowerCase().indexOf(l.callsign.toLowerCase()) >= 0; }); })[0];
      if (!hidden) {
        note('every worktree kept for one piece of work in ' + repo.repo + ' is named like a repo: finding one by name not checked');
        return null;
      }
      type(filter(), hidden.callsign);
      return wait(100).then(function () {
        check('a worktree’s name lists it under Worktrees', heads().indexOf('Worktrees') >= 0 && !!named(hidden.callsign), heads().join(' | ') + ': ' + titles().join(' | '));
        key(filter(), { key: 'Enter' });
        return closed();
      }).then(function () {
        check('and ⏎ starts there', said(repoChip) === hidden.repo && said(treeChip) === hidden.callsign, said(repoChip) + ' / ' + said(treeChip));
        return null;
      });
    })
    .then(function () {
      if (filter()) press('t');
      return closed().then(function () { press('t'); return opened(); });
    })
    .then(function () {
      type(filter(), repo.repo);
      return wait(100);
    })
    .then(function () {
      // Named after its project too, where the panel lists several.
      var row = options().filter(function (o) { var t = o.querySelector('.ca-picker-title').textContent; return t === repo.repo || t === repo.project + ' · ' + repo.repo; })[0];
      check('the repo is a row of its own', !!row, titles().join(' | '));
      row.click();
      return closed();
    })
    .then(function () {
      var ready = repo.lanes.filter(function (l) { return l.origin !== 'main' && (l.reuse === 'free' || l.reuse === 'landed'); }).map(function (l) { return l.callsign; });
      var at = said(treeChip);
      check('a repo picked starts where new work goes in it', said(repoChip) === repo.repo && (at === 'main' || ready.indexOf(at) >= 0), said(repoChip) + ' / ' + at + ' (ready: ' + ready.join(', ') + ')');
      press('b');
      return opened();
    })
    .then(function () {
      var listed = titles();
      check('⌘B lists the repo’s worktrees, main first, then New worktree', listed[0] === 'main' && listed[1] === 'New worktree', listed.join(' | '));
      var front = repo.lanes.filter(function (l) { return l.origin === 'main' || l.reuse === 'free' || l.reuse === 'landed' || pinned.indexOf(l.id) >= 0; }).length;
      var rest = repo.lanes.length - front;
      var row = more();
      check('the rest behind one row that says how many', !!row && row.textContent === rest + ' more' && listed.length === front + 2, (row && row.textContent) + ', ' + listed.length + ' rows, ' + front + ' in front');
      row.click();
      return wait(100).then(function () {
        check('which opens them under their headings', !more() && heads().every(function (h) { return h === 'Work under way' || h === 'Other worktrees'; }) && heads().length > 0, heads().join(' | '));
        var tree = named('New worktree');
        if (tree.disabled) {
          note('the account picked makes no worktree here (' + tree.title + '): New worktree not picked');
          return named('main');
        }
        tree.click();
        return closed().then(function () {
          check('New worktree says so on the chip', said(treeChip) === 'New worktree', said(treeChip));
          check('and on the start row', startTitle() === 'Start in a new worktree of ' + repo.repo, startTitle());
          type(input(), 'Tidy the release notes');
          key(input(), { key: 'Enter', metaKey: true });
          return until(function () { return sent('start_from_quick').length > 0; }, 5000, 'the start');
        }).then(function () {
          var spec = sent('start_from_quick').pop().spec;
          var main = repo.lanes.filter(function (l) { return l.origin === 'main'; })[0];
          check('which its harness makes from the main checkout, whichever worktree was picked', spec.worktree === true && spec.laneId === main.id, JSON.stringify({ laneId: spec.laneId, worktree: spec.worktree }));
          press('b');
          return opened().then(function () { return named('main'); });
        });
      });
    })
    .then(function (main) {
      main.click();
      return closed();
    })
    .then(function () {
      var callsign = repo.lanes.filter(function (l) { return l.origin === 'main'; })[0].callsign;
      check('main says so on the chip', said(treeChip) === 'main', said(treeChip));
      check('and the start row names the checkout', startTitle() === 'Start in ' + callsign + ' (main)', startTitle());
      done();
    })
    .catch(stopped);
})();
