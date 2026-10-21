// Checks Group by in PRs and Todo (PrsView.tsx, TodoView.tsx; groups.ts, Groups.tsx): its three
// buttons in the bar over the list, remembered for each list. Grouped by worktree, every row is
// under a heading, a worktree's or the one over rows in no worktree, last in its section; a row
// leaves out the worktree its heading names; a worktree's heading isn't a row to select, and a
// press on it shows the worktree's panel in place of the one beside the list, with the way back.
// The arrows go from row to row, past the headings. Grouped by repo, each repo heads its PRs once
// among the open sections, and a PR leaves out its repo's chip. Merged & closed, and Done, stay
// folded until shown, grouped among themselves.
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/group-lists-check.js
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-sample.json scripts/preview.sh todo '' scripts/preview/group-lists-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var view = localStorage.getItem('codealong.view');
  var prs = view === 'prs';
  var KEY = prs ? 'codealong.prGroup' : 'codealong.todoGroup';
  var NONE = prs ? 'No worktree here' : 'No worktree';
  var ROW = prs ? '.ca-prrow' : '.ca-todorow';
  localStorage.removeItem(KEY);

  var button = function (label) { return q('.ca-listtools [role="radiogroup"][aria-label="Group by"] button[aria-label="' + label + '"]'); };
  var on = function (label) { var b = button(label); return !!b && b.getAttribute('aria-checked') === 'true'; };
  var list = function () { return q('.ca-board-list'); };
  var rows = function () { return all(ROW, list()); };
  var heads = function () { return all('.ca-grouphead', list()); };
  var press = function (el, init) { el.dispatchEvent(new MouseEvent('mousedown', Object.assign({ bubbles: true, cancelable: true, button: 0 }, init))); };
  var keyDown = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var selected = function () { return q('[role="option"][aria-selected="true"]', list()); };

  say('group lists check: waiting for ' + (prs ? 'PRs' : 'Todo'));
  // Each row's status as its box says it, and the folded section's count, before grouping.
  var glyphs = function () { var m = {}; rows().forEach(function (r) { var g = q('.ca-todoglyph, .ca-lane-pricon, .ca-seg-glyph > *', r); m[r.id] = g ? g.outerHTML : ''; }); return m; };
  var foldCount = function () { var b = all('.ca-bayblock', list()).filter(function (x) { return /^(Merged & closed|Done)/i.test((q('.ca-bay', x) || {}).textContent || ''); })[0]; return b ? (q('.ca-bay-count', b) || {}).textContent : null; };
  var before = null;
  var countBefore = null;
  until(function () { return button('No groups') && rows().length > 1; }, 20000, 'the list, with Group by in its bar')
    .then(function () {
      before = glyphs();
      countBefore = foldCount();
      var labels = all('.ca-listtools [role="radiogroup"][aria-label="Group by"] button').map(function (b) { return b.getAttribute('aria-label'); });
      check('Group by has its three buttons', labels.join('|') === 'No groups|Worktree|Repo', labels.join('|'));
      check('the list starts with no groups', on('No groups') && heads().length === 0);
      button('Worktree').click();
      return until(function () { return heads().length > 0; }, 3000, 'the worktrees’ headings');
    })
    .then(function () {
      check('Worktree groups the list, and this list remembers it', on('Worktree') && localStorage.getItem(KEY) === 'worktree' && localStorage.getItem(prs ? 'codealong.todoGroup' : 'codealong.prGroup') === null, localStorage.getItem(KEY));
      var after = glyphs();
      var changed = Object.keys(after).filter(function (id) { return id in before && before[id] !== after[id]; });
      check('a row’s box says its own status, grouped as not', changed.length === 0, changed.slice(0, 3).join(' | '));
      check('the folded section counts all its rows, grouped as not', foldCount() === countBefore, foldCount() + ' was ' + countBefore);
      var loose = rows().filter(function (r) { return !r.closest('.ca-group') && !r.closest('.ca-pinned:not(:has(.ca-group))'); });
      check('every row is under a heading', loose.length === 0, loose.length + ' loose');
      var wt = heads().filter(function (h) { return h.classList.contains('is-worktree-head'); });
      check('a worktree’s heading is a heading, not a row to select', wt.length > 0 && wt.every(function (h) { return h.getAttribute('role') === 'heading' && !h.hasAttribute('aria-selected'); }), wt.length + ' worktree headings');
      var named = [];
      wt.forEach(function (h) {
        var name = q('.ca-grouphead-name', h).textContent;
        all(ROW, h.closest('.ca-group')).forEach(function (r) {
          all('.ca-wtchip-name', r).forEach(function (c) { if (c.textContent === name) named.push(r.id); });
        });
      });
      check('its rows leave out the worktree it names', named.length === 0, named.slice(0, 3).join(' | '));
      // The rows in no worktree: a heading of their own, last in each section it's in.
      var none = heads().filter(function (h) { return h.classList.contains('is-none-head'); });
      var last = none.every(function (h) { var g = h.closest('.ca-group'); return !g.nextElementSibling || !g.nextElementSibling.classList.contains('ca-group'); });
      if (none.length === 0) note('every row is in a worktree, so the heading over the rest went unchecked');
      else check('the rows in no worktree are under “' + NONE + '”, last in their section', none.every(function (h) { return h.textContent.trim() === NONE; }) && last);
      // The arrows go from row to row, past the headings.
      press(rows()[0]);
      return wait(200).then(function () {
        list().focus();
        var seen = [];
        for (var i = 0; i < Math.min(6, rows().length - 1); i++) {
          keyDown({ key: 'ArrowDown', code: 'ArrowDown' });
          seen.push(selected());
        }
        return wait(100).then(function () { return seen; });
      });
    })
    .then(function () {
      var s = selected();
      check('the arrows go from row to row, past the headings', !!s && s.matches(ROW), s ? s.className : 'nothing selected');
      // A press on a worktree's heading: the worktree's panel in place of the row's.
      var h = heads().filter(function (x) { return x.classList.contains('is-worktree-head'); })[0];
      var name = q('.ca-grouphead-name', h).textContent;
      var before = s.id;
      press(h);
      // The worktree's panel (Inspector.tsx) is the one with the way back to the row's.
      return until(function () { var b = q('.ca-insp .ca-panelback'); return b && b.closest('.ca-insp'); }, 3000, 'the worktree’s panel').then(function (p) {
        check('a press on a worktree’s heading shows its panel in the row’s place, with the way back', !p.classList.contains('ca-prpanel') && !p.classList.contains('ca-sessinsp') && selected() && selected().id === before && / details$/.test(p.getAttribute('aria-label') || ''), (p.getAttribute('aria-label') || '') + ' for ' + name);
        q('.ca-panelback', p).click();
        return wait(150);
      });
    })
    .then(function () {
      if (!prs) return null;
      button('Repo').click();
      return until(function () { return heads().some(function (h) { return h.classList.contains('is-repo-head'); }); }, 3000, 'the repos’ headings').then(function () {
        check('Repo groups the PRs by repo, and is remembered', on('Repo') && localStorage.getItem(KEY) === 'repo');
        var open = all('.ca-bayblock', list()).filter(function (b) { return !b.classList.contains('ca-pinned') && !/Merged/.test(b.textContent.slice(0, 40)); });
        var names = open.flatMap(function (b) { return all('.ca-grouphead.is-repo-head .ca-grouphead-name', b).map(function (n) { return n.textContent; }); });
        var once = names.filter(function (n, i) { return names.indexOf(n) === i; });
        check('each repo heads its PRs once among the open sections', once.length === names.length && names.length > 0, names.join(', '));
        var chipped = rows().filter(function (r) { return r.closest('.ca-group') && q('.ca-grouphead.is-repo-head', r.closest('.ca-group')) && q('.ca-repochip', r); });
        check('a PR under its repo leaves out its repo’s chip', chipped.length === 0, chipped.length + ' with a repo chip');
      });
    })
    .then(function () {
      // The folded section: hidden until shown, grouped among itself.
      var fold = all('.ca-bayblock', list()).filter(function (b) { return /^(Merged & closed|Done)/i.test((q('.ca-bay', b) || {}).textContent || ''); })[0];
      if (!fold) return void note('nothing merged, closed or done, so the folded section went unchecked');
      check('the folded section lists nothing until shown', all(ROW, fold).length === 0);
      var show = all('button', fold).filter(function (b) { return b.textContent === 'Show'; })[0];
      show.click();
      return wait(200).then(function () {
        var shown = all(ROW, fold);
        check('shown, its rows are under its own headings', shown.length > 0 && shown.every(function (r) { return !!r.closest('.ca-group') && r.closest('.ca-bayblock') === fold; }), shown.length + ' rows');
      });
    })
    .then(function () {
      button('No groups').click();
      return until(function () { return heads().length === 0; }, 3000, 'the list without groups');
    })
    .then(function () {
      check('No groups lists the rows alone again, and is remembered', on('No groups') && localStorage.getItem(KEY) === 'none');
    })
    .then(done)
    .catch(stopped);
})();
