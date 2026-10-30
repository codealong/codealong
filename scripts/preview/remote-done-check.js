// Checks Mark as done on a session on another Mac (menus.ts `sessionActions`, actions.rs
// `done_anyway`): its strip offers Done as it does for this Mac's, ⇧⌘E asks the engine to be done
// with it by the key that names its Mac, it leaves the list at once, and when that Mac won't
// close it (remote control off there) the toast says it's archived but still open there.
//   PREVIEW_WAIT=60000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/remote-done-check.js
(function () {
  // This runs before the app's script: the first session on the Mac mini waits on you, and
  // nothing else does.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  var quiet = function (s) {
    if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
    s.snoozedUntil = undefined;
    s.raised = undefined;
  };
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_done') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ done: true, noted: true, message: 'Done with “Far A”: it’s archived, but still open on Mac mini. Remote control is off on Mac mini; turn it on there in Settings › Devices.' });
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
          s.live = true;
          s.surface = 'cli';
          s.background = undefined;
          s.ownWorktree = undefined;
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
  var listed = function () { var r = row(); return !!r && !r.closest('.ca-archived'); };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var asked = function () { return window.__calls.filter(function (c) { return c[0] === 'session_done'; }).map(function (c) { return c[1]; }); };
  var hotkey = function (label) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-sessinsp .ca-hotkeys .ca-hotkey'), function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };

  say('remote done check: waiting for the Mac mini’s session');
  until(function () { return !!row(); }, 20000, 'no session on the Mac mini (run with PREVIEW_DEVICES)')
    .then(function () { return wait(300); })
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Far A'; }, 3000, 'its panel').then(function () { return wait(50); });
    })
    .then(function () {
      var b = hotkey('Done');
      check('the strip offers Done for it, with its key', !!b && !b.disabled && /⇧⌘E/.test(b.textContent), b ? b.textContent + ' disabled=' + b.disabled : 'no button');
      check('its tooltip says what Done does', !!b && /^Mark as done: it closes where Codealong can close it/.test(b.title), b ? b.title : '');
      key({ key: 'E', code: 'KeyE', metaKey: true, shiftKey: true });
      return wait(300);
    })
    .then(function () {
      var a = asked()[0];
      check('⇧⌘E asks the engine to be done with it, by the key that names its Mac', !!a && a.key === picked.key && /^@9f8e7d6c5b:/.test(a.key) && a.stop === false, JSON.stringify(a));
      check('it leaves the list at once', !listed());
      check('the toast says it’s archived but still open there, and why', /still open on Mac mini\. Remote control is off on Mac mini/.test(toast()), toast());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
