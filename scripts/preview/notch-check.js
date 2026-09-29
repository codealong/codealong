// Checks the bar docked in the notch (Notch.tsx): closed it shows the most urgent count and tells
// Codealong its shape; pointing at it grows it a little at once and opens it after a moment, the
// clip only ever growing and the window never resized; leaving closes it after a moment; a click
// opens it at once; it stays open while the peek is up; and Codealong's own word of the pointer
// (`notch-pointer`) works as the page's events do.
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
  var shape = function () { return document.querySelector('.ca-notch-shape'); };
  var isOpen = function () { return notch() && notch().classList.contains('is-open'); };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }).map(function (c) { return c[1]; }); };
  var fits = function () { return calls('fit_bar').map(function (a) { return a.width + 'x' + a.height; }); };
  var lastShape = function () { var s = calls('notch_shape'); return s[s.length - 1]; };
  // The clip's left inset as drawn now, from its computed value: inset(0px 123px 34px round …),
  // which gives one to four lengths, top first, the left one last.
  var inset = function () {
    var m = /inset\(([^)]*?)(?: round|\))/.exec(getComputedStyle(shape()).clipPath);
    if (!m) return NaN;
    var v = m[1].trim().split(/\s+/).map(parseFloat);
    return v.length === 4 ? v[3] : v.length >= 2 ? v[1] : v[0];
  };
  var enter = function () { shape().dispatchEvent(new PointerEvent('pointerover', { bubbles: true })); };
  var leave = function () { shape().dispatchEvent(new PointerEvent('pointerout', { bubbles: true, relatedTarget: document.body })); };
  var closedW = 185 + 2 * 44;

  say('notch check: waiting for it');
  var windowAtStart;
  var fitsAtStart = 0;
  until(function () { return notch() && calls('notch_shape').length > 0 && document.querySelector('.ca-notch-count'); }, 15000)
    .then(function () { return wait(400); })
    .then(function () {
      windowAtStart = fits()[fits().length - 1];
      fitsAtStart = fits().length;
      var s = lastShape();
      check('says its closed shape', s.w === closedW && s.h === 32 && s.y === 0, JSON.stringify(s));
      check('shows the most urgent count', !!document.querySelector('.ca-notch-side.is-right .ca-notch-count'));
      check('dark', document.documentElement.dataset.theme === 'dark');
      enter();
      return wait(100);
    })
    .then(function () {
      check('not open at once', !isOpen());
      check('grows a little under the pointer', lastShape().w === closedW + 8, JSON.stringify(lastShape()));
      // Sample the clip as it opens: it only ever narrows its inset, the window stays put, and the
      // left corner reaches the same 3 points under the shape's edge every frame: it moves in step
      // with the shape, and no seam shows between them.
      var samples = [];
      var overlaps = [];
      var sampler = setInterval(function () {
        var i = inset();
        samples.push(i);
        var ear = document.querySelector('.ca-notch-ear.is-left').getBoundingClientRect().right;
        overlaps.push(ear - (notch().getBoundingClientRect().left + i));
      }, 16);
      return until(isOpen, 1500)
        .then(function () { return wait(600); })
        .then(function () {
          clearInterval(sampler);
          var back = samples.filter(function (v, i) { return i > 0 && v > samples[i - 1] + 0.5; }).length;
          check('opens smoothly, never jumping back', back === 0 && samples[samples.length - 1] === 0, back + ' backward steps in ' + samples.length + ', last ' + samples[samples.length - 1]);
          check('never resizes its window', fits().slice(fitsAtStart).every(function (f) { return f === windowAtStart; }), windowAtStart + ' then ' + fits().slice(fitsAtStart).join(' '));
          var least = Math.min.apply(null, overlaps);
          var most = Math.max.apply(null, overlaps);
          check('the corner reaches under the shape, in step with it throughout', least >= 2.5 && most - least < 0.5, 'overlap ' + least.toFixed(2) + ' to ' + most.toFixed(2));
          check('the shadow shows open', getComputedStyle(document.querySelector('.ca-notch-shadow')).opacity === '1');
          check('says its open shape', lastShape().w > closedW && lastShape().h > 32, JSON.stringify(lastShape()));
          check('pills below', notch().querySelectorAll('.ca-notch-body .ca-hud-pill').length > 0);
        });
    })
    .then(function () {
      leave();
      return wait(250);
    })
    .then(function () {
      check('stays a moment after leaving', isOpen());
      return until(function () { return !isOpen(); }, 1500);
    })
    .then(function () {
      check('closes, saying its closed shape', lastShape().w === closedW, JSON.stringify(lastShape()));
      shape().click();
      return wait(100);
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
    .then(function () {
      // Codealong's word of the pointer, as in the app, where the window lets it through.
      window.__emit('notch-pointer', true);
      return until(isOpen, 1500).then(function () { check('opens on Codealong’s word', true); });
    })
    .then(function () {
      window.__emit('notch-pointer', false);
      return until(function () { return !isOpen(); }, 1500).then(function () { check('closes on it', true); });
    })
    .then(function () { say('done: ' + out.join(', ')); })
    .catch(function (e) { say('done: ' + out.concat(['error ' + e.message]).join(', ')); });
})();
