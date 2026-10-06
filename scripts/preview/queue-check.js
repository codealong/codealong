// Checks the waiting window working through what waits on you (Waiting.tsx, quick.ts
// `queueRows`): three sessions that need you and one whose turn ended, the longest wait first.
// It opens on the first; ⌘] skips, ⌘E on one that has started running does too, archiving,
// sending with ⌘⏎ and marking seen go on to the next, ⏎ sends and stays, a skipped one comes round
// again, and after the last the window says nothing waits, until one comes to wait on you. The
// other keys for getting about are the ones Sessions and the notch have (queue.ts): ⌥⇧↑ ⌥⇧↓,
// ⌃- ⌃⇧- and ⌥↑ ⌥↓ along what waits.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/queue-check.js
(function () {
  // This runs before the app's script: the board it gets has four sessions waiting on you, made
  // from real ones, and nothing else waiting.
  var now = Date.now();
  var plan = [
    { status: 'needs', since: now - 3 * 3600e3 },
    { status: 'needs', since: now - 3600e3 },
    { status: 'needs', since: now - 5 * 60e3 },
    { status: 'unread', since: now - 10 * 60e3 },
  ];
  var picked = [];
  // The board as the window got it, to push again with a session's status changed.
  var boardNow;
  var push = function (key, status, since) {
    var b = JSON.parse(JSON.stringify(boardNow));
    // A board read after what the window did, as the engine's next one is: what it did ahead of
    // the engine (marking seen) gives way to it.
    b.generatedAt = Date.now();
    b.lanes.forEach(function (l) {
      l.sessions.forEach(function (s) {
        if (s.key !== key) return;
        s.status = status;
        s.asks = status !== 'working';
        if (since) s.statusSince = since;
      });
    });
    boardNow = b;
    window.__emit('board', b);
  };
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    // The engine answers with the time of the board that shows it, which a newer one settles.
    if (cmd === 'mark_seen') return p.then(function () { return Date.now(); });
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
  var box = function () { return document.querySelector('.ca-reply .ca-composer textarea'); };
  var button = function (label) { return document.querySelector('.ca-reply .ca-sessinsp-head button[aria-label="' + label + '"]'); };
  var empty = function () { var e = document.querySelector('.ca-waiting-empty'); return e ? e.textContent : ''; };
  var called = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }).length; };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };

  say('queue check: waiting for the first that waits');
  until(function () { return picked.length === plan.length && !!title(); }, 20000)
    .then(function () { return on('Queue A'); })
    .then(function () {
      check('it opens on the longest wait', title() === 'Queue A', title());
      check('the strip says what’s left', /3 more waiting on you/.test(strip()), strip());
      check('the box has the cursor', document.activeElement === box());
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
      return on('Queue C').then(function () { check('⌥↓ steps along what waits', title() === 'Queue C', title()); });
    })
    .then(function () {
      key(window, { key: 'ArrowUp', altKey: true });
      return on('Queue B').then(function () { check('⌥↑ steps back', title() === 'Queue B' && /3 more waiting on you/.test(strip()), title() + ' / ' + strip()); });
    })
    .then(function () {
      check('the header leaves archiving to its menu and its shortcut', !button('Archive') && !button('Snooze') && !!button('More'));
      key(window, { key: 'a', code: 'KeyA', metaKey: true, ctrlKey: true });
      return on('Queue C');
    })
    .then(function () {
      check('archiving goes on to the next', title() === 'Queue C' && called('archive_session') === 1, title());
      check('and says how the last one went', /Queue B: archived/.test(strip()), strip());
      type(box(), 'Yes, go ahead.');
      key(box(), { key: 'Enter', metaKey: true });
      return on('Queue D');
    })
    .then(function () {
      check('⌘⏎ sends and goes on to the next', title() === 'Queue D' && called('send_to_session') === 1, title());
      check('and says it went', /Queue C: sent/.test(strip()), strip());
      key(window, { key: 'e', code: 'KeyE', metaKey: true });
      return on('Queue A');
    })
    .then(function () {
      check('⌘E marks a finished turn seen and goes on', title() === 'Queue A' && called('mark_seen') === 1, title());
      check('a skipped one comes round again', title() === 'Queue A', title());
      var next = Array.prototype.find.call(document.querySelectorAll('.ca-reply-queue .ca-hotkey'), function (b) { return /^Next/.test(b.textContent); });
      check('nothing left after it', !!next && next.disabled, next ? next.outerHTML : 'no Next');
      // Marked seen, the last one leaves what waits on you.
      key(window, { key: 'e', code: 'KeyE', metaKey: true });
      return until(function () { return /Nothing waits on you/.test(empty()); }, 3000).then(
        function () { check('after the last the window says nothing waits', true); },
        function () { check('after the last the window says nothing waits', false, title()); },
      );
    })
    .then(function () {
      // A's reply comes in: its turn ended after you answered, so it waits on you anew.
      push(picked[0], 'unread', Date.now());
      return on('Queue A');
    })
    .then(function () {
      check('one that comes to wait on you shows by itself', title() === 'Queue A', title() || empty());
      type(box(), 'One more thing.');
      key(box(), { key: 'Enter' });
      return wait(500);
    })
    .then(function () { check('⏎ sends and stays, to watch the reply', title() === 'Queue A' && called('send_to_session') === 2, title()); })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
