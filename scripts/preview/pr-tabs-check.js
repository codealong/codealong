// Checks the PRs view's tabs (PrsView.tsx `PR_TABS`, `prSections`): All, Yours and Others over the
// list, All shown first, each counting the PRs in it that need you; All leads with Review
// requested, then Ready to merge, then Agent reviewing; Yours lists only yours, under Ready to
// merge, Needs work, Waiting on others and Drafts; Others only everyone else's, under Review
// requested, Agent reviewing, Snoozed and Not asking you; the sidebar's Authors count only the tab's
// PRs; the tab picked is remembered; a PR shown from elsewhere that the tab doesn't list switches
// to its own tab, once: a tab picked after it stays through a visit to Sessions. In each project the three
// latest open PRs are made a review asked of you, a PR of yours ready to merge, and a review
// Codealong's agent is still on, since a snapshot seldom has all three.
//   scripts/preview.sh prs '' scripts/preview/pr-tabs-check.js
(function () {
  var made = { asked: [], ready: [], held: [] };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var open = board.prs.filter(function (pr) { return pr.state !== 'merged' && pr.state !== 'closed'; });
      // The PR made yours is by you, as your own are, so the sidebar's Authors call it you.
      var me = (board.prs.filter(function (pr) { return pr.mine && pr.author; })[0] || {}).author;
      board.projects.forEach(function (project) {
        var three = open.filter(function (pr) { return pr.project === project.name; }).slice(0, 3);
        var base = { snoozedUntil: null, autoReview: undefined, turn: undefined, mergedAt: null, closedAt: null };
        if (three[0]) {
          Object.assign(three[0], base, { state: 'open', mine: false, reviewRequested: true });
          made.asked.push(three[0]);
        }
        if (three[1]) {
          Object.assign(three[1], base, { state: 'open', mine: true, author: me || three[1].author, reviewRequested: false, merge: 'clean', checks: { passed: 2, failed: 0, pending: 0, skipped: 0, failing: [] } });
          made.ready.push(three[1]);
        }
        if (three[2]) {
          Object.assign(three[2], base, { state: 'open', mine: false, reviewRequested: true, autoReview: { stage: 'reviewing', head: three[2].headOid || 'abc1234', at: Date.now(), rounds: 1, holds: true } });
          made.held.push(three[2]);
        }
      });
      board.pinnedPrs = [];
      return board;
    });
  };

  var tabs = function () { return all('.ca-listtabs .ca-tab'); };
  var tab = function (label) { return tabs().filter(function (t) { return t.firstChild && t.firstChild.textContent === label; })[0]; };
  var countOf = function (t) { var c = q('.ca-tab-count', t); return c ? Number(c.textContent) : 0; };
  var headings = function () { return all('.ca-board-list .ca-bayblock').map(function (b) { var h = q('.ca-bay > span', b); return h ? h.textContent : null; }); };
  var rowsUnder = function (label) {
    var b = all('.ca-board-list .ca-bayblock').filter(function (x) { var h = q('.ca-bay > span', x); return h && h.textContent === label; })[0];
    return b ? all('.ca-prrow', b) : [];
  };
  var num = function (row) { return (q('.ca-prrow-num', row) || {}).textContent; };
  // A PR's row by its number and title: two repos of a project can each have a #12.
  var isRow = function (pr) { return function (r) { return num(r) === '#' + pr.number && (q('.ca-lane-headline', r) || {}).textContent === pr.title; }; };
  var under = function (label, pr) { return rowsUnder(label).some(isRow(pr)); };
  var listed = function (pr) { return all('.ca-board-list .ca-prrow').some(isRow(pr)); };
  var numbers = function () { return all('.ca-board-list .ca-prrow').map(num).join(' '); };
  var nav = function (label) { return all('.ca-side .ca-nav').filter(function (b) { return b.textContent.indexOf(label) === 0; })[0]; };
  /** The sidebar's Authors, each as its name and count run together (`you3`). */
  var authors = function () {
    var head = all('.ca-side .ca-side-label').filter(function (x) { return (q('span', x) || {}).textContent === 'Authors'; })[0];
    var chips = head && head.nextElementSibling;
    return chips ? Array.prototype.filter.call(chips.children, function (c) { return !c.classList.contains('ca-side-more'); }).map(function (c) { return c.textContent.trim(); }) : [];
  };
  var only = function (allowed) { return headings().every(function (h) { return allowed.indexOf(h) >= 0; }); };
  /** The made PRs of the project shown. */
  var shown = function (list) { return list.filter(listedAnywhere)[0]; };
  var seen = {};
  var listedAnywhere = function (pr) { return !!seen[pr.key]; };

  say('pr tabs check: waiting for the tabs');
  until(function () { return tabs().length === 3 && q('.ca-prrow'); }, 20000, 'the PRs view with its tabs')
    .then(function () {
      check('the tabs read All, Yours, Others, All shown', tabs().map(function (t) { return t.firstChild.textContent; }).join('|') === 'All|Yours|Others' && tab('All').getAttribute('aria-selected') === 'true', tabs().map(function (t) { return t.textContent + '=' + t.getAttribute('aria-selected'); }).join(', '));
      [].concat(made.asked, made.ready, made.held).forEach(function (pr) { if (listed(pr)) seen[pr.key] = true; });
      var asked = shown(made.asked), ready = shown(made.ready), held = shown(made.held);
      if (!asked || !ready || !held) throw new Error('the project shown has fewer than three open PRs');
      var heads = headings();
      check('All leads with Review requested, then Ready to merge', heads[0] === 'Review requested' && heads.indexOf('Ready to merge') > 0, heads.join(', '));
      check('a review asked of you is under Review requested, a PR of yours to merge under Ready to merge', under('Review requested', asked) && under('Ready to merge', ready), heads.join(', '));
      check('a review the agent is still on waits under Agent reviewing, after them', under('Agent reviewing', held) && heads.indexOf('Agent reviewing') > heads.indexOf('Ready to merge'), heads.join(', '));
      var needing = rowsUnder('Review requested').length + rowsUnder('Ready to merge').length + rowsUnder('Needs work').length;
      check('All counts what needs you', countOf(tab('All')) === needing, countOf(tab('All')) + ' for ' + needing);
      tab('Yours').click();
      return until(function () { return tab('Yours').getAttribute('aria-selected') === 'true'; }, 5000, 'Yours picked').then(function () { return wait(200); }).then(function () {
        check('Yours lists none of everyone else’s', !listed(asked) && !listed(held), '#' + asked.number + ', #' + held.number + ' among ' + numbers());
        check('Yours lists yours under its own headings', only(['Ready to merge', 'Needs work', 'Waiting on others', 'Drafts', 'Merged & closed']), headings().join(', '));
        check('with your PR under Ready to merge', under('Ready to merge', ready), headings().join(', '));
        check('Yours counts its own', countOf(tab('Yours')) === rowsUnder('Ready to merge').length + rowsUnder('Needs work').length, String(countOf(tab('Yours'))));
        check('the list is the tab’s panel', q('.ca-board-list').id === 'ca-prtabs-mine' && tab('Yours').getAttribute('aria-controls') === 'ca-prtabs-mine', q('.ca-board-list').id);
        var who = authors();
        if (who.length === 0) note('no Authors in the sidebar to count');
        else check('the sidebar’s Authors count only yours', who.every(function (a) { return /^you\d*$/.test(a); }), who.join(', '));
        tab('Others').click();
        return until(function () { return tab('Others').getAttribute('aria-selected') === 'true'; }, 5000, 'Others picked').then(function () { return wait(200); });
      }).then(function () {
        check('Others lists none of yours', !listed(ready), '#' + ready.number + ' among ' + numbers());
        check('Others lists everyone else’s under its own headings', only(['Review requested', 'Agent reviewing', 'Snoozed', 'Not asking you', 'Merged & closed']), headings().join(', '));
        check('with the review asked of you under Review requested, and the agent’s under Agent reviewing', under('Review requested', asked) && under('Agent reviewing', held), headings().join(', '));
        check('the tab picked is remembered', /others/.test(localStorage.getItem('codealong.prTab') || ''), localStorage.getItem('codealong.prTab'));
        window.__emit('reveal', { kind: 'pr', key: ready.key });
        return until(function () { return tab('Yours').getAttribute('aria-selected') === 'true' && listed(ready); }, 5000, 'Yours shown for a PR of yours shown from elsewhere').then(function () {
          var row = all('.ca-board-list .ca-prrow').filter(isRow(ready))[0];
          check('a PR of yours shown from elsewhere switches to Yours, selected', row && row.classList.contains('is-selected'), row && row.className);
          tab('Others').click();
          return until(function () { return tab('Others').getAttribute('aria-selected') === 'true'; }, 5000, 'Others shown');
        }).then(function () {
          nav('Sessions').click();
          return until(function () { return !tab('Others'); }, 5000, 'Sessions shown');
        }).then(function () {
          nav('PRs').click();
          return until(function () { return tab('Others') && q('.ca-prrow'); }, 5000, 'the PRs again').then(function () { return wait(300); });
        }).then(function () {
          check('a tab picked after it stays through a visit to Sessions', tab('Others').getAttribute('aria-selected') === 'true' && !listed(ready), tabs().map(function (t) { return t.textContent + '=' + t.getAttribute('aria-selected'); }).join(', '));
        });
      });
    })
    .then(done)
    .catch(stopped);
})();
