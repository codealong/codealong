// Checks submitting the agent's review in one press, from a PR's box (PrBox.tsx, agentReview.ts
// `agentsVerdict`): where the agent suggests requesting changes and its review of the commit the
// PR's at waits to be submitted, Request changes is the filled button, Approve steps back, the line
// under the box says so, and a press with nothing written submits the agent's review as that verdict
// (review_pr with `agents`), what you wrote going first when there's some; where it suggests
// commenting, Comment is, and ⌥⌘⏎ in the box does the same; where it suggests approving, or its
// review went into a draft that has gone from GitHub since, the box is as it always is, Request
// changes needing something written.
//   scripts/preview.sh prs '' scripts/preview/agent-submit-check.js
(function () {
  ['codealong.prPanelMax', 'codealong.prDetailsOpen'].forEach(function (k) { localStorage.removeItem(k); });
  var HEAD = '1234567890abcdef1234567890abcdef12345678';
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'pr_detail') return Promise.resolve({ bodyHtml: '<p>Adds teams.</p>', mergeable: null, commits: 1, reviews: [], requested: [], comments: [], files: [], moreFiles: false });
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var suggests = ['request-changes', 'comment', 'approve', 'request-changes'];
      board.prs = (board.prs || []).map(function (x, i) {
        if (i >= suggests.length) return Object.assign({}, x, { reviewRequested: false });
        var pr = Object.assign({}, x, {
          state: 'open', mine: false, author: 'ana', reviewRequested: true, requestedAt: Date.now() - 3600e3, snoozedUntil: null, mergedAt: null, closedAt: null, updatedAt: Date.now() - i * 1000,
          headOid: HEAD,
          autoReview: { stage: i === 3 ? 'drafted' : 'posted', head: HEAD, at: Date.now() - 600e3, rounds: 1, suggests: suggests[i], comment: x.url + '#issuecomment-1' },
        });
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
  var rowOf = function (pr) { return all('.ca-prrow').filter(function (r) { return r.textContent.indexOf('#' + pr.number + ' ') >= 0 || r.querySelector('.ca-prrow-num').textContent === '#' + pr.number; })[0]; };
  var button = function (label) { return all('.ca-prreview-buttons .ca-btn').filter(function (b) { return b.textContent.trim() === label; })[0]; };
  var note = function () { return (q('.ca-prreview-box .ca-composer-note') || {}).textContent || ''; };
  var show = function (pr) {
    press(rowOf(pr));
    return until(function () { return q('.ca-prpanel') && q('.ca-insp-call') && q('.ca-insp-call').textContent === '#' + pr.number && button('Approve'); }, 5000, '#' + pr.number + '’s box');
  };
  var lastReview = function () { return last('review_pr'); };

  say('agent submit check: waiting for the PRs');
  until(function () { return picked.length === 4 && rowOf(picked[3]); }, 20000, 'four PRs asking for your review')
    .then(function () { return show(picked[0]); })
    .then(function () {
      var changes = button('Request changes');
      check('the agent’s request for changes is the filled button', changes.classList.contains('ca-btn--primary') && !changes.disabled, changes.className);
      check('Approve steps back beside it', !button('Approve').classList.contains('ca-btn--primary'));
      check('its tooltip says what goes with it', /^Request changes pointing to the agent’s review on the PR\. The agent suggests this\.$/.test(changes.getAttribute('title')), changes.getAttribute('title'));
      check('the line under the box says the press, and that approving is still there', /^Request changes sends the agent’s review · ⌘⏎ approves anyway/.test(note()), note());
      changes.click();
      return until(function () { return lastReview(); }, 3000, 'review_pr');
    })
    .then(function (sent) {
      check('a press with nothing written submits the agent’s review as that verdict', sent.verdict === 'request-changes' && sent.body === '' && sent.agents === true && sent.number === picked[0].number, JSON.stringify(sent));
      return wait(200);
    })
    .then(function () {
      type(q('.ca-prreview-box textarea'), 'Also, add a test.');
      return wait(50);
    })
    .then(function () {
      check('with words written, its tooltip says they go first', /what you wrote before it/.test(button('Request changes').getAttribute('title')), button('Request changes').getAttribute('title'));
      button('Request changes').click();
      return until(function () { return calls('review_pr').length === 2; }, 3000, 'a second review_pr');
    })
    .then(function () {
      var sent = lastReview();
      check('what you wrote goes with it', sent.body === 'Also, add a test.' && sent.agents === true, JSON.stringify(sent));
      return show(picked[1]);
    })
    .then(function () {
      var comment = button('Comment');
      check('where it suggests commenting, Comment is the filled one, needing nothing written', comment.classList.contains('ca-btn--primary') && !comment.disabled && !button('Request changes').classList.contains('ca-btn--primary') && button('Request changes').disabled, comment.className);
      check('and the line says ⌥⌘⏎ comments with it', /^⌥⌘⏎ comments with the agent’s review/.test(note()), note());
      var box = q('.ca-prreview-box textarea');
      box.focus();
      key(box, { key: 'Enter', metaKey: true, altKey: true });
      return until(function () { return calls('review_pr').length === 3; }, 3000, 'review_pr from ⌥⌘⏎');
    })
    .then(function () {
      var sent = lastReview();
      check('⌥⌘⏎ comments with the agent’s review', sent.verdict === 'comment' && sent.agents === true && sent.number === picked[1].number, JSON.stringify(sent));
      return show(picked[2]);
    })
    .then(function () {
      check('where it suggests approving, the box is as it always is', button('Approve').classList.contains('ca-btn--primary') && button('Request changes').disabled && !button('Request changes').classList.contains('ca-btn--primary'), button('Approve').className);
      check('its line says ⌘⏎ approves', /^⌘⏎ approves/.test(note()), note());
      return show(picked[3]);
    })
    .then(function () {
      // Its agent's review went into a draft that's gone from GitHub (no PR has one here).
      check('a drafted review whose draft has gone leaves the box as it always is', button('Approve').classList.contains('ca-btn--primary') && button('Request changes').disabled && !button('Request changes').classList.contains('ca-btn--primary'), button('Request changes').className);
    })
    .then(done, stopped);
})();
