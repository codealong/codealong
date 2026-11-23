// Checks a waiting session on another device (SessionReply.tsx `Approval`, `remoteAnswerWhere`):
// when `session_pending` returns null or `cannot`, the panel names that device rather than
// offering a broken button to open this Mac's configured terminal.
//   PREVIEW_DEVICES=scripts/preview/devices-sample.json PREVIEW_WAIT=60000 scripts/preview.sh kept '' scripts/preview/remote-approval-check.js
(function () {
  localStorage.setItem('codealong.preview.peek', '@');
  var picked = null;
  var deviceName = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(null);
    }
    var p = real(cmd, args, options);
    if (cmd !== 'devices') return p;
    return p.then(function (v) {
      (v.lanes || []).forEach(function (l) {
        if (!l.device || !l.device.online) return;
        l.sessions = l.sessions.map(function (s) {
          if (picked ? s.key !== picked : (s.visitor || s.archived)) return s;
          picked = s.key;
          deviceName = l.device.name;
          return Object.assign({}, s, {
            harness: 'codex',
            account: 'codex',
            status: 'needs',
            need: 'approval',
            stalledSince: undefined,
            live: true,
            hosted: false,
            asks: false,
            surface: 'cli',
            terminal: undefined,
            excerpt: 'Waiting for approval? · exec',
          });
        });
      });
      return v;
    });
  };

  say('remote approval check: waiting for the remote session');
  until(function () { return !!picked && !!q('.ca-composer.is-waiting'); }, 15000, 'the waiting bar')
    .then(function () { return wait(300); })
    .then(function () {
      var bar = q('.ca-composer.is-waiting');
      var text = bar ? bar.textContent.trim() : '';
      check('names the remote device when pending is null', text === 'Answer on ' + deviceName, text);
      check('has no broken local open button', !bar || !bar.querySelector('button'), bar ? bar.innerHTML : 'none');
      check('never names a local terminal on the waiting bar', !/Answer in /.test(text), text);
      done();
    })
    .catch(stopped);
})();
