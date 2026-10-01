// Checks the pin that keeps the notch open (Notch.tsx): it shows once the notch is open, a click
// on it asks Codealong to keep it open and the notch then stays open with the pointer gone, the
// notch's menu says so with a check, and unpinning lets it close again. Codealong's word that it's
// pinned (`bar-mode`) opens it at once.
//   PREVIEW_WAIT=40000 scripts/preview.sh notch '' scripts/preview/notch-pin-check.js
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
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }).map(function (c) { return c[1]; }); };
  var notch = function () { return document.querySelector('.ca-notch'); };
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var pin = function () { return document.querySelector('.ca-notch-pin'); };
  var isOpen = function () { return notch() && notch().classList.contains('is-open'); };
  var leave = function () { shape().dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body })); };
  var menuItem = function (label) {
    var m = calls('plugin:menu|new');
    var items = m.length ? m[m.length - 1].options.items : [];
    return items.filter(function (i) { return i.text === label; })[0];
  };

  say('notch pin check: waiting for it');
  until(function () { return notch() && pin(); }, 15000)
    .then(function () {
      check('closed, the pin is out of sight', getComputedStyle(pin()).opacity === '0');
      shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true }));
      pin().click();
      return wait(100);
    })
    .then(function () {
      check('closed, a click on the pin only opens the notch', isOpen() && calls('set_notch_open').length === 0);
      // It opened on a session, as a click anywhere on it does: put away, the pills stay under
      // the pointer.
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', cancelable: true }));
      return wait(500);
    })
    .then(function () {
      check('open, the pin shows', getComputedStyle(pin()).opacity === '1');
      pin().click();
      return wait(100);
    })
    .then(function () {
      var c = calls('set_notch_open');
      check('a click pins it', c.length === 1 && c[0].keep === true, JSON.stringify(c));
      check('the pin says it’s on', pin().classList.contains('is-on') && pin().getAttribute('aria-pressed') === 'true');
      leave();
      return wait(1200);
    })
    .then(function () {
      check('stays open with the pointer gone', isOpen());
      shape().dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      return wait(300);
    })
    .then(function () {
      var item = menuItem('Keep the notch open');
      check('its menu says it’s kept open', !!item && item.checked === true, JSON.stringify(item));
      return wait(300);
    })
    .then(function () {
      pin().click();
      return wait(100);
    })
    .then(function () {
      var c = calls('set_notch_open');
      check('a second click unpins it', c.length === 2 && c[1].keep === false, JSON.stringify(c));
      leave();
      return until(function () { return !isOpen(); }, 2000).then(function () { check('unpinned, it closes once the pointer is gone', true); });
    })
    .then(function () {
      window.__emit('bar-mode', Object.assign({}, window.__BAR_MODE__, { keepOpen: true }));
      return until(isOpen, 1000).then(function () { check('pinned from elsewhere, it opens at once', true); });
    })
    .then(function () { say('done: ' + out.join(' · ')); })
    .catch(function (e) { say('done: ' + out.concat(['error ' + e.message]).join(' · ')); });
})();
