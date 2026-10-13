// Checks what the notch says of a message that didn't go once its box had gone (NotchSession.tsx
// `unsent`): after ⌘⏎ went on to the next, the strip over that one says why, and nothing shows on
// the bar; after the notch closed on it, the bar says why on its session's pill.
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=60000 scripts/preview.sh notch '' scripts/preview/notch-unsent-check.js
(function () {
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var is = function (state) { return notch() && notch().classList.contains('is-' + state); };
  var pill = function (key) { return document.querySelector('.ca-notch-body .ca-hud-pill[data-key="' + key + '"]'); };
  var title = function () { var t = document.querySelector('.ca-notch-session .ca-sessinsp-title, .ca-notch-session .ca-sessinsp-head h2'); return t ? t.textContent.trim() : ''; };
  var strip = function () { var t = document.querySelector('.ca-notch-session .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var box = function () { return document.querySelector('.ca-notch-session .ca-composer textarea'); };
  // What the notch's card says on the bar goes as an event to the bar's own window, which the
  // stub records without passing on (Bar.tsx hears it as `bar-note`).
  var barNotes = function () { return calls('plugin:event|emit_to').filter(function (c) { return c[1] && c[1].event === 'bar-note'; }).map(function (c) { return c[1].payload; }); };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };

  // Two of the board's sessions whose turns ended, whatever the board has now, as the notch
  // session check picks them. This runs before the app's script, so both files are read here.
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
  var items = f.items.filter(function (i) { return onBoard[i.key]; }).slice(0, 2).map(function (i, n) {
    return Object.assign({}, i, { title: 'Unsent ' + 'AB'[n], group: 'unread', status: 'unread', asks: false, snoozedUntil: null, since: 1000 + n, hidden: false });
  });
  var feed = Object.assign({}, f, { loading: false, items: items, recent: [], snoozed: [], reviews: [], todos: [] });
  var said = {};
  items.forEach(function (i) {
    said[i.key] = { title: i.title, status: 'unread', asks: false, snoozedUntil: null, archived: false, live: true, surface: 'cli', hosted: false, pid: 4242, attachCommand: null };
  });
  var flying = [];
  var fail = function () { if (flying.length) flying.shift()('Its terminal is gone.'); };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    // Every message fails, when the check says: once its box has gone.
    if (cmd === 'send_to_session') {
      window.__calls.push([cmd, args, options]);
      return new Promise(function (_, reject) { flying.push(reject); });
    }
    var p = real.apply(this, arguments);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      b.lanes.forEach(function (l) {
        l.sessions = l.sessions.map(function (s) { return said[s.key] && !s.visitor ? Object.assign({}, s, said[s.key]) : s; });
      });
      return b;
    });
  };
  var shows = function (i) { return function () { return is('session') && title() === items[i].title; }; };
  var send = function (meta) {
    box().focus();
    type(box(), 'Run the tests again');
    box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: meta, bubbles: true, cancelable: true }));
  };

  say('notch unsent check: waiting for it');
  until(function () { return notch() && calls('notch_shape').length > 0; }, 15000, 'the notch')
    .then(function () {
      if (items.length < 2) throw new Error('the bar has fewer than two of the board’s sessions');
      window.__emit('bar-feed', feed);
      enter();
      return until(function () { return is('pills') && pill(items[0].key); }, 3000, 'its pills');
    })
    .then(function () { return wait(500); })
    .then(function () {
      pill(items[0].key).click();
      return until(shows(0), 3000, 'the first session').then(function () { return until(box, 3000, 'its box'); });
    })
    .then(function () {
      send(true);
      return until(shows(1), 3000, 'the next');
    })
    .then(function () {
      fail();
      return until(function () { return /Didn’t send/.test(strip()); }, 3000).catch(function () {});
    })
    .then(function () {
      check('a message that didn’t go after ⌘⏎ went on is said on the strip of the one gone on to', strip() === 'Didn’t send to Unsent A: Its terminal is gone.', strip());
      check('and not on the bar', barNotes().length === 0, JSON.stringify(barNotes()));
      return until(box, 3000, 'the next one’s box');
    })
    .then(function () {
      send(false);
      return wait(50);
    })
    .then(function () {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }));
      // The card stays while the notch closes over it (NotchSession.tsx `LINGER_MS`).
      return until(function () { return !is('session') && !box(); }, 3000, 'the notch closing');
    })
    .then(function () {
      fail();
      return until(function () { return barNotes().length > 0; }, 3000).catch(function () {});
    })
    .then(function () {
      var n = barNotes();
      check('one that didn’t go after the notch closed on it is said on the bar, by its pill', n.length === 1 && n[0].key === items[1].key && n[0].message === 'Its terminal is gone.' && n[0].tone === 'error', JSON.stringify(n));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
