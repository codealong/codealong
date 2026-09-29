// Checks what an item in the floating bar's menus does (Bar.tsx): a session picked from a folded
// pill's menu or the recent pill's shows in Codealong's window, rather than opening where it
// lives. The same menus serve the bar in the notch.
//   PREVIEW_WAIT=30000 scripts/preview.sh bar '' scripts/preview/bar-menus-check.js
// Needs a folded pill and a recent one in the feed (a busy day has both).
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
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }); };
  var lastMenu = function () { var m = calls('plugin:menu|new'); return m.length ? m[m.length - 1][1].options.items : []; };
  var pick = function (item) { window['_' + item.handler.id]({ index: 0, message: item.id }); };
  var first = function (items) { return items.filter(function (i) { return i.text && i.handler; })[0]; };

  var tryMenu = function (name, selector) {
    var pill = document.querySelector(selector);
    if (!pill) {
      check(name + ': none in this feed', true);
      return Promise.resolve();
    }
    var opens = calls('open_from_bar').length;
    var shows = calls('show_main').length;
    pill.click();
    return wait(200).then(function () {
      var item = first(lastMenu());
      pick(item);
      return wait(100).then(function () {
        var shown = calls('show_main')[shows];
        check(name + ' shows the session in Codealong', !!shown && shown[1].reveal && shown[1].reveal.kind === 'session' && item.text.indexOf(' · ') > 0, JSON.stringify(shown && shown[1]));
        check(name + ' doesn’t open it where it lives', calls('open_from_bar').length === opens);
      });
    });
  };

  say('bar menus check: waiting for pills');
  until(function () { return document.querySelector('.ca-hud [data-key]'); }, 20000)
    .then(function () { return wait(300); })
    .then(function () { return tryMenu('a folded pill’s menu', '.ca-hud .ca-hud-fold'); })
    .then(function () { return tryMenu('the recent pill’s menu', '.ca-hud .is-recent'); })
    .catch(function (e) { out.push('error ' + e); })
    .then(function () { say('done: ' + out.join(' · ')); });
})();
