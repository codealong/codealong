// Checks that what takes a session off what waits on you goes on to the next that does, from the
// Sessions view's panel (App.tsx `leftAndOn`, `seenAndOn`): four sessions wait on you and one
// runs. ⌃⌘1 snoozes and goes on, ⌃⌘A archives and goes on, ⌃⌘W closes its terminal and goes on,
// each from the box, with the cursor in the next one's box and the strip saying how the last one
// went; the strip's Snooze button does too; ⌘E on the one that runs, which has nothing to mark
// seen, goes on as ⌘] does; and archiving one that didn't wait on you goes on to nothing.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/leave-next-check.js
(function () {
  // This runs before the app's script: the board it gets has five sessions waiting on you and one
  // running, made from real ones on this device, and nothing else waiting. The third has a
  // terminal to close.
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.setItem('codealong.view', 'sessions');
  localStorage.removeItem('codealong.sessionPanelMax');
  var now = Date.now();
  var plan = [
    { status: 'needs', since: now - 3 * 3600e3, asks: true },
    { status: 'needs', since: now - 3600e3, asks: true },
    { status: 'unread', since: now - 20 * 60e3, asks: false },
    { status: 'unread', since: now - 10 * 60e3, asks: false },
    { status: 'unread', since: now - 5 * 60e3, asks: false },
    { status: 'working', since: now - 60e3, asks: false },
  ];
  var picked = [];
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
          if (l.hidden || l.device || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
          seen[s.key] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = p.since;
          s.asks = p.asks;
          s.hosted = false;
          s.stoppable = false;
          // One in a terminal of its own, which closes.
          s.live = true;
          s.surface = 'cli';
          s.pid = 4242 + picked.length;
          s.title = 'Queue ' + 'ABCDEW'[picked.length];
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var key = function (init) { (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, init))); };
  var chord = function (k, code) { key({ key: k, code: code, ctrlKey: true, metaKey: true }); };
  var calls = function (cmd) { return window.__calls.filter(function (c) { return c[0] === cmd; }).map(function (c) { return c[1] || {}; }); };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  var hotkey = function (label) {
    return Array.prototype.filter.call(document.querySelectorAll('.ca-sessinsp .ca-hotkeys .ca-hotkey'), function (b) { return b.textContent.indexOf(label) === 0; })[0];
  };

  say('leave next check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Queue A').then(function () { return wait(50); });
    })
    .then(function () {
      check('the cursor is in the box', document.activeElement === box());
      chord('1', 'Digit1');
      return on('Queue B').then(function () { return wait(50); });
    })
    .then(function () {
      var snoozed = calls('snooze_sessions')[0];
      check('⌃⌘1 snoozes and goes on to the next', title() === 'Queue B' && !!snoozed && snoozed.keys[0] === picked[0].key, title());
      check('and says how that went', /Queue A: snoozed\./.test(strip()), strip());
      check('the next one’s box has the cursor', document.activeElement === box());
      chord('a', 'KeyA');
      return on('Queue C').then(function () { return wait(50); });
    })
    .then(function () {
      var archived = calls('archive_session')[0];
      check('⌃⌘A archives and goes on', title() === 'Queue C' && !!archived && archived.key === picked[1].key && archived.archived === true, title());
      check('and says so', /Queue B: archived\./.test(strip()), strip());
      check('with the cursor in its box', document.activeElement === box());
      chord('w', 'KeyW');
      return on('Queue D').then(function () { return wait(50); });
    })
    .then(function () {
      var closed = calls('close_session')[0];
      check('⌃⌘W closes its terminal and goes on', title() === 'Queue D' && !!closed && closed.key === picked[2].key, title());
      check('and says so', /Queue C: closed\./.test(strip()), strip());
      // The one that runs: nothing to mark seen, so ⌘E goes on as ⌘] does.
      row(5).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Queue W').then(function () { return wait(50); });
    })
    .then(function () {
      var seen = hotkey('Seen');
      check('Seen is dimmed on a session that runs', !!seen && seen.disabled, seen ? seen.title : 'no button');
      check('and says what ⌘E does there', !!seen && /goes on to the next/.test(seen.title), seen ? seen.title : '');
      key({ key: 'e', code: 'KeyE', metaKey: true });
      return on('Queue D').then(function () { return wait(50); });
    })
    .then(function () {
      check('⌘E on a session that runs goes on to the next', title() === 'Queue D', title());
      check('and marks nothing seen', calls('mark_seen').length === 0, JSON.stringify(calls('mark_seen')));
      check('with the cursor in its box', document.activeElement === box());
      // The strip's own button, and a pick from its menu.
      var snooze = hotkey('Snooze');
      snooze.click();
      var first = function () { return window.__menu().filter(function (m) { return /15 minutes/.test(m[0]); })[0]; };
      return until(first, 2000).then(function () { first()[1](); }).catch(function () {});
    })
    .then(function () { return on('Queue E'); })
    .then(function () {
      check('the strip’s Snooze goes on too', title() === 'Queue E' && /Queue D: snoozed\./.test(strip()), title() + ' / ' + strip());
      // One that didn't wait on you: archived, it goes on to nothing.
      row(5).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Queue W').then(function () { return wait(50); });
    })
    .then(function () {
      chord('a', 'KeyA');
      return wait(400);
    })
    .then(function () {
      var archived = calls('archive_session');
      check('archiving one that didn’t wait on you is no step through the queue', archived.length === 2 && archived[1].key === picked[5].key && !/archived\./.test(strip()), title() + ' / ' + strip());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
