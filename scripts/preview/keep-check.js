// Checks keeping a session on the desktop (Peek.tsx, kept.rs). In the peek: its header has the
// pin and not the buttons its menu covers; the pin keeps the session (keep_session); dragging an
// edge sizes the peek (peek_resize) and letting go keeps it. In a kept session's window: it says
// it's ready (kept_ready), a click takes the keyboard (kept_focus), its edges size it
// (kept_resize), its header drags it, sending leaves it up, and its × closes it (kept_close).
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/keep-check.js
//   PREVIEW_WAIT=60000 scripts/preview.sh kept '' scripts/preview/keep-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var done = function () { say('done: ' + out.join(' · ')); };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  var last = function (cmd) { var c = calls(cmd); return c.length ? c[c.length - 1][1] : null; };
  var kept = window.__CODEALONG_KEPT__ === true;
  var head = function () { return document.querySelector('.ca-peek .ca-sessinsp-head'); };
  var button = function (label) { return document.querySelector('.ca-peek .ca-sessinsp-head button[aria-label="' + label + '"]'); };
  var pointer = function (el, type, x, y, buttons) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: buttons, screenX: x, screenY: y, clientX: x, clientY: y, pointerId: 1, isPrimary: true, pointerType: 'mouse' }));
  };
  var grip = function (edge) { return document.querySelector('.ca-peek-grip.is-' + edge); };
  // setPointerCapture needs a real pointer: the stand-in keeps the drag going.
  Element.prototype.setPointerCapture = function () {};

  say('keep check: waiting for the session');
  until(function () { return !!head(); }, 20000)
    .then(function () { return wait(400); })
    .then(function () {
      check('the header leaves snoozing, archiving and showing in Codealong to its menu', !button('Snooze') && !button('Snoozed') && !button('Archive') && !button('Show in Codealong') && !!button('More'));
      var w = window.innerWidth, h = window.innerHeight;
      if (!kept) {
        check('the peek has the pin, and nothing to close', !!button('Keep on the desktop') && !button('Close'));
        check('and follows its card', !document.querySelector('.ca-peek-root.is-filled'));
        button('Keep on the desktop').click();
        check('the pin keeps the session on the desktop', calls('keep_session').length === 1);
        // Dragging the far edge makes it bigger, and letting go keeps it at that size.
        var far = grip('far');
        pointer(far, 'pointerdown', 200, 500, 1);
        pointer(far, 'pointermove', 200, 501, 1);
        check('an edge pressed without moving sizes nothing', calls('peek_resize').length === 0);
        pointer(far, 'pointermove', 200, 560, 1);
        return wait(100).then(function () {
          var r = last('peek_resize');
          check('dragging its far edge makes it taller', r && r.width === w && r.height === h + 60, JSON.stringify(r));
          check('and the card fills the window from then on', !!document.querySelector('.ca-peek-root.is-filled'));
          check('nothing is kept while the edge is held', calls('keep_session').length === 1);
          pointer(far, 'pointerup', 200, 560, 0);
          return wait(100);
        }).then(function () {
          check('letting go keeps it on the desktop', calls('keep_session').length === 2);
          var corner = grip('corner');
          pointer(corner, 'pointerdown', 480, 500, 1);
          pointer(corner, 'pointermove', 520, 540, 1);
          return wait(100);
        }).then(function () {
          var r = last('peek_resize');
          check('the corner sizes it both ways', r && r.width === w + 40 && r.height === h + 40, JSON.stringify(r));
          pointer(grip('corner'), 'pointerup', 520, 540, 0);
        });
      }
      check('a kept session has the × and no pin', !!button('Close') && !button('Keep on the desktop'));
      check('its page said it was laid out', calls('kept_ready').length >= 1);
      check('and fills its window', !!document.querySelector('.ca-peek-root.is-kept.is-filled'));
      // A click gives it the keyboard.
      pointer(document.querySelector('.ca-peek .ca-thread, .ca-peek .ca-sessinsp-meta') || document.querySelector('.ca-peek'), 'pointerdown', 100, 300, 1);
      check('a click gives it the keyboard', calls('kept_focus').length === 1);
      // Its header drags the window; a button in it doesn't.
      var title = document.querySelector('.ca-peek .ca-sessinsp-title');
      var before = calls('plugin:window|start_dragging').length;
      pointer(title, 'pointerdown', 100, 30, 1);
      pointer(title, 'pointermove', 102, 31, 1);
      check('a press on its header that barely moves drags nothing', calls('plugin:window|start_dragging').length === before);
      pointer(title, 'pointermove', 130, 60, 1);
      check('its header drags the window', calls('plugin:window|start_dragging').length === before + 1);
      var more = button('More');
      pointer(more, 'pointerdown', 500, 30, 1);
      pointer(more, 'pointermove', 540, 70, 1);
      check('a button in the header does not', calls('plugin:window|start_dragging').length === before + 1);
      var side = grip('side');
      pointer(side, 'pointerdown', 600, 300, 1);
      pointer(side, 'pointermove', 650, 300, 1);
      return wait(100).then(function () {
        var r = last('kept_resize');
        check('its right edge makes it wider', r && r.width === w + 50 && r.height === h, JSON.stringify(r));
        pointer(side, 'pointerup', 650, 300, 0);
        return wait(50);
      }).then(function () {
        check('and letting go keeps nothing again', calls('keep_session').length === 0 && calls('peek_resize').length === 0);
        window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
        check('Escape leaves it up', calls('peek_hide').length === 0 && calls('kept_close').length === 0);
        button('Close').click();
        return wait(50);
      }).then(function () {
        check('its × takes it off the desktop', calls('kept_close').length === 1);
      });
    })
    .then(done)
    .catch(function (e) {
      out.push('FAIL ' + e);
      done();
    });
})();
