// Checks merging a PR from its card in the waiting window (PrCard.tsx, MergeBox.tsx, ahead.ts
// `mergeAhead`): the merge leaves what waits on you as a review does, so confirming it goes on to
// the next at once, before GitHub answers, the strip saying it merged; and one GitHub refuses
// (the stub refuses a commit titled "fail") comes back round, the strip saying why. A session
// needs you and two PRs ask for your review; nothing else waits.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/merge-goes-on-check.js
(function () {
  localStorage.removeItem('codealong.merge.methods');
  var now = Date.now();
  var picked = null;
  var prs = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (l.hidden || s.visitor || s.archived || (picked && picked !== s.key)) return;
          picked = s.key;
          s.status = 'needs';
          s.statusSince = now - 3600e3;
          s.asks = true;
          s.title = 'Queue A';
        });
      });
      var n = 0;
      board.prs = (board.prs || []).map(function (x) {
        if (n < 2 && x.state !== 'merged' && x.state !== 'closed') {
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

  var sessionTitle = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var prTitle = function () { var t = q('.ca-prreview .ca-prinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = q('.ca-reply-queue-note'); return t ? t.textContent : ''; };
  var main = function () { return q('.ca-prreview .ca-merge-main > .ca-btn'); };
  var title = function () { return q('.ca-merge-write .ca-field input'); };
  var merges = function () { return sent('merge_pr'); };
  var onReview = function (name) { return until(function () { return prTitle() === name && !!main(); }, 3000, name); };
  /** Presses merge on the card shown and confirms it with `text` as the commit's title. */
  var merge = function (text) {
    main().click();
    return until(function () { return !!title(); }, 2000, 'the commit').then(function () {
      type(title(), text);
      key(title(), { key: 'Enter', metaKey: true });
    });
  };

  say('merge goes on check: waiting for the window');
  until(function () { return !!picked && prs.length === 2 && (!!sessionTitle() || !!prTitle()); }, 20000, 'the window')
    .then(function () {
      key(window, { key: ']', code: 'BracketRight', metaKey: true });
      return onReview('Review X');
    })
    .then(function () {
      return merge('Ship X');
    })
    .then(function () { return wait(100); })
    .then(function () {
      check('confirming a merge goes on at once, before GitHub answers', prTitle() === 'Review Y' && merges().length === 1, prTitle() || sessionTitle());
      check('the strip says it merged', /Review X: merged\.?$/.test(strip()), strip());
      check('it merged the PR it showed', merges()[0].number === prs[0].number && merges()[0].op.op === 'merge' && merges()[0].op.title === 'Ship X', JSON.stringify(merges()[0]));
      return until(function () { return !!main(); }, 3000, 'Review Y’s merge button');
    })
    .then(function () {
      return merge('fail');
    })
    .then(function () { return until(function () { return sessionTitle() === 'Queue A'; }, 3000, 'the next after Review Y'); })
    .then(function () {
      check('a merge GitHub will refuse goes on as well', merges().length === 2 && merges()[1].number === prs[1].number, JSON.stringify(merges()[1]));
      return until(function () { return /Head branch was modified/.test(strip()); }, 3000, 'why it didn’t go');
    })
    .then(function () {
      check('a refused merge says why on the strip', true);
      var n = 0;
      return (function step() {
        if (prTitle() === 'Review Y') return Promise.resolve();
        if (n++ > 4) return Promise.reject(new Error('timeout: Review Y back round'));
        key(window, { key: ']', code: 'BracketRight', metaKey: true });
        return wait(300).then(step);
      })();
    })
    .then(function () {
      check('and the PR comes back round', prTitle() === 'Review Y');
      check('the merged one doesn’t', !all('.ca-prreview .ca-prinsp-title').some(function (t) { return t.textContent === 'Review X'; }));
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' (showing “' + (prTitle() || sessionTitle()) + '”)'); });
})();
