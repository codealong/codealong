// Checks that only a pointer that moved points at a pill on the floating bar (Bar.tsx): when the
// pills shift as sessions change groups, the one that slides under a still pointer doesn't open
// the peek, and a move on it does. The bar shows three sessions of the check's own in place of
// the feed's, so it has pills to try whatever this Mac's agents are doing.
//   PREVIEW_WAIT=60000 scripts/preview.sh bar '' scripts/preview/bar-slide-check.js
(function () {
  // Hovering asks nothing of the engine but the peek, so the sessions need be on no board.
  var made = ['needs', 'unread', 'unread'].map(function (group, n) {
    var key = 'claude:claude:bar-slide-check-' + n;
    return { key: key, group: group, status: group, asks: false, title: 'Bar slide session ' + (n + 1), account: 'claude', laneId: '/tmp/bar-slide-check', callsign: 'bar-slide-check', project: '', hidden: false, detail: null, since: Date.now() - (n + 1) * 60e3, openTarget: 's|' + key + '|focus', target: null, snoozedUntil: null, stoppable: false };
  });
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'bar_feed') return p;
    return p.then(function (feed) { return Object.assign({}, feed, { loading: false, items: made, recent: [], snoozed: [], reviews: [], todos: [] }); });
  };
  var opens = function () { return window.__calls.filter(function (c) { return c[0] === 'peek_open'; }); };
  var at = function (type, el, x, y, from) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, screenX: x, screenY: y, clientX: x, clientY: y, relatedTarget: from || null, pointerType: 'mouse' }));
  };

  say('bar slide check: waiting for pills');
  until(function () { return document.querySelectorAll('.ca-hud [data-key]').length >= 2; }, 20000)
    .then(function () {
      var hud = document.querySelector('.ca-hud');
      var pills = document.querySelectorAll('.ca-hud [data-key]');
      check('the bar shows the check’s sessions', pills.length === made.length && made.every(function (m, n) { return pills[n].dataset.key === m.key; }), Array.prototype.map.call(pills, function (p) { return p.dataset.key; }).join(', '));
      var a = pills[0];
      var b = pills[1];
      // The pointer rests on pill a; then b slides under it, which WebKit may report as the
      // pointer coming onto b and moving on it, at the same place.
      at('pointerover', a, 200, 20, hud);
      at('pointermove', a, 200, 20);
      return wait(400).then(function () {
        check('resting on a pill opens its peek', opens().length === 1 && opens()[0][1].key === a.dataset.key, JSON.stringify(opens().map(function (c) { return c[1].key; })));
        at('pointerout', a, 200, 20, b);
        at('pointerover', b, 200, 20, a);
        at('pointermove', b, 200, 20);
        return wait(400);
      }).then(function () {
        check('a pill sliding under a still pointer doesn’t take the peek', opens().length === 1, String(opens().length));
        at('pointermove', b, 203, 21);
        return wait(400);
      }).then(function () {
        check('moving on it does', opens().length === 2 && opens()[1][1].key === b.dataset.key, JSON.stringify(opens().map(function (c) { return c[1].key; })));
        // Out of the bar and back to the same place: coming onto a pill from outside points at it.
        at('pointerout', b, 203, 21, null);
        at('pointerleave', hud, 203, 21, null);
        at('pointerover', a, 203, 21, null);
        return wait(400);
      }).then(function () {
        check('coming back onto the bar points at the pill', opens().length === 3 && opens()[2][1].key === a.dataset.key, String(opens().length));
        done();
      });
    })
    .catch(stopped);
})();
