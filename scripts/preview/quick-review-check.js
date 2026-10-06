// Checks a PR waiting on your review in the quick panel (Quick.tsx): the empty panel lists it
// under Review requested, after the sessions that need you and before the finished turns; ⏎ on
// it hands it to the waiting window to review, and the panel puts itself away; ⇥ on a session
// hands that one over to answer. The window's side is waiting-review-check.js.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/quick-review-check.js
(function () {
  localStorage.setItem('codealong.quick.project', 'All projects');
  var now = Date.now();
  var plan = [
    { status: 'needs', since: now - 3600e3 },
    { status: 'unread', since: now - 10 * 60e3 },
  ];
  var picked = [];
  var prs = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (l.hidden || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.asks = p.status === 'needs';
          s.title = 'Queue ' + 'AB'[picked.length];
          picked.push(s.key);
        });
      });
      // Two PRs ask for your review, the first for longer; no other does.
      var n = 0;
      board.prs = (board.prs || []).map(function (x) {
        if (n < 2) {
          var asked = Object.assign({}, x, { state: 'open', reviewRequested: true, requestedBy: 'ana', requestedAt: now - (2 - n) * 1800e3, snoozedUntil: null, mergedAt: null, closedAt: null, title: 'Review ' + 'XY'[n] });
          if (!prs[n]) prs[n] = asked;
          n += 1;
          return asked;
        }
        return Object.assign({}, x, { reviewRequested: false });
      });
      return board;
    });
  };

  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var rows = function () { return all('.ca-quick-row'); };
  var active = function () { return (q('.ca-quick-row.is-active .ca-quick-row-title') || {}).textContent; };
  /** The waiting window asked for, the latest first. */
  var asked = function () { var c = calls('show_waiting'); return c.length ? (c[c.length - 1][1].context || {}) : null; };

  say('quick review check: waiting for the list');
  until(function () { return picked.length === plan.length && prs.length === 2 && rows().length > 0; }, 20000, 'the list')
    .then(function () { return wait(300); })
    .then(function () {
      var titles = rows().map(function (r) { return r.querySelector('.ca-quick-row-title').textContent.trim(); });
      var i = titles.indexOf('Review X');
      check('lists the reviews after what needs you and before the finished turns', titles.indexOf('Queue A') < i && i < titles.indexOf('Review Y') && titles.indexOf('Review Y') < titles.indexOf('Queue B'), titles.join('|'));
      key(input(), { key: 'ArrowDown' });
      return wait(150);
    })
    .then(function () {
      check('↓ reaches the review', active() === 'Review X', active());
      var hides = calls('hide_quick').length;
      key(input(), { key: 'Enter' });
      return until(function () { return !!asked(); }, 2000, 'the waiting window').then(function () {
        check('⏎ on a review hands it to the waiting window', asked().key === prs[0].key, JSON.stringify(asked()));
        check('and the panel puts itself away', calls('hide_quick').length > hides);
      });
    })
    .then(function () { return wait(150); })
    .then(function () {
      check('the list is back at its first row', active() === 'Queue A', active());
      var before = calls('show_waiting').length;
      key(input(), { key: 'Tab' });
      return until(function () { return calls('show_waiting').length > before; }, 2000, '⇥').then(function () {
        check('⇥ on a session hands it to the waiting window to answer', asked().key === picked[0] && !asked().rename, JSON.stringify(asked()));
      });
    })
    .then(function () {
      check('the panel has no reply view of its own', !q('.ca-quick .ca-reply') && !q('.ca-prreview'));
      return wait(200);
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' (on “' + active() + '”)'); });
})();
