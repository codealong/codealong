// Checks a session of another device's kept on the desktop (Peek.tsx's kept window, which finds it
// among your devices' worktrees, useDevices.ts): it shows, with its header saying the device, and
// isn't taken for gone.
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_WAIT=60000 scripts/preview.sh kept '' scripts/preview/kept-devices-check.js
(function () {
  // A connected device's first session (Peek.tsx `previewKey`).
  localStorage.setItem('codealong.preview.peek', '@');
  var title = function () { var t = document.querySelector('.ca-sessinsp-title, .ca-sessinsp-head h2'); return t ? t.textContent.trim() : ''; };
  var badge = function () { var b = document.querySelector('.ca-sessinsp-head .ca-devbadge, .ca-devbadge'); return b ? b.textContent.trim() : ''; };
  var theirs = null;

  say('kept devices check: waiting for it');
  window.__TAURI_INTERNALS__.invoke('devices')
    .then(function (d) {
      d.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) { if (!theirs && l.device.online && !s.visitor) theirs = { lane: l, session: s }; });
      });
      check('has a session on a connected device', !!theirs);
      return until(function () { return title() !== '' || document.body.textContent.indexOf('isn’t on the board any more') >= 0; }, 15000, 'the kept session');
    })
    .then(function () { return wait(400); })
    .then(function () {
      check('it shows the device’s session', title().indexOf(theirs.session.title.slice(0, 20)) === 0, title() || document.body.textContent.slice(0, 200));
      check('its header says the device', badge() === 'On ' + theirs.lane.device.name, badge() || 'none');
      check('it isn’t taken for gone', document.body.textContent.indexOf('isn’t on the board any more') < 0);
      done();
    })
    .catch(stopped);
})();
