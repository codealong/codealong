// Checks a PR waiting on your review in the waiting window (Waiting.tsx, PrCard.tsx): it comes
// after the sessions that need you and before the finished turns; ⌘] reaches its card, with the
// box's cursor and the strip saying what's left; ⌘] goes on to the session after the reviews and
// ⌥⇧↑ back; ⌃⌘1 snoozes the review and goes on; ⌘⏎ approves it with what's in the box and goes
// on at once, and a review GitHub refuses brings the PR back round with its text and why on the strip.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/waiting-review-check.js
(function () {
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
          var asked = Object.assign({}, x, { state: 'open', mine: false, author: 'ana', reviewRequested: true, requestedBy: 'ana', requestedAt: now - (2 - n) * 1800e3, snoozedUntil: null, mergedAt: null, closedAt: null, title: 'Review ' + 'XY'[n] });
          if (!prs[n]) prs[n] = asked;
          n += 1;
          return asked;
        }
        return Object.assign({}, x, { reviewRequested: false });
      });
      return board;
    });
  };

  var sessionTitle = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var prTitle = function () { var t = document.querySelector('.ca-prreview .ca-prinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-reply-queue-note'); return t ? t.textContent : ''; };
  // What the strip counts of what else waits on you: the total, then the counts' tooltip.
  var counted = function () { var c = q('.ca-reply-queue-counts'); if (!c) return ''; var n = all('.ca-reply-queue-count', c).reduce(function (sum, x) { return sum + Number(x.textContent); }, 0); return n + ' · ' + c.title; };
  var onSession = function (name) { return until(function () { return sessionTitle() === name; }, 3000, name); };
  var onReview = function (name) { return until(function () { return prTitle() === name; }, 3000, name); };

  say('waiting review check: waiting for the window');
  until(function () { return picked.length === plan.length && prs.length === 2 && !!sessionTitle(); }, 20000, 'the window')
    .then(function () { return onSession('Queue A'); })
    .then(function () {
      check('it opens on what needs you first', sessionTitle() === 'Queue A', sessionTitle());
      key(window, { key: ']', code: 'BracketRight', metaKey: true });
      return onReview('Review X');
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('⌘] goes on to the review that waited longest', prTitle() === 'Review X', prTitle());
      check('the cursor is in the box that reviews it', document.activeElement && document.activeElement.tagName === 'TEXTAREA' && !!document.activeElement.closest('.ca-prreview-box'));
      check('the strip counts what’s left, the skipped one included', /^3 · Waiting on you after this one: /.test(counted()), counted());
      key(window, { key: ']', code: 'BracketRight', metaKey: true });
      return onReview('Review Y');
    })
    .then(function () {
      key(window, { key: ']', code: 'BracketRight', metaKey: true });
      return onSession('Queue B');
    })
    .then(function () {
      check('⌘] goes on through the reviews to the finished turn', sessionTitle() === 'Queue B', sessionTitle());
      key(window, { key: 'ArrowUp', altKey: true, shiftKey: true });
      return onReview('Review Y');
    })
    .then(function () {
      check('⌥⇧↑ goes back to the review', true);
      var before = calls('snooze_reviews').length;
      key(window, { key: '1', code: 'Digit1', metaKey: true, ctrlKey: true });
      return until(function () { return calls('snooze_reviews').length === before + 1; }, 2000, '⌃⌘1').then(function () {
        var c = calls('snooze_reviews')[before][1];
        check('⌃⌘1 snoozes the review', c.keys.join() === prs[1].key && c.until > now, JSON.stringify(c));
        return onSession('Queue B');
      });
    })
    .then(function () {
      check('and goes on to the next that waits', /snoozed\.$/.test(strip()), strip());
      key(window, { key: 'ArrowUp', altKey: true, shiftKey: true });
      return onReview('Review X');
    })
    .then(function () {
      // A review GitHub refuses (the stub refuses "fail"): the window goes on at once, then the
      // strip says why, and the PR is back in what's left with its text.
      var box = q('.ca-prreview-box textarea');
      box.focus();
      type(box, 'fail');
      key(box, { key: 'Enter', metaKey: true });
      return onSession('Queue B');
    })
    .then(function () {
      check('⌘⏎ goes on before GitHub answers', sessionTitle() === 'Queue B', sessionTitle());
      return until(function () { return /Didn’t approve #\d+: GitHub said no/.test(strip()); }, 3000, 'why it didn’t go');
    })
    .then(function () {
      check('a refused review says why on the strip', true);
      // ⌘] round what's left (the skipped session first) until the PR comes back.
      var n = 0;
      return (function step() {
        if (prTitle() === 'Review X') return Promise.resolve();
        if (n++ > 4) return Promise.reject(new Error('timeout: Review X back round'));
        key(window, { key: ']', code: 'BracketRight', metaKey: true });
        return wait(300).then(step);
      })();
    })
    .then(function () {
      var box = q('.ca-prreview-box textarea');
      check('and the PR comes back round with what was written', box.value === 'fail', box.value);
      box.focus();
      type(box, 'Ship it.');
      var before = calls('review_pr').length;
      key(box, { key: 'Enter', metaKey: true });
      return until(function () { return calls('review_pr').length === before + 1; }, 2000, '⌘⏎').then(function () {
        var c = calls('review_pr')[before][1];
        check('⌘⏎ approves it with what’s in the box', c.repo === prs[0].repoSlug && c.number === prs[0].number && c.verdict === 'approve' && c.body === 'Ship it.', JSON.stringify(c));
        return onSession('Queue B');
      });
    })
    .then(function () {
      check('and goes on, the strip saying so', /approved\.$/.test(strip()), strip());
      return wait(200);
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' (showing “' + (prTitle() || sessionTitle()) + '”)'); });
})();
