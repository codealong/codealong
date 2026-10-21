// Checks that marking sessions seen shows at once, before the engine answers (ahead.ts): Mark as
// seen, in the menu of a session in Your turn, takes it out of Your turn while mark_seen is still
// on its way, and a mark_seen the engine turns down brings it back with why.
//   scripts/preview.sh sessions '' scripts/preview/seen-ahead-check.js
// The project needs a session whose turn ended.
(function () {
  // mark_seen waits until the check lets it answer, as it does behind a busy engine.
  var real = window.__TAURI_INTERNALS__.invoke;
  var waiting = [];
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd !== 'mark_seen') return real.apply(this, arguments);
    window.__calls.push([cmd, args, options]);
    return new Promise(function (res, rej) { waiting.push({ res: res, rej: rej }); });
  };
  var id = null;
  var unread = function () { var r = id && document.getElementById(id); return !!r && r.classList.contains('is-unread'); };
  var toast = function () { var t = document.querySelector('.ca-toast'); return t ? t.textContent : ''; };
  say('seen check: waiting for the board');
  until(function () { return !!document.querySelector('.ca-sessionlist'); }, 20000)
    .then(function () {
      var row = document.querySelector('.ca-sessionlist .ca-lane.is-agent.is-unread[id^="row-"]');
      if (!row) throw new Error('no session in Your turn');
      id = row.id;
      return menuOf(row);
    })
    .then(function (labels) {
      var seen = menuItem('Mark as seen');
      if (!seen) throw new Error('no Mark as seen in ' + labels.join(' | '));
      seen();
      return until(function () { return !unread(); }, 2000);
    })
    .then(function () {
      check('its turn is seen before the engine answers', !unread() && waiting.length === 1, String(waiting.length));
      waiting[0].rej('Codealong’s engine stopped.');
      return until(unread, 2000);
    })
    .then(function () {
      check('turned down, it comes back', unread());
      check('with why', /engine stopped/.test(toast()), toast());
      done();
    })
    .catch(stopped);
})();
