// Checks that a message the session's harness holds until its turn ends shows in its conversation
// as queued for as long as the board says so, not as one that may not have reached it, and as sent
// once it's out of the queue (ahead.ts `settled`, SessionThread.tsx `Unsaid`): a session runs, a
// message goes to it, and boards come that show it held (`queuedAt`), then taken. Then the board
// lists what's held (`queued`, the Codex app's queue): a message sent from here shows once, one
// queued elsewhere shows too, after the turn, and they go as the queue empties. It ends with two
// messages queued, for the screenshot.
//   PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/queued-send-check.js
//   PREVIEW_SCHEME=dark PREVIEW_WAIT=40000 scripts/preview.sh sessions '' scripts/preview/queued-send-check.js
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var picked = null;
  var boardNow = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  var running = function (s) {
    s.status = 'working';
    s.statusSince = Date.now() - 10 * 60e3;
    s.live = true;
    s.hosted = false;
    s.asks = false;
    s.suggestions = undefined;
    s.title = 'Queued A';
    return s;
  };
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'send_to_session') {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ message: 'Typed into its Ghostty tab.', opened: false });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (picked && s.key === picked.key) running(s);
          if (picked || l.hidden || l.device || s.visitor || s.archived || s.harness !== 'claude') return;
          running(s);
          picked = { key: s.key, lane: l.id };
        });
      });
      boardNow = board;
      return board;
    });
  };

  // The board as the engine has it, the session changed as `change` says.
  var push = function (change) {
    var b = JSON.parse(JSON.stringify(boardNow));
    b.generatedAt = Date.now();
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === picked.key) Object.assign(s, change); }); });
    boardNow = b;
    window.__emit('board', b);
  };
  var title = function () { var t = q('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var box = function () { return q('.ca-sessinsp .ca-composer:not(.is-approval) textarea'); };
  var row = function () { return picked && document.getElementById('row-' + picked.lane + '::' + picked.key); };
  var key = function (el, init) { el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var type = function (el, text) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };
  // The messages on their way at the end of the conversation, each as (its words, what it says).
  var unsaid = function () {
    return all('.ca-sessinsp .ca-thread-said.is-ahead').map(function (el) {
      var meta = q('.ca-thread-meta .ca-meta', el);
      return { el: el, words: el.querySelector('.ca-thread-you').textContent.trim(), says: meta ? meta.textContent : '' };
    });
  };
  var sayingAll = function (words) { return unsaid().filter(function (u) { return u.words === words; }); };
  var saying = function (words) { return sayingAll(words)[0]; };
  var send = function (words) {
    type(box(), words);
    key(box(), { key: 'Enter' });
    return until(function () { var u = saying(words); return u && u.says === 'Sent'; }, 5000, 'the message sent');
  };

  say('queued send check: waiting for the list');
  until(function () { return row(); }, 20000, 'no Claude Code session to make run')
    .then(function () {
      row().dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Queued A' && !!box(); }, 5000, 'its panel');
    })
    .then(function () { return send('Also check the docs'); })
    .then(function () {
      check('a message that went says Sent', saying('Also check the docs').says === 'Sent');
      // Its harness holds it: it says so, whatever the session's quiet says.
      push({ queuedAt: Date.now(), status: 'idle', statusSince: Date.now() - 60e3 });
      return until(function () { return saying('Also check the docs').says === 'Queued until this turn ends'; }, 5000, 'the message queued');
    })
    .then(function () {
      var u = saying('Also check the docs');
      check('one its harness holds says it’s queued until the turn ends', u.says === 'Queued until this turn ends', u.says);
      check('not that it may not have reached the session', !u.el.classList.contains('is-unconfirmed') && !q('.ca-thread-askrow', u.el));
      // Long after the quiet that would mark it, it's still queued.
      return wait(1500);
    })
    .then(function () {
      check('and stays so while it waits', saying('Also check the docs').says === 'Queued until this turn ends');
      // The turn ends and the queue goes in.
      push({ queuedAt: undefined, status: 'working', statusSince: Date.now(), lastPromptAt: Date.now() });
      return until(function () { var u = saying('Also check the docs'); return !u || u.says === 'Sent'; }, 5000, 'the message out of the queue');
    })
    .then(function () {
      var u = saying('Also check the docs');
      check('out of the queue, it’s sent', !u || u.says === 'Sent', u && u.says);
      // A prompt the board has from within a second of a send is taken for it: the next goes later.
      return wait(1500).then(function () { return send('And the changelog'); });
    })
    .then(function () {
      push({ queuedAt: Date.now() });
      return until(function () { var u = saying('And the changelog'); return u && u.says === 'Queued until this turn ends'; }, 5000, 'the second message queued');
    })
    .then(function () {
      // The board lists what the harness holds: the one sent from here shows once, from the list.
      var at = Date.now();
      push({ queuedAt: at, queued: [{ at: at, text: 'And the changelog' }] });
      return until(function () { return all('.ca-sessinsp .ca-thread-said.is-ahead[data-prompt^="queued:"]').length === 1; }, 5000, 'the board’s queued message');
    })
    .then(function () {
      var shown = sayingAll('And the changelog');
      check('a message the board lists as queued shows once', shown.length === 1, shown.length);
      check('as queued until the turn ends', shown[0] && shown[0].says === 'Queued until this turn ends', shown[0] && shown[0].says);
      // One queued elsewhere (the Codex app's own box, another device) shows too, in the queue's order.
      var at = Date.now();
      push({ queuedAt: at, queued: [{ at: at - 1000, text: 'And the changelog' }, { at: at, text: 'Queued in the app' }] });
      return until(function () { return !!saying('Queued in the app'); }, 5000, 'the message queued elsewhere');
    })
    .then(function () {
      var u = saying('Queued in the app');
      check('one queued elsewhere shows, as queued', u.says === 'Queued until this turn ends', u.says);
      var said = all('.ca-sessinsp .ca-thread-said');
      var tail = said.slice(-2).map(function (el) { return el.querySelector('.ca-thread-you') && el.querySelector('.ca-thread-you').textContent.trim(); });
      check('both at the end of the conversation, in the queue’s order', tail[0] === 'And the changelog' && tail[1] === 'Queued in the app', tail.join(' | '));
      // The queue empties as the turn ends: neither waits any more.
      push({ queuedAt: undefined, queued: undefined, status: 'working', statusSince: Date.now(), lastPromptAt: Date.now() });
      return until(function () { return !saying('Queued in the app'); }, 5000, 'the queue emptied');
    })
    .then(function () {
      check('out of the queue, one queued elsewhere goes', !saying('Queued in the app'));
      var u = saying('And the changelog');
      check('and yours says sent, if it still shows', !u || u.says === 'Sent', u && u.says);
      return wait(6000);
    })
    .then(function () {
      var at = Date.now();
      push({ queuedAt: at, queued: [{ at: at - 1000, text: 'Then the release notes' }, { at: at, text: 'Queued in the app' }], status: 'working' });
      return until(function () { return !!saying('Queued in the app'); }, 5000, 'two queued for the screenshot');
    })
    .then(function () { return wait(300); })
    .then(done)
    .catch(stopped);
})();
