// Checks that marking sessions seen shows at once, before the engine answers (ahead.ts): the
// Worktrees board's "Mark all seen" in the Your turn bay empties the bay while mark_seen is still
// on its way, and a mark_seen the engine turns down brings the bay back with why.
//   scripts/preview.sh worktrees '' scripts/preview/seen-ahead-check.js
// Every step should say ok (the project needs a session whose turn ended).
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 50);
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  // mark_seen waits until the check lets it answer, as it does behind a busy engine.
  var real = window.__TAURI_INTERNALS__.invoke;
  var waiting = [];
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd !== 'mark_seen') return real.apply(this, arguments);
    window.__calls.push([cmd, args, options]);
    return new Promise(function (res, rej) { waiting.push({ res: res, rej: rej }); });
  };
  var button = function () { return Array.prototype.find.call(document.querySelectorAll('.ca-linkbtn'), function (b) { return b.textContent === 'Mark all seen'; }); };
  var toast = function () { var t = document.querySelector('.ca-toast'); return t ? t.textContent : ''; };
  say('seen check: waiting for the board');
  until(function () { return !!document.querySelector('.ca-bayblock'); }, 20000)
    .then(function () {
      if (!button()) throw new Error('no session in Your turn');
      button().click();
      return until(function () { return !button(); }, 2000);
    })
    .then(function () {
      check('the bay empties before the engine answers', !button() && waiting.length === 1, String(waiting.length));
      waiting[0].rej('Codealong’s engine stopped.');
      return until(function () { return !!button(); }, 2000);
    })
    .then(function () {
      check('turned down, it comes back', !!button());
      check('with why', /engine stopped/.test(toast()), toast());
      say('done: ' + out.join('; '));
    })
    .catch(function (e) { say('done: ' + out.join('; ') + '; stopped: ' + e.message); });
})();
