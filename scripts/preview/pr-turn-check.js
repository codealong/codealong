// Checks that a PR row's review pill says whose move it is where GitHub's review state doesn't
// (prStatus.ts `turnPill`, board.rs `turn_of`): four open PRs made a PR of yours whose fixes
// went unsaid (Reply needed, amber), one whose reviewer is asked again (Re-review asked, grey),
// one whose reviewer wrote again (Reviewer replied, amber), and someone else's whose author wrote
// after you (Author replied, amber), each saying who and why on hover, where Changes requested
// would have shown; and one with no turn still saying Changes requested.
//   scripts/preview.sh prs '' scripts/preview/pr-turn-check.js
//   PREVIEW_SCHEME=dark scripts/preview.sh prs '' scripts/preview/pr-turn-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var at = Date.now() - 20 * 60e3;
  var picked = [];
  var turns = [
    { mine: true, state: 'changes', turn: { kind: 'replyNeeded', who: 'ken', since: at } },
    { mine: true, state: 'changes', turn: { kind: 'rereviewAsked', who: 'ken', since: at } },
    { mine: true, state: 'changes', turn: { kind: 'reviewerReplied', who: 'ken', since: at } },
    { mine: false, state: 'open', reviewRequested: false, turn: { kind: 'authorReplied', who: 'gwen', since: at } },
    { mine: true, state: 'changes', turn: undefined },
  ];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var n = 0;
      board.prs = (board.prs || []).map(function (x) {
        var as = turns[n];
        if (!as) return x;
        n += 1;
        var pr = Object.assign({}, x, { mergedAt: null, closedAt: null, snoozedUntil: null, autoReview: undefined, merge: 'blocked', checks: { passed: 3, failed: 0, pending: 0, skipped: 0, failing: [] } }, as);
        picked.push(pr);
        return pr;
      });
      return board;
    });
  };
  var rowOf = function (pr) { return all('.ca-prrow').filter(function (r) { return (q('.ca-prrow-num', r) || {}).textContent === '#' + pr.number; })[0]; };
  var pill = function (pr) { var r = rowOf(pr); return r && q('.ca-prpill--review', r); };

  say('pr turn check: waiting for the PRs');
  until(function () { return picked.length === turns.length && picked.every(pill); }, 20000, 'five PRs in the PRs view, each with a review pill')
    .then(function () {
      var want = [
        ['Reply needed', 'is-attn', /^Commits came after ken asked for changes \(.+\), and nobody has said so on it\. GitHub tells a reviewer of a comment or a review request, not of a push/],
        ['Re-review asked', 'is-plain', /^Waiting on ken, who asked for changes: asked to review it again, or written to since \(.+\)\. GitHub keeps their request for changes until they approve\.$/],
        ['Reviewer replied', 'is-attn', /^ken asked for changes, then wrote again \(.+\): your move\./],
        ['Author replied', 'is-attn', /^gwen wrote \(.+\) after your last review or comment: your move\.$/],
        ['Changes requested', 'is-attn', /^A reviewer asked for changes$/],
      ];
      picked.forEach(function (pr, i) {
        var p = pill(pr);
        var text = q('.ca-prpill-text', p).textContent;
        var title = p.getAttribute('title') || '';
        check('#' + pr.number + ' reads ' + want[i][0], text === want[i][0] && p.classList.contains(want[i][1]) && want[i][2].test(title), text + ' / ' + p.className + ' / ' + title);
      });
    })
    .then(done)
    .catch(stopped);
})();
