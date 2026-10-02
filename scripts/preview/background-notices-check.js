// Checks that a window's own background failures are notices in the accounts bar, and end once
// what failed works again (notices.ts `reportNotice`, `clearNotice`): where each account can
// start a session, read at launch for Review with (App.tsx), and Jev's suggestions for a todo
// (TodoPanel.tsx). Both fail at first; the notices show; then they work, Jev's on the next todo
// and the start options as a PR's menu asks for them, and the notices go.
//   PREVIEW_WAIT=60000 PREVIEW_TODOS=scripts/preview/todos-jev-sample.json scripts/preview.sh todo '' scripts/preview/background-notices-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var until = function (f, ms, what) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { var v = f(); if (v) return res(v); if (Date.now() - s > ms) return rej(new Error('timeout: ' + what)); setTimeout(p, 50); })();
    });
  };
  // This runs before the page's own scripts, so the app asks through this from the start.
  var failing = { start_options: true, suggest_todo: true };
  var tauri = window.__TAURI_INTERNALS__;
  var invoke = tauri.invoke;
  tauri.invoke = function (cmd) {
    if (failing[cmd]) return Promise.reject('the engine is busy');
    return invoke.apply(this, arguments);
  };
  var titles = function () {
    var chip = document.querySelector('.ca-notice-open');
    if (!chip) return [];
    if (!document.querySelector('.ca-notice-card')) chip.click();
    var list = Array.prototype.map.call(document.querySelectorAll('.ca-notice-card .ca-notice-title'), function (el) { return el.textContent; });
    chip.click();
    return list;
  };
  var shows = function (t) { return titles().indexOf(t) >= 0; };
  say('background notices check: waiting');
  until(function () { return shows('Can’t read where sessions start') && shows('Jev can’t suggest for todos'); }, 20000, 'both notices')
    .then(function () {
      check('a failed read of where sessions start is a notice', true);
      check('a failed suggestion from Jev is a notice', true);
      var text = (function () {
        document.querySelector('.ca-notice-open').click();
        var t = Array.prototype.map.call(document.querySelectorAll('.ca-notice-card .ca-notice-text'), function (el) { return el.textContent; }).join(' | ');
        document.querySelector('.ca-notice-open').click();
        return t;
      })();
      check('each says what failed and why', /the last answer read stands: the engine is busy/.test(text) && /Jev couldn’t suggest .*: the engine is busy/.test(text), text);
      failing.suggest_todo = false;
      failing.start_options = false;
      // Another todo asks Jev again.
      var rows = document.querySelectorAll('.ca-todorow-title');
      rows[1].click();
      return until(function () { return !shows('Jev can’t suggest for todos'); }, 10000, 'Jev’s notice to go');
    })
    .then(function () {
      check('Jev’s goes once it answers', true);
      check('the other stays until its read works', shows('Can’t read where sessions start'));
      var prs = Array.prototype.find.call(document.querySelectorAll('.ca-nav'), function (b) { return /PRs/.test(b.textContent); });
      prs.click();
      return until(function () { return document.querySelector('.ca-bay-lanes > *'); }, 10000, 'a PR row');
    })
    .then(function (row) {
      row.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
      return until(function () { return !document.querySelector('.ca-notice-open'); }, 10000, 'the start options notice to go');
    })
    .then(function () {
      check('and it goes once a PR’s menu reads them', true);
      say('done: ' + out.join('; '));
    })
    .catch(function (e) { say('done: ' + out.concat(['FAIL ' + e.message]).join('; ')); });
})();
