// Checks the zoom in the windows that float (zoom.ts `cardZoom`, SessionCard.tsx, cardHost.ts
// `zoomKey`): the card of the session shown draws at the board's zoom and still fills its window
// across; ⌘= and ⌘- from its box ask the engine to zoom, as in the board's windows; the notch
// opens its session as much wider (bar.ts `notchSizes`), and a step while it's up resizes the
// shape at once, with no easing, in the window it had (Notch.tsx `zooming`). The board and the bar's feed here say
// 125%; the stub's answers to a step are fake-tauri.js's. A peek's window opening bigger is the
// engine's (peek.rs `place`), which the preview doesn't size.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/card-zoom-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/card-zoom-check.js
//   PREVIEW_SIZE=1512,980 PREVIEW_WAIT=90000 scripts/preview.sh notch '' scripts/preview/card-zoom-check.js
(function () {
  var inNotch = window.__CODEALONG_BAR__ === true;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board' && cmd !== 'bar_feed') return p;
    return p.then(function (it) {
      if (it) it.zoom = 125;
      return it;
    });
  };

  var card = function () { return document.querySelector('.ca-reply'); };
  var box = function () { return document.querySelector('.ca-reply .ca-composer textarea'); };
  var steps = function () { return sent('zoom').map(function (a) { return a.step; }).join(' '); };
  var key = function (init, el) { el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true, metaKey: true }, init))); };

  // The notch opens on a session as notch-session-check.js has it: the bar shows only this Mac's
  // own sessions on the board, the pointer comes onto the shape, and a click on a pill shows its
  // session. This runs before the app's script, so the files are read here, at once.
  var opened = Promise.resolve();
  if (inNotch) {
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
    var items = f.items.filter(function (i) { return onBoard[i.key]; }).slice(0, 3).map(function (i) {
      return Object.assign({}, i, { group: 'unread', status: 'unread', asks: false, snoozedUntil: null, hidden: false });
    });
    var feed = Object.assign({}, f, { zoom: 125, loading: false, items: items, recent: [], snoozed: [], reviews: [], todos: [] });
    var notch = function () { return document.querySelector('.ca-notch'); };
    var pill = function (k) { return document.querySelector('.ca-notch-body .ca-hud-pill[data-key="' + k + '"]'); };
    opened = until(function () { return notch() && calls('notch_shape').length > 0; }, 15000, 'the notch')
      .then(function () {
        if (items.length === 0) throw new Error('no session of this Mac’s on the bar');
        window.__emit('bar-feed', feed);
        document.querySelector('.ca-notch-shape').dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
        return until(function () { return notch().classList.contains('is-pills') && pill(items[0].key); }, 3000, 'its pills');
      })
      .then(function () { pill(items[0].key).click(); });
  }

  say('card zoom check: waiting for a session’s card');
  opened
    .then(function () { return until(function () { return !!card() && !!box(); }, 30000, 'a session’s card'); })
    .then(function () { return wait(700); })
    .then(function () {
      var c = card();
      var z = getComputedStyle(c).zoom;
      check('the card draws at the board’s zoom', z === '1.25', 'zoom ' + z);
      var host = c.parentElement.getBoundingClientRect().width;
      var drawn = c.getBoundingClientRect().width;
      check('and still fills its window across', Math.abs(host - drawn) < 1.5, Math.round(drawn) + ' of ' + Math.round(host));
      if (inNotch) {
        var w = document.querySelector('.ca-notch-card').getBoundingClientRect().width;
        check('the notch opens its session as much wider', w > 680 * 1.2, Math.round(w) + ' wide');
      }
      box().focus();
      key({ key: '=', code: 'Equal' }, box());
      key({ key: '-', code: 'Minus' }, box());
      return wait(300);
    })
    .then(function () {
      check('⌘= and ⌘- from its box ask the engine to zoom', steps() === 'in out', steps());
      if (!inNotch) return;
      // A step while it's up: the shape takes its new size with no easing, in the window it had.
      var fits = function () { return sent('fit_bar').map(function (a) { return a.width + 'x' + a.height; }); };
      var windowBefore = fits().pop();
      var classes = [];
      var watch = new MutationObserver(function () { classes.push(document.querySelector('.ca-notch').className); });
      watch.observe(document.querySelector('.ca-notch'), { attributes: true, attributeFilter: ['class'] });
      window.__emit('bar-feed', Object.assign({}, feed, { zoom: 150 }));
      return until(function () { return getComputedStyle(card()).zoom === '1.5'; }, 3000, 'the card at 150%')
        .then(function () { return wait(400); })
        .then(function () {
          watch.disconnect();
          var eased = getComputedStyle(document.querySelector('.ca-notch-shape')).transitionProperty;
          check('a zoom step resizes the shape with no easing, and the easing comes back', classes.some(function (c) { return /is-zooming/.test(c); }) && !/is-zooming/.test(document.querySelector('.ca-notch').className) && eased !== 'none', classes.join(' → ') + '; then ' + eased);
          check('in the window it had', fits().pop() === windowBefore, windowBefore + ' → ' + fits().pop());
        });
    })
    .then(done)
    .catch(stopped);
})();
