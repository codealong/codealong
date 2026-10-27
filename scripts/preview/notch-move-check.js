// Checks dragging the notch to another screen (Notch.tsx): closed, a press anywhere on it that
// moves far enough hands the drag to the engine (notch_drag, which takes the docked bar along the
// top of the screens) and one that barely moves is still a click; the click a drag ends in does
// nothing; the notch stays closed until the engine says it's docked. Open, the pills, the
// buttons and the session keep their presses, and only the black around the camera drags it. A
// drag the engine doesn't start doesn't hold the notch shut.
//   PREVIEW_WAIT=40000 scripts/preview.sh notch '' scripts/preview/notch-move-check.js
(function () {
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var isOpen = function () { return notch() && notch().classList.contains('is-open'); };
  var isClosed = function () { return notch() && notch().classList.contains('is-closed'); };
  var drags = function () { return calls('notch_drag').length; };
  var pointer = function (el, type, x, y, buttons) {
    el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: type === 'pointerdown' ? 0 : -1, buttons: buttons, screenX: x, screenY: y, clientX: x, clientY: y, pointerType: 'mouse' }));
  };
  var click = function (el) { el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 })); };
  // Pressed at 400 across, moved to `x` with the button down, and let go with a click.
  var press = function (el, xs) {
    pointer(el, 'pointerdown', 400, 10, 1);
    xs.forEach(function (x) { pointer(el, 'pointermove', x, 10, 1); });
  };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };
  var leave = function () { shape().dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body })); };
  var settle = function () { leave(); return until(function () { return !isOpen(); }, 2000, 'the notch to close'); };

  say('notch move check: waiting for it');
  until(function () { return notch() && calls('notch_shape').length > 0 && document.querySelector('.ca-notch-count'); }, 15000, 'the notch')
    .then(function () { return wait(300); })
    .then(function () {
      var count = document.querySelector('.ca-notch-count');
      press(count, [403, 405]);
      click(count);
      check('closed, a press that barely moves is no drag', drags() === 0);
      return until(isOpen, 1000).then(function () { check('and still a click, which opens it', true); });
    })
    .then(function () {
      // Put away whatever the click opened.
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }));
      return wait(300).then(settle);
    })
    .then(function () {
      var count = document.querySelector('.ca-notch-count');
      press(count, [404, 430]);
      var d = last('notch_drag');
      check('closed, one that moves far enough drags the notch from anywhere on it', drags() === 1 && d && d.dx === 30, JSON.stringify(d));
      pointer(count, 'pointermove', 600, 10, 1);
      check('once', drags() === 1);
      click(count);
      return wait(400);
    })
    .then(function () {
      check('the click it ends in opens nothing', !isOpen() && isClosed());
      enter();
      return wait(600);
    })
    .then(function () {
      check('and it stays closed while it moves, the pointer on it or not', isClosed());
      window.__emit('notch-drag-end', null);
      return until(isOpen, 1500, 'the notch to open under the pointer');
    })
    .then(function () {
      check('docked, it opens under the pointer again', true);
      var pill = notch().querySelector('.ca-notch-body .ca-hud-pill');
      var before = calls('drag_to_desktop').length;
      press(pill, [430, 460]);
      check('open, a pill keeps its drag', drags() === 1 && calls('drag_to_desktop').length === before + 1, drags() + ' notch drags, ' + (calls('drag_to_desktop').length - before) + ' pill drags');
      window.__emit('desktop-drag-end', { kept: false });
      var mark = notch().querySelector('.ca-hud-mark');
      press(mark, [430, 460]);
      check('and so does a button', drags() === 1);
      var cam = notch().querySelector('.ca-notch-cam');
      press(cam, [430]);
      var d = last('notch_drag');
      check('the black around the camera drags it', drags() === 2 && d && d.dx === 30, JSON.stringify(d));
      return until(isClosed, 1000, 'the notch to close as it moves');
    })
    .then(function () {
      check('and it closes as it goes', true);
      window.__emit('notch-drag-end', null);
      return wait(300).then(settle);
    })
    .then(function () {
      window.__NOTCH_DRAG__ = false;
      enter();
      return until(isOpen, 1500, 'the notch to open');
    })
    .then(function () {
      press(notch().querySelector('.ca-notch-cam'), [430]);
      return wait(300);
    })
    .then(function () {
      check('a drag the engine doesn’t start leaves it under the pointer, not held shut', drags() === 3 && notch().classList.contains('is-hover'), notch().className);
    })
    .then(done)
    .catch(stopped);
})();
