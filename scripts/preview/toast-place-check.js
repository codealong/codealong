// Checks where the main window says things around a session's panel (Toasts.tsx `usePlace`,
// MainPanel.tsx): a message that didn't go after ⌘⏎ went on to the next is said on the next one's
// strip, not in a toast over its box; a toast sits at the foot of the board, never on the panel's
// message box, and with the panel taking the whole window, above the box.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/toast-place-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    // The first one's message fails once the panel has gone on from it.
    if (cmd === 'send_to_session' && picked[0] && (args || {}).key === picked[0].key) {
      window.__calls.push([cmd, args, options]);
      return new Promise(function (_, reject) { setTimeout(function () { reject('Its terminal is gone.'); }, 400); });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || seen[s.key] || picked.length >= 2) return;
          seen[s.key] = true;
          s.status = 'unread';
          s.statusSince = now - (2 - picked.length) * 3600e3;
          s.lastActivity = s.statusSince;
          s.asks = false;
          s.hosted = false;
          s.stoppable = false;
          s.title = 'Toast ' + 'AB'[picked.length];
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var toast = function () { return document.querySelector('.ca-toast'); };
  var rect = function (el) { var r = el.getBoundingClientRect(); return { left: Math.round(r.left), right: Math.round(r.right), top: Math.round(r.top), bottom: Math.round(r.bottom) }; };
  var overlaps = function (a, b) { return a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom; };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  var archived = function () {
    window.__emit('agent-done', { key: picked[1].key, title: 'Toast B', message: '“Orchestrator” marked “Toast B” done: its terminal is closed and it’s archived.' });
    return until(toast, 3000, 'the toast').then(function () { return wait(100); });
  };

  say('toast place check: waiting for the list');
  until(function () { return picked.length === 2 && row(0); }, 20000, 'the list')
    .then(function () { return wait(300); })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Toast A').then(function () { return until(box, 5000, 'the box'); });
    })
    .then(function () {
      box().focus();
      type(box(), 'Run the tests again');
      return wait(50);
    })
    .then(function () {
      box().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: true, bubbles: true, cancelable: true }));
      return on('Toast B');
    })
    .then(function () {
      check('⌘⏎ goes on to the next', title() === 'Toast B', title());
      return until(function () { return /Didn’t send/.test(strip()); }, 3000).catch(function () {});
    })
    .then(function () {
      check('a message that didn’t go is said on the strip of the one gone on to', /Didn’t send to Toast A, so it’s back in its box: Its terminal is gone\./.test(strip()), strip());
      check('not in a toast', !toast(), toast() && toast().textContent);
      return archived();
    })
    .then(function () {
      var t = rect(toast());
      var board = rect(document.querySelector('.ca-board'));
      var composer = rect(document.querySelector('.ca-sessinsp .ca-composer'));
      check('a toast sits over the board', t.left >= board.left && t.right <= board.right, JSON.stringify({ toast: t, board: board }));
      check('in its middle', Math.abs((t.left + t.right) / 2 - (board.left + board.right) / 2) <= 1, JSON.stringify({ toast: t, board: board }));
      check('clear of the panel’s message box', !overlaps(t, composer), JSON.stringify({ toast: t, box: composer }));
      document.querySelector('.ca-sessinsp-max').click();
      return until(function () { return q('.ca-win-body.is-panel-max'); }, 3000, 'the panel taking the window').then(function () { return wait(200); });
    })
    .then(function () {
      var t = rect(toast());
      var thread = rect(document.querySelector('.ca-sessinsp .ca-thread'));
      var composer = rect(document.querySelector('.ca-sessinsp .ca-composer'));
      check('with the panel taking the window, the toast sits above its box', t.bottom <= composer.top && t.bottom <= thread.bottom, JSON.stringify({ toast: t, box: composer, thread: thread }));
      check('in the middle of the conversation', Math.abs((t.left + t.right) / 2 - (thread.left + thread.right) / 2) <= 1, JSON.stringify({ toast: t, thread: thread }));
      document.querySelector('.ca-sessinsp-max').click();
      return until(function () { return !q('.ca-win-body.is-panel-max'); }, 3000, 'the list back').then(function () { return wait(200); });
    })
    .then(function () {
      var t = rect(toast());
      var board = rect(document.querySelector('.ca-board'));
      check('and back over the board with the list back', t.left >= board.left && t.right <= board.right, JSON.stringify({ toast: t, board: board }));
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
