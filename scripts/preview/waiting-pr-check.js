// Checks a PR shown in the waiting window from a session's PR chip (Waiting.tsx, PrCard.tsx
// `from`): a click on the chip shows the PR's card in the session's place rather than the main
// window, with a link back to the session, the box saying a verdict goes back, and the strip
// without Snooze, since the PR asks no review of yours; the link goes back; ⌃- goes back too and
// ⌃⇧- forward to the PR, which still leads back; and ⌘⏎ approves it and goes back to the session
// rather than on to the next that waits, the strip saying there how the PR went.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/waiting-pr-check.js
(function () {
  var now = Date.now();
  var pr = null;
  var picked = { a: null, b: null };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      // Once: a PR one of the board's sessions worked on, that session, and another session.
      if (!pr) {
        (board.prs || []).some(function (x) {
          return x.sessions.some(function (at) {
            var lane = board.lanes.filter(function (l) { return l.id === at.lane && !l.hidden; })[0];
            var s = lane && lane.sessions.filter(function (y) { return y.key === at.key && !y.visitor && !y.archived; })[0];
            if (s) {
              pr = x;
              picked.a = s.key;
            }
            return !!s;
          });
        });
        board.lanes.forEach(function (l) {
          l.sessions.forEach(function (s) { if (!picked.b && picked.a && !l.hidden && !s.visitor && !s.archived && s.key !== picked.a) picked.b = s.key; });
        });
      }
      // The session with the PR needs you, the other's turn ended after it; nothing else waits,
      // and no PR asks for your review.
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (s.visitor) return;
          if (s.key === picked.a) Object.assign(s, { status: 'needs', statusSince: now - 3600e3, asks: true, archived: false, title: 'Queue A' });
          if (s.key === picked.b) Object.assign(s, { status: 'unread', statusSince: now - 10 * 60e3, archived: false, title: 'Queue B' });
        });
      });
      board.prs = (board.prs || []).map(function (x) {
        var none = Object.assign({}, x, { reviewRequested: false });
        return pr && x.key === pr.key ? Object.assign(none, { state: 'open', mine: false, author: 'ana', snoozedUntil: null, mergedAt: null, closedAt: null, title: 'Atlas login' }) : none;
      });
      return board;
    });
  };

  var sessionTitle = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var prTitle = function () { var t = document.querySelector('.ca-prreview .ca-prinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-reply-queue-note'); return t ? t.textContent : ''; };
  var backLink = function () { return document.querySelector('.ca-prreview .ca-panelback'); };
  var onSession = function (name) { return until(function () { return sessionTitle() === name; }, 3000, name); };
  var onPr = function () { return until(function () { return prTitle() === 'Atlas login'; }, 3000, 'the PR from its chip'); };
  var openPr = function () {
    var chip = Array.prototype.filter.call(document.querySelectorAll('.ca-waiting .ca-prchipcard button.ca-pr'), function (b) { return b.textContent.indexOf('#' + pr.number) >= 0; })[0];
    if (!chip) throw new Error('the session’s header has no chip for #' + pr.number);
    chip.click();
    return onPr();
  };

  say('waiting PR check: waiting for the window');
  until(function () { return !!pr && !!picked.b && !!sessionTitle(); }, 20000, 'the window')
    .then(function () { return onSession('Queue A'); })
    .then(openPr)
    .then(function () { return wait(300); })
    .then(function () {
      check('a click on the session’s PR chip shows the PR here', prTitle() === 'Atlas login', prTitle());
      check('not in the main window', calls('show_main').length === 0, JSON.stringify(calls('show_main')));
      var back = backLink();
      check('its card leads back to the session', !!back && back.textContent.trim() === 'Queue A', back ? back.textContent : 'no link back');
      var labels = all('.ca-hotkeys .ca-hotkey').map(function (b) { return b.textContent; });
      check('the strip has the PR’s actions, without Snooze for a review nobody asked of you', labels[0] === 'Approve⌘⏎' && labels[1] === 'Comment⌥⌘⏎' && !labels.some(function (l) { return l.indexOf('Snooze') === 0; }), labels.join('|'));
      check('the box says a verdict goes back', /approves and goes back/.test(q('.ca-prreview').textContent), q('.ca-prreview').textContent.slice(-300));
      backLink().click();
      return onSession('Queue A');
    })
    .then(function () {
      check('the link goes back to the session', sessionTitle() === 'Queue A', sessionTitle());
      // Long enough to count as looked at, for ⌃- (history.ts `DWELL_MS`).
      return wait(1300).then(openPr);
    })
    .then(function () { return wait(300); })
    .then(function () {
      key(window, { key: '-', code: 'Minus', ctrlKey: true });
      return onSession('Queue A');
    })
    .then(function () {
      key(window, { key: '_', code: 'Minus', ctrlKey: true, shiftKey: true });
      return onPr();
    })
    .then(function () {
      check('⌃- goes back to the session and ⌃⇧- forward to the PR', true);
      check('and the PR still leads back', !!backLink());
      return wait(300);
    })
    .then(function () {
      var box = q('.ca-prreview-box textarea');
      box.focus();
      type(box, 'Ship it.');
      var before = calls('review_pr').length;
      key(box, { key: 'Enter', metaKey: true });
      return until(function () { return calls('review_pr').length === before + 1; }, 2000, '⌘⏎').then(function () {
        var c = calls('review_pr')[before][1];
        check('⌘⏎ approves it with what’s in the box', c.repo === pr.repoSlug && c.number === pr.number && c.verdict === 'approve' && c.body === 'Ship it.', JSON.stringify(c));
        return onSession('Queue A');
      });
    })
    .then(function () { return wait(200); })
    .then(function () {
      check('and goes back to the session, not on to the next that waits', sessionTitle() === 'Queue A', sessionTitle());
      // As it went back, or once GitHub took it, which the window's report says on the strip too.
      check('the strip says there how the PR went', new RegExp('#' + pr.number + ': approved\\.$|^Approved #' + pr.number + '\\.$').test(strip()), strip());
      check('nothing went to the main window', calls('show_main').length === 0, JSON.stringify(calls('show_main')));
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' (showing “' + (prTitle() || sessionTitle()) + '”)'); });
})();
