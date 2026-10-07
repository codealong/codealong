// Checks a PR waiting on your review under the pills of the bar docked in the notch
// (NotchReview.tsx, PrCard.tsx): the reviews pill opens the notch on the PR's card with the
// keyboard and the cursor in its box, the pill marked and the strip saying what else waits; the
// strip has the PR's actions with their keys; ⌘] goes on from the review to the session after it
// and ⌥⇧↑ back; ⌥↑ ⌥↓ step along the bar through sessions and reviews alike; the reviews
// count beside the camera opens the first review; ⌃⌘1 snoozes the review and the notch goes on;
// ⌘⏎ approves it with what's in the box and goes on; ⌃⌘G opens it on GitHub; and Escape puts
// it away.
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=150000 scripts/preview.sh notch '' scripts/preview/notch-review-check.js
(function () {
  var calls = sent;
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var pill = function (key) { return document.querySelector('.ca-notch-body .ca-hud-pill[data-key="' + key + '"]'); };
  var reviewsPill = function () { return document.querySelector('.ca-notch-body .ca-hud-pill.is-reviews'); };
  var sessionTitle = function () { var t = document.querySelector('.ca-notch-session .ca-sessinsp-title'); return t ? t.textContent.trim() : ''; };
  var prTitle = function () { var t = document.querySelector('.ca-notch-session .ca-prreview .ca-prinsp-title'); return t ? t.textContent.trim() : ''; };
  var prNumber = function () { var t = document.querySelector('.ca-notch-session .ca-prreview .ca-insp-call'); return t ? t.textContent.trim() : ''; };
  var showsSession = function (item) { return function () { return is('session') && sessionTitle().indexOf(item.title.slice(0, 20)) === 0; }; };
  var showsReview = function (pr) { return function () { return is('session') && prNumber() === '#' + pr.number; }; };
  var keysCalls = function () { return calls('notch_keys').map(function (a) { return a.on; }); };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var escape = function () { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true })); };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };

  // Two of the board's sessions wait on you, and two of its PRs ask for your review, whatever
  // the board has now: both files are read here, at once, before the app's script.
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
  var items = f.items.filter(function (i) { return onBoard[i.key]; }).slice(0, 2).map(function (i, n) {
    var group = n === 0 ? 'needs' : 'unread';
    return Object.assign({}, i, { group: group, status: group, asks: false, snoozedUntil: null, since: 1000 + n, hidden: false });
  });
  var prs = (board.prs || []).slice(0, 2).map(function (p, n) {
    return Object.assign({}, p, { state: 'open', reviewRequested: true, requestedBy: 'ana', requestedAt: 2000 + n, snoozedUntil: null, mergedAt: null, closedAt: null });
  });
  var reviews = prs.map(function (p) {
    return { key: p.key, number: p.number, title: p.title, repo: p.repo, repoSlug: p.repoSlug, url: p.url, project: p.project, requestedBy: 'ana', since: p.requestedAt, updatedAt: p.updatedAt };
  });
  var feed = Promise.resolve(Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [], reviews: reviews, todos: [] }));
  var said = {};
  items.forEach(function (i) {
    said[i.key] = { title: i.title, status: i.status, asks: false, snoozedUntil: null, archived: false, live: true, surface: 'cli', hosted: false, pid: 4242, attachCommand: null };
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
        var mine = prs.filter(function (p) { return p.key === x.key; })[0];
        return mine ? Object.assign({}, x, mine) : x;
      });
      return b;
    });
  };

  say('notch review check: waiting for it');
  until(function () { return notch() && calls('notch_shape').length > 0; }, 15000, 'the notch')
    .then(function () { return feed; })
    .then(function (fd) {
      check('has two sessions and two PRs to try', items.length === 2 && prs.length === 2, items.length + ' sessions, ' + prs.length + ' PRs');
      if (items.length < 2 || prs.length < 2) throw new Error('the board needs two sessions and two PRs (' + items.length + ' and ' + prs.length + ')');
      window.__emit('bar-feed', fd);
      enter();
      return until(function () { return is('pills') && reviewsPill(); }, 3000, 'its pills');
    })
    .then(function () { return wait(500); })
    .then(function () {
      check('the reviews pill counts both', /2 to review/.test(reviewsPill().textContent), reviewsPill().textContent);
      // Two reviews: the pill drops a menu to pick one. The stub answers no menu, so the count
      // beside the camera is the way in: it opens the first review.
      var count = document.querySelector('.ca-notch-count.is-reviews');
      check('the reviews count beside the camera is a button', !!count && count.tagName === 'BUTTON');
      count.click();
      return until(showsReview(prs[0]), 4000, 'the first review');
    })
    .then(function () { return wait(700); })
    .then(function () {
      check('a click opens the notch on the PR’s card', is('session') && is('open') && prTitle() === prs[0].title, prTitle());
      check('takes the keyboard', keysCalls().join() === 'true', keysCalls().join());
      check('the cursor is in its box', document.activeElement && document.activeElement.tagName === 'TEXTAREA' && !!document.activeElement.closest('.ca-prreview-box'));
      check('the reviews pill is marked', reviewsPill().classList.contains('is-peeked'));
      var strip = document.querySelector('.ca-notch-session .ca-reply-queue');
      check('says how many more wait: the other review, the finished turn and the one skipped', !!strip && /3 more waiting on you/.test(strip.textContent), strip ? strip.textContent : 'no strip');
      var labels = Array.prototype.map.call(document.querySelectorAll('.ca-notch-session .ca-hotkeys .ca-hotkey'), function (b) { return b.textContent; });
      check('the strip has the PR’s actions with their keys', labels.join('|') === 'Approve⌘⏎|Comment⌥⌘⏎|Snooze⌃⌘1–4|Review with⌃⌘R|GitHub⌃⌘G|Back⌃-|Next⌘]|esc', labels.join('|'));
      check('the card has the details', !!document.querySelector('.ca-notch-session .ca-prreview-body .ca-insp-sec'));
      key({ key: ']', code: 'BracketRight', metaKey: true });
      return until(showsReview(prs[1]), 3000, '⌘] to the next review');
    })
    .then(function () {
      check('⌘] goes on to the next review', true);
      key({ key: ']', code: 'BracketRight', metaKey: true });
      return until(showsSession(items[1]), 3000, '⌘] on to the finished turn');
    })
    .then(function () {
      check('and from the last review to the session whose turn ended', true);
      check('its pill is marked and the reviews pill isn’t', pill(items[1].key).classList.contains('is-peeked') && !reviewsPill().classList.contains('is-peeked'));
      key({ key: 'ArrowUp', altKey: true, shiftKey: true });
      return until(showsReview(prs[1]), 3000, '⌥⇧↑ back to the review');
    })
    .then(function () {
      check('⌥⇧↑ goes back to the review', true);
      key({ key: 'ArrowUp', altKey: true });
      return until(showsReview(prs[0]), 3000, '⌥↑ along the bar');
    })
    .then(function () {
      key({ key: 'ArrowUp', altKey: true });
      return until(showsSession(items[0]), 3000, '⌥↑ to the session that needs you');
    })
    .then(function () {
      check('⌥↑ steps along the bar through reviews and sessions alike', true);
      var note = document.querySelector('.ca-notch-session .ca-reply-queue-note');
      check('on the session the strip counts the reviews among what waits', !!note && /^3 more waiting on you/.test(note.textContent), note ? note.textContent : 'no strip');
      key({ key: 'ArrowDown', altKey: true });
      return until(showsReview(prs[0]), 3000, '⌥↓ back to the review');
    })
    .then(function () { return wait(300); })
    .then(function () {
      // ⌃⌘G opens it on GitHub.
      var before = calls('open_url').length;
      key({ key: 'g', metaKey: true, ctrlKey: true });
      return until(function () { return calls('open_url').length === before + 1; }, 2000, '⌃⌘G').then(function () {
        check('⌃⌘G opens the PR on GitHub', last('open_url').url === prs[0].url, JSON.stringify(last('open_url')));
      });
    })
    .then(function () {
      // ⌃⌘1 snoozes the review for the first of its menu's times, and the notch goes on.
      var before = calls('snooze_reviews').length;
      var at = Date.now();
      key({ key: '1', code: 'Digit1', metaKey: true, ctrlKey: true });
      return until(function () { return calls('snooze_reviews').length === before + 1; }, 2000, '⌃⌘1').then(function () {
        var c = calls('snooze_reviews')[before];
        var mins = (c.until - at) / 60000;
        check('⌃⌘1 snoozes the review for 15 minutes', c.keys.join() === prs[0].key && mins > 14.5 && mins < 15.5, JSON.stringify(c));
        return until(showsReview(prs[1]), 3000, 'on to the next review');
      });
    })
    .then(function () { return wait(400); })
    .then(function () {
      check('and the notch goes on to the next review', true);
      var strip = document.querySelector('.ca-notch-session .ca-reply-queue-note');
      check('the strip says the last one was snoozed', !!strip && /snoozed\.$/.test(strip.textContent), strip ? strip.textContent : 'no strip');
      // A review GitHub refuses (the stub refuses "fail"): the notch goes on at once, then the
      // strip says why, and the PR is back in what's left with its text.
      var box = document.querySelector('.ca-notch-session .ca-prreview-box textarea');
      box.focus();
      type(box, 'fail');
      key({ key: 'Enter', metaKey: true });
      return until(showsSession(items[1]), 3000, 'on at once');
    })
    .then(function () {
      check('⌘⏎ goes on before GitHub answers', true);
      return until(function () { var s = document.querySelector('.ca-notch-session .ca-reply-queue-note'); return !!s && /Didn’t approve #\d+: GitHub said no/.test(s.textContent); }, 3000, 'why it didn’t go');
    })
    .then(function () {
      check('a refused review says why on the strip', true);
      // ⌘] round what's left (the skipped session and the snoozed review first) until it's back.
      var n = 0;
      return (function step() {
        if (showsReview(prs[1])()) return Promise.resolve();
        if (n++ > 4) return Promise.reject(new Error('timeout: the PR back round'));
        key({ key: ']', code: 'BracketRight', metaKey: true });
        return wait(300).then(step);
      })();
    })
    .then(function () { return wait(300); })
    .then(function () {
      var box = document.querySelector('.ca-notch-session .ca-prreview-box textarea');
      check('and the PR comes back round with what was written', !!box && box.value === 'fail', box ? box.value : 'no box');
      box.focus();
      type(box, 'Looks right to me.');
      var before = calls('review_pr').length;
      key({ key: 'Enter', metaKey: true });
      return until(function () { return calls('review_pr').length === before + 1; }, 2000, '⌘⏎').then(function () {
        var c = calls('review_pr')[before];
        check('⌘⏎ approves it with what’s in the box', c.repo === prs[1].repoSlug && c.number === prs[1].number && c.verdict === 'approve' && c.body === 'Looks right to me.', JSON.stringify(c));
        return until(showsSession(items[1]), 3000, 'on to the finished turn');
      });
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('and goes on to the next that waits', true);
      var strip = document.querySelector('.ca-notch-session .ca-reply-queue-note');
      check('the strip says it was approved', !!strip && /approved\.$/.test(strip.textContent), strip ? strip.textContent : 'no strip');
      escape();
      return until(function () { return !is('session'); }, 3000, 'Escape');
    })
    .then(function () {
      check('Escape puts it away and gives the keyboard back', keysCalls()[keysCalls().length - 1] === false, keysCalls().join());
      return wait(300);
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' (showing “' + (prTitle() || sessionTitle()) + '”, ' + (notch() ? notch().className : 'no notch') + ')'); });
})();
