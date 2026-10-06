// Checks the quick panel's reply view working through what waits on you (Quick.tsx, quick.ts
// `queueAfter`): three sessions that need you and one whose turn ended, the longest wait first;
// ⇥ answers the first, ⌘] skips, ⌘E on one that has started running does too, archiving, sending and marking seen go on to the next, a skipped
// one comes round again, and after the last the panel closes. The other keys for getting about are
// the ones Sessions and the notch have (queue.ts): ⌥⇧↑ ⌥⇧↓, ⌃- ⌃⇧- and ⌥↑ ⌥↓ along the list.
//   PREVIEW_WAIT=60000 scripts/preview.sh quick '' scripts/preview/queue-check.js
(function () {
  // This runs before the app's script: the board it gets has four sessions waiting on you, made
  // from real ones, and nothing else waiting.
  localStorage.setItem('codealong.quick.project', 'All projects');
  var now = Date.now();
  var plan = [
    { status: 'needs', since: now - 3 * 3600e3 },
    { status: 'needs', since: now - 3600e3 },
    { status: 'needs', since: now - 5 * 60e3 },
    { status: 'unread', since: now - 10 * 60e3 },
  ];
  var picked = [];
  // The board as the panel got it, to push again with a session's status changed.
  var boardNow;
  var push = function (key, status) {
    var b = JSON.parse(JSON.stringify(boardNow));
    b.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === key) { s.status = status; if (status === 'working') s.asks = false; else s.asks = true; } }); });
    window.__emit('board', b);
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (l.hidden || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.asks = p.status === 'needs';
          s.title = 'Queue ' + 'ABCD'[picked.length];
          picked.push(s.key);
        });
      });
      // No PR waits on your review either: the queue is these four sessions alone.
      (board.prs || []).forEach(function (pr) { pr.reviewRequested = false; });
      boardNow = board;
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-reply-queue-note'); return t ? t.textContent : ''; };
  var input = function () { return document.querySelector('.ca-quick-input textarea'); };
  var box = function () { return document.querySelector('.ca-reply .ca-composer textarea'); };
  var button = function (label) { return document.querySelector('.ca-reply .ca-sessinsp-head button[aria-label="' + label + '"]'); };
  var called = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }).length; };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };

  say('queue check: waiting for the list');
  until(function () { return picked.length === plan.length && document.querySelectorAll('.ca-quick-row, [role="option"]').length > 0; }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      key(input(), { key: 'Tab' });
      return on('Queue A');
    })
    .then(function () {
      check('⇥ answers the longest wait first', title() === 'Queue A', title());
      check('the strip says what’s left', /3 more waiting on you/.test(strip()), strip());
      key(window, { key: ']', code: 'BracketRight', metaKey: true });
      return on('Queue B');
    })
    .then(function () {
      // B starts running while it shows: nothing to mark seen, so ⌘E goes on as ⌘] does.
      push(picked[1], 'working');
      return wait(300);
    })
    .then(function () {
      check('on one that runs the strip says what waits, not what more', /^3 waiting on you/.test(strip()), strip());
      key(window, { key: 'e', code: 'KeyE', metaKey: true });
      return on('Queue A');
    })
    .then(function () {
      // From one that doesn't wait on you, the next is the first that does.
      check('⌘E on one that runs goes on to what waits, and marks nothing seen', title() === 'Queue A' && called('mark_seen') === 0, title());
      // It waits on you again.
      push(picked[1], 'needs');
      return wait(300);
    })
    .then(function () {
      key(window, { key: ']', code: 'BracketRight', metaKey: true });
      return on('Queue B');
    })
    .then(function () {
      // Back to A, long enough there for it to count as looked at (history.ts), and on again.
      key(window, { key: 'ArrowUp', altKey: true, shiftKey: true });
      return on('Queue A').then(function () { check('⌥⇧↑ goes back to the one before', title() === 'Queue A', title()); return wait(1100); });
    })
    .then(function () {
      key(window, { key: 'ArrowDown', altKey: true, shiftKey: true });
      return on('Queue B').then(function () { check('⌥⇧↓ goes on, as ⌘] does', title() === 'Queue B', title()); });
    })
    .then(function () {
      key(window, { key: '-', code: 'Minus', ctrlKey: true });
      return on('Queue A').then(function () { check('⌃- goes back to the one looked at before', title() === 'Queue A', title()); });
    })
    .then(function () {
      key(window, { key: '_', code: 'Minus', ctrlKey: true, shiftKey: true });
      return on('Queue B').then(function () { check('⌃⇧- goes forward again', title() === 'Queue B', title()); });
    })
    .then(function () {
      key(window, { key: 'ArrowDown', altKey: true });
      return on('Queue C').then(function () { check('⌥↓ steps along the list', title() === 'Queue C', title()); });
    })
    .then(function () {
      key(window, { key: 'ArrowUp', altKey: true });
      return on('Queue B').then(function () { check('⌥↑ steps back', title() === 'Queue B' && /3 more waiting on you/.test(strip()), title() + ' / ' + strip()); });
    })
    .then(function () {
      check('⌘] skips to the next', title() === 'Queue B', title());
      check('the header leaves archiving to its menu and its shortcut', !button('Archive') && !button('Snooze') && !!button('More'));
      key(window, { key: 'a', code: 'KeyA', metaKey: true, ctrlKey: true });
      return on('Queue C');
    })
    .then(function () {
      check('archiving goes on to the next', title() === 'Queue C' && called('archive_session') === 1, title());
      check('and says how the last one went', /Queue B: archived/.test(strip()), strip());
      type(box(), 'Yes, go ahead.');
      key(box(), { key: 'Enter' });
      return on('Queue D');
    })
    .then(function () {
      check('sending goes on to the next', title() === 'Queue D' && called('send_to_session') === 1, title());
      check('and says it went', /Queue C: sent/.test(strip()), strip());
      button('Mark as seen').click();
      return on('Queue A');
    })
    .then(function () {
      check('a skipped one comes round again', title() === 'Queue A', title());
      check('nothing left after it', !document.querySelector('.ca-reply-queue button'));
      var hides = called('hide_quick');
      type(box(), 'Thanks.');
      key(box(), { key: 'Enter' });
      return until(function () { return called('hide_quick') > hides; }, 3000).then(
        function () { check('after the last the panel closes', true); },
        function () { check('after the last the panel closes', false, title()); },
      );
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
