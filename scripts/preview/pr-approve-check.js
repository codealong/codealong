// Checks a PR panel's row of buttons in the PRs view (PrInspector.tsx, MergeBox.tsx): Approve is
// there for an open PR that isn't yours though your review wasn't asked for, bordered rather than
// filled, and approves it; the merge box's button comes right after it, in the same row, before
// GitHub and Review with…; the PR's menu has Approve too. Your own PR has no Approve, in its panel
// or its menu (GitHub doesn't let you approve your own), and its merge button leads the row. In
// each project shown, the latest open PR is made someone else's that doesn't ask for your review,
// and the next your own.
//   PREVIEW_WAIT=60000 scripts/preview.sh prs '' scripts/preview/pr-approve-check.js
(function () {
  var press = function (el) {
    el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0, buttons: 1 }));
    el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, button: 0 }));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  };
  var others = [];
  var yours = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var open = board.prs.filter(function (pr) { return pr.state !== 'merged' && pr.state !== 'closed'; });
      board.projects.forEach(function (project) {
        var here = open.filter(function (pr) { return pr.project === project.name; }).sort(function (a, b) { return b.updatedAt - a.updatedAt; });
        if (here[0]) {
          Object.assign(here[0], { state: 'open', mine: false, author: 'ana', reviewRequested: false, snoozedUntil: null });
          others.push('#' + here[0].number);
        }
        if (here[1]) {
          Object.assign(here[1], { state: 'open', mine: true, reviewRequested: false, snoozedUntil: null });
          yours.push('#' + here[1].number);
        }
      });
      board.pinnedPrs = [];
      return board;
    });
  };
  var number = function (row) { return (q('.ca-prrow-num', row) || {}).textContent; };
  var row = function (n) { return all('.ca-prrow').filter(function (r) { return number(r) === n; })[0]; };
  var buttons = function () { return all('.ca-prinsp .ca-insp-actions > .ca-btn, .ca-prinsp .ca-insp-actions > .ca-split > .ca-btn:first-child'); };
  var labels = function () { return buttons().map(function (b) { return b.textContent; }); };
  var approve = function () { return buttons().filter(function (b) { return b.textContent === 'Approve'; })[0]; };
  var merge = function () { return q('.ca-prinsp .ca-insp-actions .ca-merge-main'); };
  var theirs, mineN;

  say('pr approve check: waiting for PR rows');
  until(function () { return q('.ca-prrow'); }, 20000)
    .then(function () {
      theirs = others.filter(row)[0];
      mineN = yours.filter(row)[0];
      if (!theirs) throw new Error('the project shown has no open PR');
      press(row(theirs));
      return until(function () { return !!merge(); }, 5000, 'the merge button beside Approve');
    })
    .then(function () {
      var l = labels();
      check('an open PR that isn’t yours has Approve, though your review wasn’t asked for', l[0] === 'Approve', l.join(' | '));
      check('bordered, since it isn’t asked of you', !approve().classList.contains('ca-btn--primary'), approve().className);
      check('the merge button comes right after it, in the same row', approve().nextElementSibling === merge(), l.join(' | '));
      check('then GitHub and Review with…', l.slice(-2).join('|') === 'GitHub|Review with…', l.join(' | '));
      check('no Snooze… where no review was asked', l.indexOf('Snooze…') < 0, l.join(' | '));
      approve().click();
      return until(function () { return sent('review_pr').length === 1; }, 3000, 'the approval');
    })
    .then(function () {
      var r = sent('review_pr')[0];
      check('Approve approves it', r.verdict === 'approve' && '#' + r.number === theirs && r.body === '', JSON.stringify(r));
      return menuOf(row(theirs));
    })
    .then(function (items) {
      check('its menu has Approve', items.indexOf('Approve ' + theirs) >= 0, items.join(' | '));
      if (!mineN) {
        note('the project shown has one open PR, so none is yours');
        return null;
      }
      press(row(mineN));
      return until(function () { return !!merge() && q('.ca-prinsp .ca-insp-call') && q('.ca-prinsp .ca-insp-call').textContent === mineN; }, 5000, 'your PR’s panel').then(function () {
        var l = labels();
        check('your own PR has no Approve', l.indexOf('Approve') < 0, l.join(' | '));
        check('its merge button leads the row', buttons()[0] === q('.ca-merge-main > .ca-btn'), l.join(' | '));
        return menuOf(row(mineN));
      });
    })
    .then(function (items) {
      if (items) check('nor in its menu', !items.some(function (x) { return /^Approve /.test(x); }), items.join(' | '));
    })
    .then(done, stopped);
})();
