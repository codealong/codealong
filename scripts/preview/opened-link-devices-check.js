// Checks a link macOS opened with Codealong (front.rs `open_links`) that names a session on
// another of your devices. The window reads your devices after the board, so with the board
// read and the devices still to come it doesn't yet list the session: it waits for them rather
// than say there's none, and shows the session once they've come. The devices answer late here,
// as they do on a busy Mac, and the session is the first they list.
//   PREVIEW_WAIT=30000 PREVIEW_DEVICES=scripts/preview/devices-sample.json scripts/preview.sh sessions '' scripts/preview/opened-link-devices-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.age', 'all');
  var LATE = 2500;
  var picked = null;
  var answered = 0;
  var known = null;
  var listed = new Promise(function (r) { known = r; });
  var first = true;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    // The link is there before the page asks: its session's key is known once the stub has
    // made your devices' worktrees, which the page doesn't get until later.
    if (cmd === 'take_opened_link' && first) {
      first = false;
      window.__calls.push([cmd, args, options]);
      return listed.then(function () { return picked ? 'codealong://session/' + picked.key : null; });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'devices') return p;
    return p.then(function (view) {
      (view.lanes || []).forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (!picked && !l.hidden && !s.visitor && !s.archived) picked = { key: s.key, lane: l.id, title: s.title };
        });
      });
      known();
      return new Promise(function (r) { setTimeout(function () { answered = Date.now(); r(view); }, LATE); });
    });
  };

  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var title = function () { var t = q('.ca-sessinsp .ca-sessinsp-title'); return t ? t.textContent : null; };
  var toast = function () { var t = q('.ca-toast'); return t ? t.textContent : ''; };

  say('opened link devices check: waiting for your devices');
  listed
    .then(function () {
      if (!picked) throw new Error('your devices list no session: run it with PREVIEW_DEVICES=scripts/preview/devices-sample.json');
      // The board is read and the link taken, well before the devices answer.
      return until(function () { return calls('take_opened_link').length > 0 && document.querySelector('[id^="row-"]'); }, 15000, 'the board’s rows');
    })
    .then(function () { return wait(1200); })
    .then(function () {
      check('with your devices still to come, the session isn’t listed yet', !answered && !row());
      check('and the link isn’t said to name none', !/no session/.test(toast()), toast());
      return until(function () { return row() && title() === picked.title; }, LATE + 10000, 'the session on another device, in the panel');
    })
    .then(function () {
      check('once they’ve come, it shows the session on the other device', answered > 0 && row().matches('.is-selected, [aria-selected="true"]'), row().className);
      check('and never said there was none', !/no session/.test(toast()), toast());
    })
    .then(done)
    .catch(stopped);
})();
