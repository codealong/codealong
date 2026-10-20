// Checks what the windows say of the review you last gave a PR (`PrItem.lastReview`,
// prStatus.ts `yourReview`): a PR that asks for your review again after you approved an earlier
// commit reads Re-review in its row, ahead of Ready to merge; its box's line leads with your
// approval and a pill says how many commits came since, which shows what they changed under
// Changes, where a switch goes back to all of it; a PR whose commit you approved says so, and
// Approve reads Approved, stepped back, the agent's suggestion gone; approving a PR shows that
// at once; and a branch rewritten since your review says so, and Changes shows all of it.
//   scripts/preview.sh prs '' scripts/preview/your-review-check.js
(function () {
  ['codealong.prPanelMax', 'codealong.prDetailsOpen'].forEach(function (k) { localStorage.removeItem(k); });
  var HEAD = 'c0ffee1c0ffee1c0ffee1c0ffee1c0ffee1c0ffe';
  var OLD = '1234567890abcdef1234567890abcdef12345678';
  var REWRITTEN = 'feedbee0feedbee0feedbee0feedbee0feedbee0';
  var FILES = ['src/teams.ts', 'src/teams.test.ts', 'README.md'].map(function (path) { return { path: path, status: 'modified', additions: 3, deletions: 1, patch: '@@ -1,2 +1,4 @@\n a\n-b\n+c\n+d\n+e' }; });
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'pr_detail') return Promise.resolve({ bodyHtml: '<p>Adds teams.</p>', mergeable: null, commits: 3, reviews: [], requested: [], comments: [], files: FILES, moreFiles: false });
    if (cmd === 'pr_since') {
      real(cmd, args, options);
      return Promise.resolve(args.from === REWRITTEN ? { follows: false, commits: 0, files: [], moreFiles: false } : { follows: true, commits: 2, files: FILES.slice(0, 2), moreFiles: false });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var agent = { stage: 'posted', head: HEAD, at: Date.now() - 600e3, rounds: 1, suggests: 'approve' };
      var shape = [
        // Asked again, after you approved an earlier commit; your approval has it ready.
        { reviewRequested: true, state: 'approved', merge: 'clean', lastReview: { state: 'approved', commit: OLD, at: Date.now() - 86400e3 }, autoReview: agent },
        // You approved the commit it's at.
        { reviewRequested: false, state: 'approved', merge: 'blocked', lastReview: { state: 'approved', commit: HEAD, at: Date.now() - 300e3 }, autoReview: agent },
        // Never reviewed.
        { reviewRequested: true, state: 'open', merge: 'blocked', lastReview: undefined, autoReview: undefined },
        // Commented on a commit the branch no longer has.
        { reviewRequested: true, state: 'open', merge: 'blocked', lastReview: { state: 'commented', commit: REWRITTEN, at: Date.now() - 7200e3 }, autoReview: undefined },
      ];
      board.prs = (board.prs || []).map(function (x, i) {
        if (i >= shape.length) return Object.assign({}, x, { reviewRequested: false, lastReview: undefined });
        var pr = Object.assign({}, x, {
          mine: false, author: 'ana', requestedAt: Date.now() - 3600e3, snoozedUntil: null, mergedAt: null, closedAt: null, updatedAt: Date.now() - i * 1000,
          headOid: HEAD, checks: { passed: 3, failed: 0, pending: 0, skipped: 0, failing: [] }, reviewersAsked: [],
        }, shape[i]);
        picked[i] = pr;
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
  var rowOf = function (pr) { return all('.ca-prrow').filter(function (r) { return (q('.ca-prrow-num', r) || {}).textContent === '#' + pr.number; })[0]; };
  var button = function (label) { return all('.ca-prreview-buttons .ca-btn').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var text = function (sel) { return (q(sel) || {}).textContent || ''; };
  var line = function () { return text('.ca-prbox-line-says'); };
  var shownFiles = function () { return all('.ca-prfiles-sec .ca-prfile').length; };
  var segOn = function () { return text('.ca-prfiles-top .ca-segctl .is-on'); };
  var show = function (pr, says) {
    press(rowOf(pr));
    return until(function () { return text('.ca-insp-call') === '#' + pr.number && says.test(line()); }, 5000, '#' + pr.number + '’s line saying ' + says);
  };

  say('your review check: waiting for the PRs');
  until(function () { return picked.length === 4 && rowOf(picked[3]); }, 20000, 'four PRs')
    .then(function () {
      var pills = all('.ca-prpill-text', rowOf(picked[0])).map(function (p) { return p.textContent; });
      check('a PR that asks for your review again reads Re-review, ahead of Ready to merge', pills[0] === 'Re-review' && pills[1] === 'Ready to merge', pills.join(', '));
      var title = q('.ca-prpill', rowOf(picked[0])).getAttribute('title');
      check('and says what yours was', /^Your review is asked for again\. You approved 1234567, and it’s had commits since\./.test(title), title);
      check('one never reviewed reads Review requested', all('.ca-prpill-text', rowOf(picked[2]))[0].textContent === 'Review requested');
      return show(picked[0], /^You approved 1234567$/);
    })
    .then(function () {
      return until(function () { return /2 new commits/.test(text('.ca-prbox-since')); }, 5000, 'the pill saying 2 new commits');
    })
    .then(function () {
      check('the agent’s suggestion is still there, for the new commits', /Agent suggests approving/.test(text('.ca-prbox-standing')), text('.ca-prbox-standing'));
      check('Approve is the filled one', button('Approve') && button('Approve').classList.contains('ca-btn--primary'));
      check('the comparison asked for is your commit against the one it’s at', JSON.stringify(last('pr_since')) === JSON.stringify({ repo: picked[0].repoSlug, from: OLD, to: HEAD }), JSON.stringify(last('pr_since')));
      press(q('.ca-prbox-since'));
      return until(function () { return text('.ca-tab.is-on') .indexOf('Changes') === 0 && shownFiles() === 2; }, 5000, 'Changes showing the two files changed since');
    })
    .then(function () {
      check('the pill shows Changes since your review', /^Since 1234567$/.test(segOn()), segOn());
      check('saying how many commits', text('.ca-prfiles-since') === '2 commits since your review of 1234567.', text('.ca-prfiles-since'));
      check('and counting its files', /^Files changed · 2$/.test(text('.ca-prfiles-top .ca-label')), text('.ca-prfiles-top .ca-label'));
      press(all('.ca-prfiles-top .ca-segctl button').filter(function (b) { return b.textContent === 'All'; })[0]);
      return until(function () { return shownFiles() === 3; }, 3000, 'All showing every file');
    })
    .then(function () {
      check('All goes back to every change', !q('.ca-prfiles-since') && /^Files changed · /.test(text('.ca-prfiles-top .ca-label')));
      return show(picked[1], /, its latest commit$/);
    })
    .then(function () {
      check('a PR whose commit you approved says so', line() === 'You approved c0ffee1, its latest commit', line());
      check('without the agent’s suggestion', !/suggests/i.test(text('.ca-prbox-standing')), text('.ca-prbox-standing'));
      check('nor a pill', !q('.ca-prbox-since'));
      var approved = button('Approved');
      check('Approve reads Approved, stepped back', approved && !approved.classList.contains('ca-btn--primary'), approved && approved.className);
      check('its tooltip says another press is another approval', approved && /^You approved c0ffee1, the commit it’s at, 5m ago\. Approving again adds another approval/.test(approved.getAttribute('title')), approved && approved.getAttribute('title'));
      check('the line under the box says ⌘⏎ approves again', /^⌘⏎ approves again/.test(text('.ca-prreview-box .ca-composer-note')), text('.ca-prreview-box .ca-composer-note'));
      var header = all('.ca-btn').filter(function (b) { return !b.closest('.ca-prreview-buttons') && /^Approv/.test(b.textContent.trim()); })[0];
      check('and so does the panel’s own Approve', header && header.textContent.trim() === 'Approved', header && header.textContent);
      return show(picked[2], /^No review of yours yet$/);
    })
    .then(function () {
      button('Approve').click();
      return until(function () { return line() === 'You approved c0ffee1, its latest commit' && button('Approved'); }, 3000, 'the approval showing at once');
    })
    .then(function () {
      check('approving shows your approval at once', true);
      check('and asks GitHub for it', last('review_pr').verdict === 'approve' && last('review_pr').number === picked[2].number, JSON.stringify(last('review_pr')));
      return show(picked[3], /^You commented on feedbee$/);
    })
    .then(function () {
      return until(function () { return text('.ca-prbox-since') === 'Rewritten since'; }, 5000, 'the pill saying the branch was rewritten');
    })
    .then(function () {
      press(q('.ca-prbox-since'));
      return until(function () { return /rebased or force-pushed/.test(text('.ca-prfiles-since')); }, 5000, 'Changes saying why it shows everything');
    })
    .then(function () {
      check('a branch rewritten since shows every change, and says why', shownFiles() === 3, String(shownFiles()));
    })
    .then(done, stopped);
})();
