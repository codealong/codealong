// Right-clicks the first session (or card) in a view and prints its menu's items, to see that a
// session's menu reads the same in every window (menus.ts):
//   PREVIEW_WAIT=20000 scripts/preview.sh <sessions|bar|peek|quick|desktop> '' scripts/preview/menus-check.js
// The peek also drops its menu from the header's ⋯ button, which should list the same items.
(function () {
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { var v = f(); if (v) return res(v); if (Date.now() - s > ms) return rej(new Error('nothing to right-click')); setTimeout(p, 20); })();
    });
  };
  var targets = ['.ca-hud [data-key]', '.ca-reply .ca-sessinsp-head', '.ca-quick-scroll [role="option"]', '.ca-desk', '.ca-bay-lanes > *'];
  var menus = function () { return window.__calls.filter(function (c) { return c[0] === 'plugin:menu|new'; }).length; };
  var labels = function () { return window.__menu().map(function (x) { return x[0]; }).join(' | '); };
  say('menus check: waiting');
  until(function () { for (var i = 0; i < targets.length; i++) { var el = document.querySelector(targets[i]); if (el) return el; } return null; }, 15000)
    // A moment for the data behind it: a desktop card is there before its feed is.
    .then(function (el) { return wait(1500).then(function () { return el; }); })
    .then(function (el) {
      var before = menus();
      el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      return until(function () { return menus() > before; }, 3000).then(function () {
        var right = labels();
        var more = document.querySelector('.ca-reply .ca-sessinsp-head [aria-label="More"]');
        if (!more) return say('done: ' + right);
        var n = menus();
        more.click();
        return until(function () { return menus() > n; }, 3000).then(function () {
          say('done: ' + right + ' || ⋯ ' + (labels() === right ? 'same' : 'DIFFERENT: ' + labels()));
        });
      });
    })
    .catch(function (e) { say('done: FAIL ' + e.message); });
})();
