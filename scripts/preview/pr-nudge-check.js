// Checks the switch that has a PR's sessions told of what happens on it (PrLink.tsx, nudges.rs), on
// the card of a session's PR chip. The first session a PR lists has that PR made an open one with
// the nudge off, as every PR starts: hovering its chip in the session's header shows the card with
// the switch off; a click turns it on for that PR alone (set_pr_nudge with its key) and the card
// says so at once; a board that says the same keeps it on; a second click turns it off. A merged
// PR's card has no switch.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/pr-nudge-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var picked = null;
  var board = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (picked || s.visitor || s.archived || l.hidden || l.device) return;
          var pr = b.prs.find(function (x) { return x.sessions.some(function (y) { return y.key === s.key; }); });
          if (pr) picked = { key: s.key, lane: l.id, title: s.title, pr: pr };
        });
      });
      if (picked) {
        b.prs.forEach(function (x) {
          if (x.key === picked.pr.key) Object.assign(x, { state: 'open', mergedAt: null, closedAt: null, nudge: false });
        });
        // The session's header shows its worktree's PR and the PRs that list it: this one either way.
        b.lanes.forEach(function (l) {
          if (l.id === picked.lane && l.pr && l.pr.number === picked.pr.number) Object.assign(l.pr, { state: 'open', mergedAt: null, closedAt: null });
        });
      }
      // The check's own copy to change and push: the page keeps the parts of a push that equal
      // what it has, so changing the board it holds would look like no change at all.
      board = JSON.parse(JSON.stringify(b));
      return b;
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var title = function () { var t = document.querySelector('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var chip = function () {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-sessinsp .ca-prchipcard'), function (c) { return new RegExp('#' + picked.pr.number + '(\\D|$)').test(c.textContent); })[0];
  };
  var card = function () { return document.querySelector('.ca-prhover'); };
  var toggle = function () { return card() && card().querySelector('.ca-prhover-nudge'); };
  var on = function () { return toggle().getAttribute('aria-checked') === 'true'; };
  var hover = function (el) { el.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, relatedTarget: document.body })); };
  var leave = function (el) { el.dispatchEvent(new MouseEvent('mouseout', { bubbles: true, relatedTarget: document.body })); };
  var push = function (change) {
    board.prs.forEach(function (x) { if (x.key === picked.pr.key) Object.assign(x, change); });
    board.generatedAt = Date.now();
    window.__emit('board', JSON.parse(JSON.stringify(board)));
  };

  say('pr nudge check: waiting for the list');
  until(function () { return row(); }, 20000)
    .catch(function () { throw new Error('no session on the board that a PR lists'); })
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      row().click();
      return until(function () { return title() === picked.title && chip(); }, 10000, "the clicked session's panel, with its PR's chip");
    })
    .then(function () {
      hover(chip());
      return until(toggle, 3000, "the card of the session's PR chip, with its switch");
    })
    .then(function () {
      check('the card names the PR', card().querySelector('.ca-prhover-ref').textContent === picked.pr.repoSlug + '#' + picked.pr.number);
      check('its nudge is off, as every PR starts', !on() && /^Off for this PR/.test(toggle().parentNode.querySelector('.ca-prhover-meta').textContent), toggle().parentNode.textContent);
      check('nothing is sent until it is clicked', calls('set_pr_nudge').length === 0);
      var r = card().getBoundingClientRect();
      check('the card is inside the window', r.left >= 0 && r.top >= 0 && r.right <= window.innerWidth && r.bottom <= window.innerHeight, [r.left, r.top, r.right, r.bottom].map(Math.round).join(','));
      toggle().click();
      return wait(100);
    })
    .then(function () {
      var sent = calls('set_pr_nudge');
      var a = sent.length && sent[sent.length - 1][1];
      check('a click turns it on for that PR', sent.length === 1 && a.key === picked.pr.key && a.on === true, JSON.stringify(a));
      check('and the card stays up and says so at once', !!toggle() && on() && /^On: each session/.test(toggle().parentNode.querySelector('.ca-prhover-meta').textContent), toggle() && toggle().parentNode.textContent);
      push({ nudge: true });
      return wait(200);
    })
    .then(function () {
      check('the board that says so keeps it on', !!toggle() && on());
      push({ state: 'approved' });
      return wait(200);
    })
    .then(function () {
      check('approved, it still says so', /^On: each session/.test(toggle().parentNode.querySelector('.ca-prhover-meta').textContent), toggle().parentNode.textContent);
      toggle().click();
      return wait(100);
    })
    .then(function () {
      var sent = calls('set_pr_nudge');
      var a = sent[sent.length - 1][1];
      check('a second click turns it off', sent.length === 2 && a.key === picked.pr.key && a.on === false && !on(), JSON.stringify(a));
      push({ nudge: false, state: 'merged', mergedAt: Date.now(), closedAt: Date.now() });
      return wait(200);
    })
    .then(function () {
      check('a merged PR has no switch', !card() || !toggle());
      if (card()) leave(card());
    })
    .then(done)
    .catch(stopped);
})();
