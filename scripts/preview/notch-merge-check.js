// Checks a PR of yours ready to merge in the bar docked in the notch (Bar.tsx `PrsPill`,
// Notch.tsx, NotchReview.tsx): it has its own green merges pill (`N to merge`, `.is-merges`)
// beside the amber reviews pill (`N to review`, `.is-reviews`) and its own count beside the
// camera (`.ca-notch-count.is-merges`). Clicking the merges count opens the PR's card under the
// pills, marking only the merges pill; ⌘[ steps back to the review PR (marking only the reviews
// pill) and ⌘] steps forward to the merge PR; clicking the single-PR merges pill toggles it
// closed and open; and ⌃⌘1 snoozes it and goes on, taking the merges pill off.
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=150000 scripts/preview.sh notch '' scripts/preview/notch-merge-check.js
(function () {
  var calls = sent;
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var reviewsPill = function () { return document.querySelector('.ca-notch-body .ca-hud-pill.is-reviews'); };
  var mergesPill = function () { return document.querySelector('.ca-notch-body .ca-hud-pill.is-merges'); };
  var sessionTitle = function () { var t = document.querySelector('.ca-notch-session .ca-sessinsp-title'); return t ? t.textContent.trim() : ''; };
  var prNumber = function () { var t = document.querySelector('.ca-notch-session .ca-prreview .ca-insp-call'); return t ? t.textContent.trim() : ''; };
  var showsSession = function (item) { return function () { return is('session') && sessionTitle().indexOf(item.title.slice(0, 20)) === 0; }; };
  var showsPr = function (pr) { return function () { return is('session') && prNumber() === '#' + pr.number; }; };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var escape = function () { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true })); };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };

  // One session needing you, one PR asking for your review, and one PR of yours ready to merge.
  var json = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var board = json('./board.json');
  var onBoard = {};
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) { if (!s.visitor && !s.archived && s.key.charAt(0) !== '@') onBoard[s.key] = true; });
  });
  var f = json('./bar-feed.json');
  var items = f.items.filter(function (i) { return onBoard[i.key]; }).slice(0, 1).map(function (i) {
    return Object.assign({}, i, { group: 'needs', status: 'needs', asks: false, snoozedUntil: null, since: 1000, hidden: false });
  });
  var rawPrs = (board.prs || []).slice(0, 2);
  var reviewPr = rawPrs[0] && Object.assign({}, rawPrs[0], { state: 'open', mine: false, author: 'ana', reviewRequested: true, waitsToMerge: false, requestedBy: 'ana', requestedAt: 2000, snoozedUntil: null, mergedAt: null, closedAt: null });
  var mergePr = rawPrs[1] && Object.assign({}, rawPrs[1], { state: 'approved', mine: true, author: 'me', reviewRequested: false, waitsToMerge: true, merge: 'clean', requestedBy: null, requestedAt: null, snoozedUntil: null, mergedAt: null, closedAt: null, updatedAt: 3000 });
  var prs = [reviewPr, mergePr].filter(Boolean);
  var feedReviews = prs.length === 2 ? [
    { key: reviewPr.key, number: reviewPr.number, title: reviewPr.title, repo: reviewPr.repo, repoSlug: reviewPr.repoSlug, url: reviewPr.url, project: reviewPr.project, requestedBy: 'ana', since: 2000, updatedAt: reviewPr.updatedAt },
    { key: mergePr.key, number: mergePr.number, title: mergePr.title, repo: mergePr.repo, repoSlug: mergePr.repoSlug, url: mergePr.url, project: mergePr.project, requestedBy: null, since: 3000, updatedAt: 3000, merge: true },
  ] : [];
  var feed = Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [], reviews: feedReviews, due: [], todos: [] });
  var said = {};
  items.forEach(function (i) {
    said[i.key] = { title: i.title, status: 'needs', asks: false, snoozedUntil: null, archived: false, live: true, surface: 'cli', hosted: false, pid: 4242, attachCommand: null };
  });
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var p = real.apply(this, arguments);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions = l.sessions.map(function (s) { return said[s.key] && !s.visitor ? Object.assign({}, s, said[s.key]) : s; });
      });
      b.prs = b.prs.map(function (x) {
        var found = prs.filter(function (pr) { return pr.key === x.key; })[0];
        return found ? Object.assign({}, x, found) : x;
      });
      return b;
    });
  };

  say('notch merge check: waiting for it');
  until(function () { return notch() && calls('notch_shape').length > 0; }, 15000, 'the notch')
    .then(function () {
      check('has a session, a review PR and a merge PR', items.length === 1 && prs.length === 2, items.length + ' sessions, ' + prs.length + ' PRs');
      if (items.length < 1 || prs.length < 2) throw new Error('the board needs 1 session and 2 PRs');
      window.__emit('bar-feed', feed);
      enter();
      return until(function () { return is('pills') && reviewsPill() && mergesPill(); }, 3000, 'both pills');
    })
    .then(function () { return wait(400); })
    .then(function () {
      check('the reviews pill says 1 to review', /1 to review/.test(reviewsPill().textContent), reviewsPill().textContent);
      check('the merges pill says 1 to merge', /1 to merge/.test(mergesPill().textContent), mergesPill().textContent);
      var count = document.querySelector('.ca-notch-count.is-merges');
      check('the merges count beside the camera is a button with its merge glyph', !!count && count.tagName === 'BUTTON' && !!count.querySelector('.ca-glyph-merge') && count.textContent.trim() === '1', count && count.outerHTML.slice(0, 160));
      count.click();
      return until(showsPr(mergePr), 4000, 'the merge PR');
    })
    .then(function () { return wait(500); })
    .then(function () {
      check('clicking the merges count opens the merge PR card', is('session') && is('open'), prNumber());
      check('only the merges pill is marked', mergesPill().classList.contains('is-peeked') && !reviewsPill().classList.contains('is-peeked'));
      check('its merge box is shown', !!document.querySelector('.ca-notch-session .ca-prreview .ca-merge-main > .ca-btn'));
      // ⌘[ steps back to the review PR, switching the marked pill from merges to reviews.
      key({ key: '[', code: 'BracketLeft', metaKey: true });
      return until(showsPr(reviewPr), 3000, '⌘[ back to the review PR');
    })
    .then(function () {
      check('⌘[ goes back to the review PR, marking only the reviews pill', reviewsPill().classList.contains('is-peeked') && !mergesPill().classList.contains('is-peeked'));
      key({ key: ']', code: 'BracketRight', metaKey: true });
      return until(showsPr(mergePr), 3000, '⌘] forward to the merge PR');
    })
    .then(function () {
      check('⌘] goes on to the merge PR, marking only the merges pill', mergesPill().classList.contains('is-peeked') && !reviewsPill().classList.contains('is-peeked'));
      // A single PR on the merges pill: clicking the pill toggles its card off and back on.
      mergesPill().click();
      return until(function () { return !is('session'); }, 3000, 'toggled closed');
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('clicking the single-PR merges pill puts the card away', !is('session'));
      mergesPill().click();
      return until(showsPr(mergePr), 3000, 'reopened from merges pill');
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('clicking the merges pill opens the merge PR again', showsPr(mergePr)());
      // ⌃⌘1 snoozes the merge PR, taking the merges pill off and advancing to the session.
      var before = calls('snooze_reviews').length;
      key({ key: '1', code: 'Digit1', metaKey: true, ctrlKey: true });
      return until(function () { return calls('snooze_reviews').length === before + 1; }, 2000, '⌃⌘1').then(function () {
        var c = calls('snooze_reviews')[before];
        check('⌃⌘1 snoozes the merge PR', c.keys.join() === mergePr.key, JSON.stringify(c));
        return until(showsSession(items[0]), 3000, 'on to the session');
      });
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('and the merges pill leaves while the reviews pill stays', !mergesPill() && !!reviewsPill());
      escape();
      return until(function () { return !is('session'); }, 3000, 'Escape');
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' (showing “' + (prNumber() || sessionTitle()) + '”, ' + (notch() ? notch().className : 'no notch') + ')'); });
})();
