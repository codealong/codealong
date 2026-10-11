// Checks a PR's diffs and the ways out of them (PrFiles.tsx, prFiles.ts, prdiff.rs), in the PRs
// view's panel, with a worktree on this Mac holding the PR and the board saying Cursor opens
// diffs: a diff numbers its lines before and after; a line's number after opens the file there;
// long lines wrap or not, as you left them; side by side is offered only where the list is wide
// (the panel expanded to the window), where it draws the line before beside the line after, a side
// with no line there blank; a row's buttons open its diff in Cursor (a renamed file with its old
// path) and the file on GitHub; its right-click menu has those, the file at its first change and
// its path; and a file GitHub shows no diff of says so, with the ways to see it.
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/pr-diff-check.js
(function () {
  ['codealong.diffLayout', 'codealong.diffWrap', 'codealong.prPanelMax', 'codealong.prDetailsOpen'].forEach(function (k) { localStorage.removeItem(k); });
  var LONG = 'const total = ' + Array(30).join('someLongIdentifierName + ') + '1;';
  var FILES = [
    { path: 'src/teams/TeamPicker.tsx', status: 'modified', additions: 2, deletions: 1, patch: '@@ -10,3 +10,4 @@ export function TeamPicker() {\n   const a = 1;\n-  const b = 2;\n+  const b = 3;\n+  ' + LONG + '\n   return a;' },
    { path: 'src/teams/teamValues.ts', previous: 'src/teams/values.ts', status: 'renamed', additions: 1, deletions: 1, patch: '@@ -1 +1 @@\n-old\n+new' },
    { path: 'yarn.lock', status: 'modified', additions: 900, deletions: 800, patch: null },
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
      board.apps.diffEditor = { id: 'cursor', name: 'Cursor' };
      board.apps.editor = 'Cursor';
      board.apps.editorId = 'cursor';
      var lane = board.lanes.filter(function (l) { return !l.device && !l.hidden; })[0];
      // The first PR, updated now so it's listed first, held by the first worktree.
      board.prs = (board.prs || []).map(function (x, i) {
        if (i > 0) return x;
        pr = Object.assign({}, x, { state: 'open', updatedAt: Date.now(), changedFiles: FILES.length, lanes: lane ? [lane.id] : [], autoReview: undefined });
        return pr;
      });
      return board;
    });
  };
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var row = function (path) { return all('.ca-prfile').filter(function (r) { return r.querySelector('.ca-prfile-head').getAttribute('title').indexOf(path) >= 0; })[0]; };
  var nums = function (r) { return all('.ca-patch-num', r).map(function (n) { return n.textContent; }); };
  var lastAsk = function () { var a = last('open_pr_file'); return a && a.ask; };

  say('pr diff check: waiting for the PR');
  until(function () { return pr && all('.ca-prrow').filter(function (r) { return r.textContent.indexOf('#' + pr.number) >= 0; })[0]; }, 20000, 'the PR’s row')
    .then(function (prRow) {
      press(prRow);
      return until(function () { return all('.ca-prinsp .ca-tab').filter(function (t) { return /Changes/.test(t.textContent); })[0]; }, 5000, 'the Changes tab');
    })
    .then(function (tab) {
      tab.click();
      return until(function () { return row('TeamPicker.tsx'); }, 5000, 'the files');
    })
    .then(function (r) {
      press(r.querySelector('.ca-prfile-head'));
      return until(function () { return q('.ca-patch', r); }, 3000, 'its diff');
    })
    .then(function (patch) {
      var lines = all('.ca-patch-row', patch);
      check('a diff numbers its lines before and after', patch.classList.contains('is-unified') && nums(lines[1]).join() === '10,10' && nums(lines[2]).join() === '11,' && nums(lines[3]).join() === ',11' && nums(lines[5]).join() === '12,13', lines.map(function (l) { return nums(l).join('/'); }).join(' | '));
      check('its hunk’s header goes across it', lines[0].classList.contains('is-hunk') && lines[0].textContent.indexOf('@@ -10,3 +10,4 @@') === 0);
      var link = q('.ca-patch-row.is-add .ca-patch-num.is-link', patch);
      check('a line’s number after opens the file there', !!link && link.getAttribute('title') === 'Open in Cursor at line 11', link && link.getAttribute('title'));
      link.click();
      var ask = lastAsk();
      check('at that line, in the worktree that holds the PR', !!ask && ask.pr === pr.key && ask.path === 'src/teams/TeamPicker.tsx' && ask.how === 'file' && ask.line === 11, JSON.stringify(ask));
      check('a removed line’s number opens nothing', !q('.ca-patch-row.is-del .ca-patch-num.is-link', patch));
      check('long lines scroll sideways at first', patch.scrollWidth > patch.clientWidth + 1, patch.scrollWidth + ' in ' + patch.clientWidth);
      check('side by side isn’t offered in a narrow panel', !q('.ca-prfiles-view [aria-label="Split"]'));
      press(q('.ca-prfiles-view [aria-label="Wrap long lines"]'));
      return wait(100).then(function () { return patch; });
    })
    .then(function () {
      var patch = q('.ca-patch');
      check('Wrap wraps them, and is kept', patch.classList.contains('is-wrap') && patch.scrollWidth <= patch.clientWidth + 1 && localStorage.getItem('codealong.diffWrap') === '1', patch.className);
      key(window, { key: 'F', code: 'KeyF', metaKey: true, shiftKey: true });
      return until(function () { return q('.ca-prfiles-view [aria-label="Split"]'); }, 3000, 'Split, with the panel taking the window');
    })
    .then(function (split) {
      press(split);
      return until(function () { return q('.ca-patch.is-split'); }, 3000, 'the diff side by side');
    })
    .then(function (patch) {
      var change = all('.ca-patch-row.is-change', patch);
      var halves = change.map(function (r) { return all('.ca-patch-half', r).map(function (h) { return h.textContent.trim(); }).join(' ‖ '); });
      check('split draws the line before beside the line after', halves[0] === '11−  const b = 2; ‖ 11+  const b = 3;', halves.join(' / '));
      check('a side with no line there is blank', !!q('.ca-patch-none', change[1]) && change[1].querySelectorAll('.ca-patch-half')[1].textContent.indexOf('12+') === 0, change[1] && change[1].textContent);
      check('its choice is kept, and long lines always wrap there', localStorage.getItem('codealong.diffLayout') === 'split' && patch.classList.contains('is-wrap') && !q('.ca-prfiles-view [aria-label="Wrap long lines"]'));
      var renamed = row('teamValues.ts');
      var diff = q('.ca-prfile-act[aria-label="Open diff in Cursor"]', renamed);
      check('a row offers its diff in Cursor, with Cursor’s own icon', !!diff && !!q('img.ca-appicon, .ca-icon', diff), diff && diff.innerHTML.slice(0, 80));
      check('which says what’s set beside what', !!diff && /beside your copy in /.test(diff.getAttribute('title')), diff && diff.getAttribute('title'));
      diff.click();
      var ask = lastAsk();
      check('a renamed file’s diff asks with its old path', !!ask && ask.how === 'diff' && ask.path === 'src/teams/teamValues.ts' && ask.previous === 'src/teams/values.ts' && ask.status === 'renamed', JSON.stringify(ask));
      q('.ca-prfile-act[aria-label="Open on GitHub"]', renamed).click();
      return until(function () { return last('open_url'); }, 3000, 'the file on GitHub');
    })
    .then(function (sent) {
      check('and the file on GitHub, at its place on Files changed', /\/files#diff-[0-9a-f]{64}$/.test(sent.url) && sent.url.indexOf(pr.url) === 0, sent.url);
      return menuOf(row('TeamPicker.tsx').querySelector('.ca-prfile-bar'));
    })
    .then(function (labels) {
      check('its menu opens it every way, and copies its path', labels.join(' | ') === 'Open diff in Cursor | Open in Cursor at line 11 | Open on GitHub | - | Copy path' || labels.filter(function (l) { return l !== '-' && l !== 'Separator'; }).join(' | ') === 'Open diff in Cursor | Open in Cursor at line 11 | Open on GitHub | Copy path', labels.join(' | '));
      press(row('yarn.lock').querySelector('.ca-prfile-head'));
      return until(function () { return q('.ca-prfile-nodiff', row('yarn.lock')); }, 3000, 'the file without a diff');
    })
    .then(function (none) {
      var ways = all('.ca-linkbtn', none).map(function (b) { return b.textContent; });
      check('a file GitHub shows no diff of says so, with the ways to see it', /GitHub doesn’t show a diff/.test(none.textContent) && ways.join() === 'Open diff in Cursor,Open on GitHub', ways.join());
      key(window, { key: 'F', code: 'KeyF', metaKey: true, shiftKey: true });
      return wait(100);
    })
    .then(function () {
      check('the list comes back, and the diff goes back to one under the other', !q('.ca-win-body.is-panel-max') && !!q('.ca-patch.is-unified') && localStorage.getItem('codealong.diffLayout') === 'split');
    })
    .then(done, stopped);
})();
