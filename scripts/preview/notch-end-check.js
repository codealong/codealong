// Checks the end of what waits in the notch (NotchSession.tsx `end`, QueueEnd.tsx): with nothing
// waiting, though a session runs, the shortcut opens the notch on it, under the pills and only as
// tall as it needs, with the keyboard, rather than the waiting window or the session running;
// pressed again it's put away. What comes to wait
// on you while it's up takes its place by itself, and gone on from the last one (⌘E marks it
// seen) the end says nothing else waits and how that one went; Escape puts it away.
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/notch-end-check.js
(function () {
  var calls = sent;
  var notch = function () { return document.querySelector('.ca-notch'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var end = function () { var e = document.querySelector('.ca-notch-session .ca-queue-end'); return e ? e.textContent : ''; };
  var title = function () { var t = document.querySelector('.ca-notch-session .ca-sessinsp-title, .ca-notch-session .ca-sessinsp-head h2'); return t ? t.textContent.trim() : ''; };
  var keysCalls = function () { return calls('notch_keys').map(function (a) { return a.on; }); };
  var lastShape = function () { var s = calls('notch_shape'); return s[s.length - 1]; };
  var press = function () { window.__emit('waiting-shortcut', null); };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };

  // One of the board's own sessions, whose turn ended: the card reads it from the board
  // (session_at), so it's said there to be so, running in a terminal of its own.
  var json = function (url) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', url, false);
    xhr.send();
    return JSON.parse(xhr.responseText);
  };
  var onBoard = {};
  json('./board.json').lanes.forEach(function (l) {
    l.sessions.forEach(function (s) { if (!s.visitor && !s.archived && s.key.charAt(0) !== '@') onBoard[s.key] = true; });
  });
  var f = json('./bar-feed.json');
  var mine = f.items.filter(function (i) { return onBoard[i.key]; });
  var item = mine.slice(0, 1).map(function (i) {
    return Object.assign({}, i, { group: 'unread', status: 'unread', asks: false, snoozedUntil: null, since: Date.now() - 60e3, hidden: false });
  })[0];
  // And one that runs, which waits on nobody.
  var running = Object.assign({}, mine[1] || f.items[1] || f.items[0], { group: 'working', status: 'working', asks: false, snoozedUntil: null, since: Date.now() - 120e3, hidden: false });
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd) {
    var p = real.apply(this, arguments);
    if (cmd !== 'get_board' || !item) return p;
    return p.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions = l.sessions.map(function (s) {
          return s.key === item.key && !s.visitor ? Object.assign({}, s, { title: item.title, status: 'unread', asks: false, snoozedUntil: null, archived: false, live: true, surface: 'cli', hosted: false, pid: 4242, attachCommand: null }) : s;
        });
      });
      return b;
    });
  };
  var feedWith = function (items) { return Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [], reviews: [], todos: [] }); };

  say('notch end check: waiting for it');
  until(function () { return notch(); }, 20000, 'the notch')
    .then(function () {
      check('has a session of the board’s to try, and one to run', !!item && !!running && running.key !== item.key);
      window.__emit('bar-feed', feedWith([running]));
      return wait(500);
    })
    .then(function () {
      press();
      return until(function () { return is('end') && /Nothing waits on you/.test(end()); }, 3000, 'the end of what waits');
    })
    .then(function () { return wait(500); })
    .then(function () {
      check('with nothing waiting the shortcut opens the notch on the end of what waits, not the one running', is('end') && is('open'), notch().className);
      check('which says nothing waits, and no more', /Nothing waits on you/.test(end()) && !/else/.test(end()), end());
      check('with the keyboard', keysCalls().join() === 'true', keysCalls().join());
      check('the waiting window stays out of it', calls('show_waiting').length === 0, JSON.stringify(calls('show_waiting')));
      var s = lastShape();
      var box = document.querySelector('.ca-notch-end-box').getBoundingClientRect();
      var top = document.querySelector('.ca-notch-session').getBoundingClientRect().top;
      check('the shape opens only as tall as the end needs', !!s && Math.abs(s.h - (top + box.height + 1)) <= 2 && s.h < 300, JSON.stringify(s) + ' for an end ' + Math.round(box.height) + ' tall at ' + Math.round(top));
      check('and as wide as the end is', !!s && Math.abs(s.w - box.width) <= 1, JSON.stringify(s) + ' for an end ' + Math.round(box.width) + ' wide');
      press();
      return until(function () { return !is('open'); }, 3000, 'put away');
    })
    .then(function () { return wait(500); })
    .then(function () {
      check('pressed again it’s put away and the keyboard given back', !is('end') && keysCalls().join() === 'true,false', keysCalls().join());
      press();
      return until(function () { return is('end'); }, 3000, 'the end again');
    })
    .then(function () {
      // A session's turn ends while the end is up.
      window.__emit('bar-feed', feedWith([item, running]));
      return until(function () { return is('session') && title().indexOf(item.title.slice(0, 20)) === 0; }, 3000, 'the session that came to wait');
    })
    .then(function () { return wait(400); })
    .then(function () {
      check('what comes to wait on you takes the end’s place by itself', is('session'), title() || end());
      key({ key: 'e', metaKey: true });
      return until(function () { return is('end'); }, 3000, 'the end after the last one');
    })
    .then(function () { return wait(400); })
    .then(function () {
      var seen = calls('mark_seen');
      check('⌘E marks the last one seen', seen.length === 1 && JSON.stringify(seen[0]).indexOf(item.key) >= 0, JSON.stringify(seen));
      check('and the end says nothing else waits', /Nothing else waits on you/.test(end()), end());
      check('and how the last one went', end().indexOf(item.title + ': marked seen.') >= 0, end());
      check('it keeps the keyboard', keysCalls()[keysCalls().length - 1] === true, keysCalls().join());
      check('the one marked seen doesn’t come straight back', is('end'), notch().className);
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }));
      return until(function () { return !is('open'); }, 3000, 'put away by Escape');
    })
    .then(function () {
      check('Escape puts it away and gives the keyboard back', keysCalls()[keysCalls().length - 1] === false, keysCalls().join());
      check('the waiting window never opened', calls('show_waiting').length === 0, JSON.stringify(calls('show_waiting')));
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' (' + (notch() ? notch().className : 'no notch') + ')'); });
})();
