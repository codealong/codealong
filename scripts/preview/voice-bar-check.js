// Checks what the floating bar shows of a session that's hearing (Bar.tsx `ListeningPill`,
// ui/Said.tsx `SaidLine`): the mark's center says one is, the session has a pill of its own, the
// first, and a click on it shows the session in Codealong's window; a coach's cue takes the
// pills' place for as long as it shows, and a click on it shows the session too, so the way to
// the session is never gone; when nothing hears, the pill and the mark's light go; and a meeting
// heard ahead of its session (Listen now), which is still starting, has the light and a pill
// under its own name, with nothing to show for a click. The bar shows sessions of the check's
// own in place of the feed's, which need be on no board.
//   PREVIEW_WAIT=30000 scripts/preview.sh bar '' scripts/preview/voice-bar-check.js
(function () {
  var now = Date.now();
  var made = function (n, group) {
    var key = 'claude:claude:voice-bar-check-' + n;
    return { key: key, group: group, status: group, asks: false, title: 'Voice bar session ' + (n + 1), account: 'claude', laneId: '/tmp/voice-bar-check', callsign: 'voice-bar-check', project: '', hidden: false, detail: null, since: now - (n + 1) * 60e3, openTarget: 's|' + key + '|focus', target: null, snoozedUntil: null, stoppable: false };
  };
  var items = [made(0, 'needs'), made(1, 'unread')];
  var HEARS = 'claude:claude:voice-bar-check-hears';
  var listening = function (over) {
    return Object.assign({ key: HEARS, laneId: '/tmp/voice-bar-check', title: 'Renewal call', id: 'a', startedAt: Date.now() - 247e3, role: 'coach', mic: true, playback: true, transcript: '', notes: '', cueAt: 0 }, over);
  };
  var feed = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'bar_feed') return p;
    return p.then(function (f) {
      feed = Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [], reviews: [], todos: [], listenings: [] });
      return feed;
    });
  };
  var hears = function (listenings) { window.__emit('bar-feed', Object.assign({}, feed, { listenings: listenings })); };
  var q = function (sel) { return document.querySelector(sel); };
  var pill = function () { return q('.ca-hud > .ca-hud-pill.is-hearing'); };
  var cue = function () { return q('.ca-hud > .ca-hud-spoken.is-cue'); };
  var mark = function () { return q('.ca-hud .ca-hud-mark .ca-lmark[data-hearing]'); };
  // What the last show of Codealong's window was asked to reveal, after `from` of them.
  var shown = function (from) { var c = calls('show_main')[from]; return c && c[1].reveal; };
  var isIt = function (r) { return !!r && r.kind === 'session' && r.key === HEARS && r.laneId === '/tmp/voice-bar-check'; };
  var shows = 0;

  say('voice bar check: waiting for pills');
  until(function () { return feed && q('.ca-hud > [data-key]'); }, 20000, 'the bar’s pills')
    .then(function () { return wait(300); })
    .then(function () {
      check('with nothing hearing, no pill says so, nor the mark', !pill() && !mark());
      hears([listening({})]);
      return until(function () { return pill() && mark(); }, 3000, 'the pill of the session that hears');
    })
    .then(function () {
      var first = q('.ca-hud > .ca-hud-pill');
      check('a session that hears has a pill of its own, the first', pill() === first && first.textContent.trim() === 'Renewal call' && !!first.querySelector('.ca-live-dot'), first ? first.textContent : 'no pill');
      check('beside the sessions that wait on you', document.querySelectorAll('.ca-hud > [data-key]').length === 3, String(document.querySelectorAll('.ca-hud > [data-key]').length));
      check('and the mark says one is hearing', (q('.ca-hud .ca-hud-mark').getAttribute('title') || '').indexOf('Renewal call is listening') === 0, q('.ca-hud .ca-hud-mark').getAttribute('title'));
      var hud = q('.ca-hud').getBoundingClientRect();
      check('all of it on the bar', pill().getBoundingClientRect().right <= hud.right && hud.width <= window.innerWidth + 1, JSON.stringify([Math.round(hud.width), window.innerWidth]));
      shows = calls('show_main').length;
      pill().click();
      return until(function () { return calls('show_main').length > shows; }, 2000, 'a click on it to show the session');
    })
    .then(function () {
      check('a click on it shows the session in Codealong', isIt(shown(shows)), JSON.stringify(shown(shows)));
      // A coach's cue has the pills' place while it shows: the way to the session is the cue.
      hears([listening({ cue: 'Ask when they renew.', cueAt: Date.now() })]);
      return until(cue, 3000, 'the cue');
    })
    .then(function () {
      check('a cue takes the pills’ place', !pill() && !q('.ca-hud > [data-key]') && cue().textContent.indexOf('Renewal call') === 0 && cue().textContent.indexOf('Ask when they renew.') > 0, cue().textContent);
      shows = calls('show_main').length;
      cue().click();
      return until(function () { return calls('show_main').length > shows; }, 2000, 'a click on the cue to show the session');
    })
    .then(function () {
      check('and a click on it shows the session too', isIt(shown(shows)), JSON.stringify(shown(shows)));
      hears([]);
      return until(function () { return !pill() && !cue() && !mark() && q('.ca-hud > [data-key]'); }, 3000, 'the pill and the light to go');
    })
    .then(function () {
      check('when nothing hears, the pill and the mark’s light go', true);
      // Heard ahead of its session (barfeed.rs `BarListening`): no key or worktree yet.
      hears([listening({ key: '', laneId: '', title: 'On-site with Kestrel', starting: 'On-site with Kestrel', role: 'notes', startedAt: Date.now() - 3e3 })]);
      return until(function () { return pill() && mark(); }, 3000, 'the pill of a meeting whose session is starting');
    })
    .then(function () {
      var says = q('.ca-hud .ca-hud-mark').getAttribute('title') || '';
      check('a meeting whose session is still starting has the light, under its name', pill().textContent.trim() === 'On-site with Kestrel' && says.indexOf('On-site with Kestrel is being heard, and its session is starting') === 0, says);
      check('and its pill says nothing of showing it', (pill().getAttribute('title') || '').indexOf('Show it') < 0, pill().getAttribute('title'));
      shows = calls('show_main').length;
      pill().click();
      return wait(400);
    })
    .then(function () { check('a click on it shows nothing yet', calls('show_main').length === shows, String(calls('show_main').length - shows)); })
    .then(done)
    .catch(stopped);
})();
