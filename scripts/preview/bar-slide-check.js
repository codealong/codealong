// Checks that only a pointer that moved points at a pill on the floating bar (Bar.tsx): when the
// pills shift as sessions change groups, the one that slides under a still pointer doesn't open
// the peek, and a move on it does.
//   PREVIEW_WAIT=60000 scripts/preview.sh bar '' scripts/preview/bar-slide-check.js
(function () {
  var opens = function () { return window.__calls.filter(function (c) { return c[0] === 'peek_open'; }); };
  var at = function (type, el, x, y, from) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, screenX: x, screenY: y, clientX: x, clientY: y, relatedTarget: from || null, pointerType: 'mouse' }));
  };

  say('bar slide check: waiting for pills');
  until(function () { return document.querySelectorAll('.ca-hud [data-key]').length >= 2; }, 20000)
    .then(function () {
      var hud = document.querySelector('.ca-hud');
      var pills = document.querySelectorAll('.ca-hud [data-key]');
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
