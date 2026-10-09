// Checks Snooze on a session on another Mac (menus.ts `sessionActions`, actions.rs
// `run_keys_heard`): its strip and its menu offer Snooze as they do for this Mac's, ⌃⌘1 asks the
// engine to snooze it by the key that names its Mac, a Mac that didn't snooze it (from before it
// could be asked) has the toast say so and leaves it as it was, and one that did shows it snoozed
// at once.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/remote-snooze-check.js
(function () {
  // This runs before the app's script: the first session on the Mac mini is Your turn, and
  // nothing else waits on you.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = null;
  var OLDER = 'Codealong on Mac mini can’t do that from here until it’s updated.';
  var refuse = true;
  var real = window.__TAURI_INTERNALS__.invoke;
  var quiet = function (s) {
    if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
    s.snoozedUntil = undefined;
  };
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'snooze_sessions') {
      window.__calls.push([cmd, args, options]);
      return refuse ? Promise.reject(OLDER) : Promise.resolve(null);
    }
    var p = real(cmd, args, options);
    if (cmd === 'get_board') {
      return p.then(function (board) {
        board.lanes.forEach(function (l) { l.sessions.forEach(quiet); });
        return board;
      });
    }
    if (cmd !== 'devices') return p;
    return p.then(function (view) {
      (view.lanes || []).forEach(function (l) {
        l.sessions.forEach(function (s) {
          quiet(s);
          if (picked || l.device.short !== '9f8e7d6c5b' || s.visitor) return;
          s.status = 'unread';
          s.statusSince = Date.now() - 10 * 60e3;
          s.lastActivity = s.statusSince;
          s.archived = false;
          s.asks = false;
          s.title = 'Far A';
          picked = { key: s.key, lane: l.id };
        });
      });
      return view;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var toast = function () { var t = document.querySelector('.ca-toast:not(.is-busy)'); return t ? t.textContent : ''; };
  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var snoozed = function () { var r = row(); return !!r && !!r.querySelector('.ca-snoozechip'); };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var asked = function () { return calls('snooze_sessions').map(function (c) { return c[1]; }); };
  var hotkey = function (label) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-sessinsp .ca-hotkeys .ca-hotkey'), function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };
  var show = function () {
    row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    return until(function () { return title() === 'Far A'; }, 3000, 'its panel').then(function () { return wait(50); });
  };

  say('remote snooze check: waiting for the Mac mini’s session');
  until(function () { return !!row(); }, 20000, 'no session on the Mac mini (run with PREVIEW_DEVICES)')
    .then(function () { return wait(300); })
    .then(function () { return menuOf(row()); })
    .then(function (labels) {
      check('its menu offers Snooze', labels.indexOf('Snooze › For 15 minutes') >= 0, labels.join(' | '));
      return show();
    })
    .then(function () {
      var b = hotkey('Snooze');
      check('the strip offers Snooze for it', !!b && !b.disabled && /^Snooze: it waits apart/.test(b.title), b ? b.title + ' disabled=' + b.disabled : 'no button');
      key({ key: '1', code: 'Digit1', metaKey: true, ctrlKey: true });
      return until(function () { return asked().length === 1; }, 3000, '⌃⌘1 asking the engine').then(function () { return wait(300); });
    })
    .then(function () {
      var a = asked()[0];
      var soon = a && a.until - Date.now();
      check('⌃⌘1 asks the engine to snooze it for 15 minutes, by the key that names its Mac', !!a && a.keys.length === 1 && a.keys[0] === picked.key && /^@9f8e7d6c5b:/.test(a.keys[0]) && soon > 14 * 60e3 && soon <= 15 * 60e3, JSON.stringify(a));
      check('a Mac that didn’t snooze it has the toast say why', toast().indexOf(OLDER) >= 0, toast());
      check('and it isn’t shown snoozed', !!row() && !snoozed());
      refuse = false;
      return show();
    })
    .then(function () {
      key({ key: '2', code: 'Digit2', metaKey: true, ctrlKey: true });
      return until(function () { return asked().length === 2; }, 3000, '⌃⌘2 asking the engine').then(function () { return wait(300); });
    })
    .then(function () {
      check('one that did shows it snoozed at once', snoozed());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
