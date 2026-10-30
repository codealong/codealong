// Checks what the waiting window does with a message that didn't go once it had gone on to the end
// of what waits (Waiting.tsx `onFailed`): sent with ⏎ to the last one, which then left what waits
// (⌃⌘1 snoozes it), the window says nothing else waits and how that one went; when the message
// then fails, the window is back on that session, its strip saying why, rather than on the end.
//   PREVIEW_WAIT=40000 scripts/preview.sh waiting '' scripts/preview/waiting-unsent-check.js
(function () {
  var picked = null;
  var flying = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    // A message fails when the check says.
    if (cmd === 'send_to_session') {
      window.__calls.push([cmd, args, options]);
      return new Promise(function (_, reject) { flying.push(reject); });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          // Only the one made here waits on you.
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
          if (picked && s.key !== picked) return;
          if (!picked && (l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude')) return;
          picked = s.key;
          s.status = 'unread';
          s.statusSince = Date.now() - 600e3;
          s.live = true;
          s.hosted = false;
          s.asks = false;
          s.suggestions = undefined;
          s.title = 'Unsent W';
        });
      });
      (board.prs || []).forEach(function (pr) { pr.reviewRequested = false; });
      return board;
    });
  };

  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return q('.ca-composer:not(.is-approval) textarea'); };
  var strip = function () { var n = q('.ca-reply-queue-note'); return n ? n.textContent : ''; };
  var end = function () { var e = q('.ca-waiting-empty'); return e ? e.textContent : ''; };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };

  say('waiting unsent check: waiting for the window');
  until(function () { return title() === 'Unsent W' && !!box(); }, 20000, 'the session and its box')
    .then(function () {
      box().focus();
      type(box(), 'Run the tests again');
      key({ key: 'Enter' });
      return until(function () { return calls('send_to_session').length === 1; }, 3000, 'the send');
    })
    .then(function () {
      key({ key: '1', code: 'Digit1', metaKey: true, ctrlKey: true });
      return until(function () { return /Nothing else waits on you/.test(end()); }, 3000, 'the end of what waits');
    })
    .then(function () {
      check('snoozed, the last one goes on to the end, saying how it went', /Unsent W: snoozed\./.test(end()), end());
      flying.shift()('Its terminal is gone.');
      return until(function () { return title() === 'Unsent W'; }, 3000).catch(function () {});
    })
    .then(function () {
      check('a message that didn’t go from there takes the window back to its session', title() === 'Unsent W' && !end(), title() || end());
      return until(function () { return /Didn’t send/.test(strip()); }, 3000).catch(function () {});
    })
    .then(function () {
      check('whose strip says why', strip() === 'Didn’t send to Unsent W: Its terminal is gone.', strip());
    })
    .then(done)
    .catch(stopped);
})();
