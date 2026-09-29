// Checks the bar docked in the notch (Notch.tsx): closed it shows the most urgent count and asks
// for a window the size of its shape; pointing at it opens it after a moment, first growing the
// window; leaving closes it after a moment, and only then shrinks the window; a click opens it at
// once; and it stays open while the peek is up, closing once the peek goes.
//   PREVIEW_WAIT=40000 scripts/preview.sh notch '' scripts/preview/notch-check.js
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
  var notch = function () { return document.querySelector('.ca-notch'); };
  var isOpen = function () { return notch() && notch().classList.contains('is-open'); };
  var fits = function () { return window.__calls.filter(function (c) { return c[0] === 'fit_bar'; }).map(function (c) { return c[1].width + 'x' + c[1].height; }); };
  var lastFit = function () { var f = fits(); return f[f.length - 1]; };
  var enter = function () { notch().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };
  var leave = function () { notch().dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body })); };
  // 185 wide, 32 tall (preview.sh), with a 44 side on each hand and a 6 corner beyond each.
  var closedWindow = (185 + 2 * 44 + 2 * 6) + 'x32';

  say('notch check: waiting for it');
  until(function () { return notch() && fits().length > 0 && document.querySelector('.ca-notch-count'); }, 15000)
    .then(function () { return wait(300); })
    .then(function () {
      check('closed window is the shape', lastFit() === closedWindow, lastFit());
      check('shows the most urgent count', !!document.querySelector('.ca-notch-side.is-right .ca-notch-count'));
      check('dark', document.documentElement.dataset.theme === 'dark');
      enter();
      return wait(120);
    })
    .then(function () {
      check('not at once', !isOpen());
      return until(isOpen, 1500);
    })
    .then(function () {
      var f = lastFit();
      var w = Number(f.split('x')[0]);
      check('grows the window to open', w > 185 + 2 * 44 + 2 * 6 && f !== closedWindow, f);
      check('pills below', notch().querySelectorAll('.ca-notch-body .ca-hud-pill').length > 0);
      leave();
      return wait(250);
    })
    .then(function () {
      check('stays a moment after leaving', isOpen());
      return until(function () { return !isOpen(); }, 1500);
    })
    .then(function () {
      check('window still open-sized while it closes', lastFit() !== closedWindow, lastFit());
      return until(function () { return lastFit() === closedWindow; }, 1500).then(function () { check('then shrinks', true); });
    })
    .then(function () {
      notch().click();
      return wait(200);
    })
    .then(function () {
      check('a click opens it at once', isOpen());
      window.__emit('peek', { key: 'some-session', engaged: false });
      leave();
      return wait(1200);
    })
    .then(function () {
      check('stays open while the peek is up', isOpen());
      window.__emit('peek', { key: null, engaged: false });
      return until(function () { return !isOpen(); }, 2000).then(function () { check('closes once the peek goes', true); });
    })
    .then(function () { say('done: ' + out.join(', ')); })
    .catch(function (e) { say('done: ' + out.concat(['error ' + e.message]).join(', ')); });
})();
