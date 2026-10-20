// Checks a PR shown in the notch from a session's PR chip (NotchSession.tsx, NotchReview.tsx,
// PrCard.tsx `from`): a click on the chip shows the PR's card in the session's place rather than
// the main window, with a link back to the session, the box saying a verdict goes back, and the
// strip without Snooze, since the PR asks no review of yours; its menu still shows it in
// Codealong; the link goes back to the session; ⌃- goes back too and ⌃⇧- forward to the PR again,
// which the bar doesn't list; and ⌘⏎ approves it and goes back to the session rather than on to
// the one that waited longer, the strip saying there how the PR went.
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=150000 scripts/preview.sh notch '' scripts/preview/notch-pr-check.js
(function () {
  var calls = sent;
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var pill = function (key) { return document.querySelector('.ca-notch-body .ca-hud-pill[data-key="' + key + '"]'); };
  var sessionTitle = function () { var t = document.querySelector('.ca-notch-session .ca-sessinsp-title'); return t ? t.textContent.trim() : ''; };
  var prNumber = function () { var t = document.querySelector('.ca-notch-session .ca-prreview .ca-insp-call'); return t ? t.textContent.trim() : ''; };
  var backLink = function () { return document.querySelector('.ca-notch-session .ca-prreview .ca-panelback'); };
  var stripNote = function () { var s = document.querySelector('.ca-notch-session .ca-reply-queue-note'); return s ? s.textContent : ''; };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var escape = function () { window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true })); };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };

  // A session on the board that worked on one of its PRs needs you, and another session waited on
  // you longer, so going on would go there; the PR asks no review of yours and the bar lists no
  // reviews. Both files are read here, at once, before the app's script.
  var json = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var board = json('./board.json');
  var homes = {};
  board.lanes.forEach(function (l) {
    l.sessions.forEach(function (s) { if (!s.visitor && !s.archived && s.key.charAt(0) !== '@') homes[l.id + '::' + s.key] = { lane: l, session: s }; });
  });
  var pr = null;
  var at = null;
  (board.prs || []).some(function (p) {
    var found = p.sessions.map(function (x) { return homes[x.lane + '::' + x.key]; }).filter(Boolean)[0];
    if (found) {
      pr = p;
      at = found;
    }
    return !!found;
  });
  var f = json('./bar-feed.json');
  var other = at && f.items.filter(function (i) { return i.key !== at.session.key && Object.keys(homes).some(function (k) { return k.split('::')[1] === i.key; }); })[0];
  var items = at && other ? [
    Object.assign({}, other, { group: 'needs', status: 'needs', asks: false, snoozedUntil: null, since: 500, hidden: false }),
    Object.assign({}, other, { key: at.session.key, title: at.session.title, account: at.session.account, laneId: at.lane.id, callsign: at.lane.callsign, project: at.lane.project, openTarget: at.session.openTarget, target: null, detail: null, group: 'needs', status: 'needs', asks: false, snoozedUntil: null, since: 1000, hidden: false }),
  ] : [];
  var feed = Promise.resolve(Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [], reviews: [], todos: [] }));
  var said = {};
  items.forEach(function (i) {
    said[i.key] = { title: i.title, status: 'needs', asks: false, snoozedUntil: null, archived: false, live: true, surface: 'cli', hosted: false, pid: 4242, attachCommand: null };
  });
  var asked = { state: 'open', mine: false, author: 'ana', reviewRequested: false, requestedBy: null, requestedAt: null, snoozedUntil: null, mergedAt: null, closedAt: null };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var p = real.apply(this, arguments);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions = l.sessions.map(function (s) { return said[s.key] && !s.visitor ? Object.assign({}, s, said[s.key]) : s; });
      });
      b.prs = b.prs.map(function (x) { return pr && x.key === pr.key ? Object.assign({}, x, asked) : x; });
      return b;
    });
  };

  var showsSession = function () { return is('session') && sessionTitle().indexOf(at.session.title.slice(0, 20)) === 0; };
  var showsPr = function () { return is('session') && prNumber() === '#' + pr.number; };
  var chip = function () {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-notch-session .ca-prchipcard button.ca-pr'), function (b) { return b.textContent.indexOf('#' + pr.number) >= 0; })[0];
  };
  var openPr = function () {
    var c = chip();
    if (!c) throw new Error('the session’s header has no chip for #' + pr.number);
    c.click();
    return until(showsPr, 3000, 'the PR from its chip');
  };

  say('notch PR check: waiting for it');
  until(function () { return notch() && calls('notch_shape').length > 0; }, 15000, 'the notch')
    .then(function () { return feed; })
    .then(function (fd) {
      check('has a session with a PR, and another session', items.length === 2, pr ? 'no other session on the board' : 'no PR on the board with a session here');
      if (items.length < 2) throw new Error('the board needs a PR one of its sessions worked on, and another session');
      window.__emit('bar-feed', fd);
      enter();
      return until(function () { return is('pills') && pill(at.session.key); }, 3000, 'its pills');
    })
    .then(function () { return wait(500); })
    .then(function () {
      pill(at.session.key).click();
      return until(showsSession, 3000, 'the session');
    })
    // Long enough to count as looked at, for ⌃- (history.ts `DWELL_MS`).
    .then(function () { return wait(1300); })
    .then(openPr)
    .then(function () { return wait(500); })
    .then(function () {
      check('a click on the session’s PR chip shows the PR in the notch', showsPr(), prNumber());
      check('not in the main window', calls('show_main').length === 0, JSON.stringify(calls('show_main')));
      check('still has the keyboard', calls('notch_keys').every(function (a) { return a.on; }), JSON.stringify(calls('notch_keys')));
      var back = backLink();
      check('its card leads back to the session', !!back && at.session.title.indexOf(back.textContent.replace('…', '').trim().slice(0, 12)) === 0, back ? back.textContent : 'no link back');
      var labels = Array.prototype.map.call(document.querySelectorAll('.ca-notch-session .ca-hotkeys .ca-hotkey'), function (b) { return b.textContent; });
      check('the strip has the PR’s actions, without Snooze for a review nobody asked of you', labels[0] === 'Approve⌘⏎' && labels[1] === 'Comment⌥⌘⏎' && !labels.some(function (l) { return l.indexOf('Snooze') === 0; }), labels.join('|'));
      var card = document.querySelector('.ca-notch-session .ca-prreview');
      check('the box says a verdict goes back', /approves and goes back/.test(card.textContent), card.textContent.slice(-300));
      return menuOf(document.querySelector('.ca-notch-session .ca-prreview .ca-prinsp-title'));
    })
    .then(function (labels) {
      check('its menu still shows it in Codealong', labels.indexOf('Show in Codealong') >= 0, labels.join(' | '));
      backLink().click();
      return until(showsSession, 3000, 'the link back');
    })
    .then(function () {
      check('the link goes back to the session', showsSession(), sessionTitle());
      check('its pill is marked again', pill(at.session.key).classList.contains('is-peeked'));
      return wait(300).then(openPr);
    })
    .then(function () { return wait(1300); })
    .then(function () {
      key({ key: '-', code: 'Minus', ctrlKey: true });
      return until(showsSession, 3000, '⌃- back to the session');
    })
    .then(function () {
      key({ key: '_', code: 'Minus', ctrlKey: true, shiftKey: true });
      return until(showsPr, 3000, '⌃⇧- forward to the PR the bar doesn’t list');
    })
    .then(function () {
      check('⌃- goes back to the session and ⌃⇧- forward to the PR', true);
      check('and the PR still leads back', !!backLink());
      return wait(300);
    })
    .then(function () {
      var box = document.querySelector('.ca-notch-session .ca-prreview-box textarea');
      box.focus();
      type(box, 'Looks right to me.');
      var before = calls('review_pr').length;
      key({ key: 'Enter', metaKey: true });
      return until(function () { return calls('review_pr').length === before + 1; }, 2000, '⌘⏎').then(function () {
        var c = calls('review_pr')[before];
        check('⌘⏎ approves it with what’s in the box', c.repo === pr.repoSlug && c.number === pr.number && c.verdict === 'approve' && c.body === 'Looks right to me.', JSON.stringify(c));
        return until(showsSession, 3000, 'back to the session');
      });
    })
    .then(function () { return wait(300); })
    .then(function () {
      check('and goes back to the session, not on to the one that waited longer', showsSession(), sessionTitle());
      check('the strip says there how the PR went', new RegExp('#' + pr.number + ': approved\\.$').test(stripNote()), stripNote());
      escape();
      return until(function () { return !is('session'); }, 3000, 'Escape');
    })
    .then(function () {
      check('Escape puts it away', true);
      check('nothing went to the main window', calls('show_main').length === 0, JSON.stringify(calls('show_main')));
    })
    .then(done)
    .catch(stopped);
})();
