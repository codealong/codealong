// Checks the filter over a PR's changed files (PrFiles.tsx, prFiles.ts), on its card in the
// waiting window: each row says the file's name, then its folder; the file count under Checks and
// reviews goes to the filter; words narrow the list to the paths that hold them all, marked, and
// `-word` leaves some out; the files whose changes mention the words come after, under their own
// heading with how many lines do; ↑ ↓ pick a file and ⏎ opens it, the words marked in its diff;
// Open all opens what the filter lists; Escape clears the filter, then leaves it; ⌘F comes back to
// it, and in the main window's PRs ⌘F from the list opens the list's find (ListBar.tsx) instead
// (findKeys.ts); and a filter nothing matches says so, with a way to clear it.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/pr-files-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/pr-files-check.js
(function () {
  var file = function (path, patch, status) { return { path: path, status: status || 'modified', additions: 3, deletions: 1, patch: patch === undefined ? '@@ -1,2 +1,3 @@\n context\n-old\n+new' : patch }; };
  var FILES = [
    file('apps/app/src/teams/TeamPicker.tsx', '@@ -1,2 +1,3 @@\n import x\n+const p = createProject(team);\n-old', 'added'),
    file('apps/app/src/teams/TeamPicker.spec.tsx', undefined, 'added'),
    file('apps/app/src/teams/teamValues.ts'),
    file('apps/app/src/console/Console.tsx'),
    file('apps/app/src/console/nav/NavSidebar.tsx'),
    file('apps/server/src/project/createProject.ts'),
    file('apps/server/src/project/members.ts'),
    file('apps/server/src/team/TeamService.ts'),
    file('apps/server/src/team/TeamService.spec.ts'),
    file('apps/server/migrations/0042_teams.sql'),
    file('docs/teams.md'),
    file('yarn.lock', null),
  ];
  var pr = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'pr_detail') {
      window.__calls.push([cmd, args]);
      return Promise.resolve({ bodyHtml: '<p>Adds teams.</p>', mergeable: null, commits: 2, reviews: [], requested: [], comments: [], files: FILES, moreFiles: false });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
        });
      });
      // The first PR asks for your review, with these files, and no other does.
      board.prs = (board.prs || []).map(function (x) {
        if (pr && x.key !== pr.key) return Object.assign({}, x, { reviewRequested: false });
        pr = Object.assign({}, x, { state: 'open', mine: false, author: 'ana', reviewRequested: true, requestedBy: 'ana', requestedAt: Date.now() - 3600e3, snoozedUntil: null, mergedAt: null, closedAt: null, changedFiles: FILES.length });
        return pr;
      });
      return board;
    });
  };

  var input = function () { return q('.ca-prfiles-filter input'); };
  var heading = function () { return q('.ca-prfiles-top .ca-label').textContent; };
  var names = function (sel) { return all((sel || '') + ' .ca-prfile-name').map(function (n) { return n.textContent; }); };
  var groups = function () { return all('.ca-prfiles'); };
  var filter = function (text) { type(input(), text); return wait(100); };
  var press = function (k, more) { key(input(), Object.assign({ key: k }, more || {})); return wait(100); };

  say('pr files check: waiting for the files');
  until(function () { return !!pr && !!input(); }, 20000, 'the files')
    .then(function () {
      check('every file is listed, the count in the heading', names().length === 12 && heading() === 'Files changed · 12', heading());
      var first = q('.ca-prfile');
      check('a row says the file’s name, then its folder', first.querySelector('.ca-prfile-name').textContent === 'TeamPicker.tsx' && first.querySelector('.ca-prfile-dir').textContent === 'apps/app/src/teams', first.textContent);
      q('.ca-prinsp-files').click();
      return wait(100);
    })
    .then(function () {
      check('the file count goes to the filter', document.activeElement === input());
      return filter('team -spec');
    })
    .then(function () {
      check('words narrow it to the paths that hold them, and -word leaves some out', JSON.stringify(names()) === JSON.stringify(['TeamPicker.tsx', 'teamValues.ts', 'TeamService.ts', '0042_teams.sql', 'teams.md']), JSON.stringify(names()));
      check('the heading says how many of how many', heading() === 'Files changed · 5 of 12', heading());
      check('the words are marked in the paths', all('.ca-prfile-path mark').length >= 5 && q('.ca-prfile-name mark').textContent === 'Team');
      return filter('createproject');
    })
    .then(function () {
      check('a path that holds it is listed first', JSON.stringify(all('.ca-prfile-name', groups()[0]).map(function (n) { return n.textContent; })) === JSON.stringify(['createProject.ts']), JSON.stringify(names()));
      var sub = q('.ca-prfiles-sub');
      check('then the files whose changes mention it, under their own heading', !!sub && sub.textContent === 'Mentioned in the changes' && groups().length === 2 && JSON.stringify(names('.ca-prfiles-sub + .ca-prfiles')) === JSON.stringify(['TeamPicker.tsx']), JSON.stringify(names()));
      check('with how many lines do', (q('.ca-prfile-mentions') || {}).textContent === '1 line');
      return press('ArrowDown');
    })
    .then(function () {
      check('↓ picks the next file', all('.ca-prfile.is-active .ca-prfile-name').map(function (n) { return n.textContent; }).join() === 'TeamPicker.tsx');
      return press('Enter');
    })
    .then(function () {
      var open = q('.ca-prfile.is-open');
      check('⏎ opens it, the words marked in its diff', !!open && open.querySelector('.ca-prfile-name').textContent === 'TeamPicker.tsx' && (open.querySelector('.ca-patch mark') || {}).textContent === 'createProject');
      var all_ = all('.ca-prfiles-top .ca-linkbtn')[0];
      check('Open all is offered while it filters', !!all_ && all_.textContent === 'Open all', all_ && all_.textContent);
      all_.click();
      return wait(100);
    })
    .then(function () {
      check('Open all opens what it lists, and turns to Close all', all('.ca-prfile.is-open').length === 2 && q('.ca-prfiles-top .ca-linkbtn').textContent === 'Close all');
      return press('Escape');
    })
    .then(function () {
      check('Escape clears the filter', input().value === '' && names().length === 12 && document.activeElement === input());
      return press('Escape');
    })
    .then(function () {
      check('and then leaves it', document.activeElement !== input());
      key(window, { key: 'f', metaKey: true });
      return wait(100);
    })
    .then(function () {
      check('⌘F comes back to it', document.activeElement === input());
      var list = q('.ca-board-list');
      if (!list) return;
      list.focus();
      key(list, { key: 'f', metaKey: true });
      return wait(100).then(function () {
        check('⌘F from the list opens the list’s find', !!q('.ca-listfind input') && document.activeElement === q('.ca-listfind input'), document.activeElement && document.activeElement.className);
      });
    })
    .then(function () {
      return filter('zzz');
    })
    .then(function () {
      var empty = q('.ca-prfiles-sec .ca-empty');
      check('a filter nothing matches says so', !!empty && empty.textContent.indexOf('No file’s path or changes hold that.') === 0 && names().length === 0, empty && empty.textContent);
      empty.querySelector('.ca-linkbtn').click();
      return wait(100);
    })
    .then(function () {
      check('with a way to clear it', input().value === '' && names().length === 12);
    })
    .then(done, stopped);
})();
