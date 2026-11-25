// Checks that staging a move in the reply box (HandoffStrip.tsx) keeps the keyboard safe in the
// waiting window, whose keys work on the session shown: with a call the session waits on, ⌘⏎ and
// ⌘⌫ while a move is staged neither allow nor deny it, Escape cancels the staged move without
// closing the window, and ⌘⏎ allows again once it's canceled.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/move-keys-check.js
(function () {
  var get = function (file) {
    var xhr = new XMLHttpRequest();
    xhr.open('GET', file, false);
    xhr.send();
    return xhr.status === 200 ? JSON.parse(xhr.responseText) : [];
  };
  var accounts = get('./accounts.json').filter(function (a) { return a.harness === 'claude' && a.signedIn && !a.disabled; });
  var picked = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked && (args || {}).key === picked) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ id: 'p1', label: 'Run `git push`', input: 'git push origin HEAD:main', cannot: null });
    }
    if (cmd === 'session_answer') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve(args.answer === 'deny' ? 'Denied.' : 'Allowed.');
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          var other = accounts.some(function (a) { return a.name !== s.account; });
          if (picked && s.key !== picked) return;
          if (l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude' || !s.resumeCommand || !other) return;
          s.status = 'needs';
          s.statusSince = Date.now() - 60e3;
          s.lastActivity = s.statusSince;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          picked = s.key;
        });
      });
      return board;
    });
  };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var bar = function () { return q('.ca-handoff-bar'); };

  (async function () {
    try {
      await until(function () { return q('.ca-waiting .ca-composer.is-approval'); }, 20000, 'no call waiting in the waiting window');
      if (!picked) return stopped('no Claude Code session with another signed-in Claude Code account to move to');
      var chip = await until(function () { return q('.ca-waiting .ca-composer-buttons .ca-modelmenu-chip'); }, 5000, 'no account chip in the box');
      chip.click();
      var menu = await until(function () { return q('.ca-waiting .ca-composer-buttons .ca-modelmenu .ca-picker'); }, 3000, 'no menu on the chip');
      var moving = await until(function () {
        return all('.ca-picker-opt', menu).filter(function (o) {
          var t = q('.ca-picker-title', o);
          var d = q('.ca-picker-detail', o);
          return (t && t.textContent.trim() === 'Move it here') || (d && d.textContent.trim() === 'Move it here');
        })[0];
      }, 3000, 'no account to move to in the chip’s menu');
      moving.click();
      var strip = await until(bar, 3000, 'picking another account showed no inline confirmation bar');
      check('the inline bar sits inside the reply box rather than covering the window', !!strip.closest('.ca-composer-field'), 'not inside composer');
      document.activeElement && document.activeElement.blur && document.activeElement.blur();
      key({ key: 'Enter', metaKey: true });
      key({ key: 'Backspace', metaKey: true });
      await wait(200);
      check('⌘⏎ and ⌘⌫ while staged neither allow nor deny the call', calls('session_answer').length === 0, JSON.stringify(calls('session_answer').map(function (c) { return c[1]; })));
      key({ key: 'Escape' });
      await until(function () { return !bar(); }, 3000, 'Escape left the inline bar up');
      check('Escape cancels the staged move and leaves the window', !!q('.ca-waiting .ca-composer.is-approval'), 'no call shown');
      check('nothing was moved', calls('move_session').length === 0, String(calls('move_session').length));
      document.activeElement && document.activeElement.blur && document.activeElement.blur();
      key({ key: 'Enter', metaKey: true });
      await until(function () { return calls('session_answer').length > 0; }, 3000, '⌘⏎ allowed nothing once the staged move was canceled');
      check('once canceled, ⌘⏎ allows the call again', calls('session_answer')[0][1].answer === 'allow', JSON.stringify(calls('session_answer')[0][1]));
    } catch (e) {
      return stopped(e);
    }
    done();
  })();
})();
