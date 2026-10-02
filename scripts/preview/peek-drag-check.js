// Checks dragging the peek by its header (Peek.tsx, peek.rs): a press that barely moves drags
// nothing; past that the page hands the drag to the engine once (peek_drag, with how far the
// pointer has gone), which follows the pointer and keeps the session where it's let go, so the
// page itself keeps nothing and sends no more; the card fills its window from then on; and a
// button in the header drags nothing.
//   PREVIEW_WAIT=60000 scripts/preview.sh peek '' scripts/preview/peek-drag-check.js
// Every step should say ok. keep-check.js has the rest of keeping a session.
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
  var head = function () { return document.querySelector('.ca-peek .ca-sessinsp-head'); };
  var button = function (label) { return document.querySelector('.ca-peek .ca-sessinsp-head button[aria-label="' + label + '"]'); };
  var pointer = function (el, type, x, y, buttons) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0, buttons: buttons, screenX: x, screenY: y, clientX: x, clientY: y, pointerId: 1, isPrimary: true, pointerType: 'mouse' }));
  };

  say('peek drag check: waiting for the session');
  until(function () { return !!head(); }, 20000)
    .then(function () { return wait(400); })
    .then(function () {
      check('the peek follows its card', !document.querySelector('.ca-peek-root.is-filled'));
      // A button in the header is pressed, not dragged.
      var more = button('More');
      pointer(more, 'pointerdown', 500, 30, 1);
      pointer(more, 'pointermove', 540, 70, 1);
      pointer(more, 'pointerup', 540, 70, 0);
      check('a button in its header drags nothing', calls('peek_drag').length === 0);
      // Nor is the conversation under it.
      var thread = document.querySelector('.ca-peek .ca-thread') || document.querySelector('.ca-peek .ca-composer');
      pointer(thread, 'pointerdown', 100, 300, 1);
      pointer(thread, 'pointermove', 160, 360, 1);
      pointer(thread, 'pointerup', 160, 360, 0);
      check('nor does the rest of the card', calls('peek_drag').length === 0, thread ? thread.className : 'nothing under the header');
      var title = document.querySelector('.ca-peek .ca-sessinsp-title') || head();
      pointer(title, 'pointerdown', 100, 30, 1);
      check('a press in it takes the keyboard', calls('peek_focus').length >= 1);
      pointer(title, 'pointermove', 102, 31, 1);
      check('a press on its header that barely moves drags nothing', calls('peek_drag').length === 0);
      pointer(title, 'pointermove', 130, 20, 1);
      return wait(100);
    })
    .then(function () {
      var d = last('peek_drag');
      check('its header drags it, saying how far the pointer has gone', calls('peek_drag').length === 1 && d && d.dx === 30 && d.dy === -10, JSON.stringify(d));
      check('and the card fills the window from then on', !!document.querySelector('.ca-peek-root.is-filled'));
      // The engine follows the pointer from there: the page sends no more, and keeps nothing itself.
      var title = document.querySelector('.ca-peek .ca-sessinsp-title') || head();
      pointer(title, 'pointermove', 300, 200, 1);
      pointer(title, 'pointerup', 300, 200, 0);
      return wait(100);
    })
    .then(function () {
      check('the engine takes it from there', calls('peek_drag').length === 1 && calls('keep_session').length === 0 && calls('peek_resize').length === 0);
      check('and the window is not dragged as a kept one is', calls('plugin:window|start_dragging').length === 0);
      check('nor put away', calls('peek_hide').length === 0);
    })
    .then(done)
    .catch(function (e) {
      out.push('FAIL ' + e);
      done();
    });
})();
