// Checks the Sessions view's panel working through what waits on you (App.tsx `goOn`,
// SessionsView.tsx `waitingQueue`): three sessions that need you, one of them on a call to allow,
// and one whose turn ended. ⌘⏎ in the box sends and goes on, ⌘⏎ on the call allows it and goes
// on, ⌘] skips and ⌘[ goes back, a skipped one comes round again, and a plain ⏎ sends and
// stays. Clicking a row puts the cursor in the box, and Escape there gives the list its keys
// back. (A synthetic press doesn't focus the list first as a real one does.)
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/panel-queue-check.js
(function () {
  // This runs before the app's script: the board it gets has four sessions waiting on you, made
  // from real ones, and nothing else waiting. The second waits on a call to allow.
  localStorage.setItem('codealong.project', 'All projects');
  var now = Date.now();
  var plan = [
    { status: 'needs', since: now - 3 * 3600e3, asks: true },
    { status: 'needs', since: now - 3600e3, asks: false },
    { status: 'needs', since: now - 5 * 60e3, asks: true },
    { status: 'unread', since: now - 10 * 60e3, asks: false },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    if (cmd === 'session_pending' && picked[1] && (args || {}).key === picked[1].key) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ id: 'p1', label: 'Run `git push`', input: 'git push origin HEAD:main', cannot: null });
    }
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
          s.lastActivity = p.since;
          s.asks = p.asks;
          s.hosted = false;
          // No turn of theirs runs, or Escape in the box would stop it rather than give back the keys.
          s.stoppable = false;
          // But they run, so the box sends to them once it knows how (fake-tauri.js `session_way`)
          // rather than resuming them as it would the real ones, which may have ended.
          s.live = true;
          (window.__madeSessions = window.__madeSessions || {})[s.key] = s;
          s.title = 'Queue ' + 'ABCD'[picked.length];
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  var note = function () { var t = document.querySelector('.ca-sessinsp .ca-composer:not(.is-approval) .ca-composer-note'); return t ? t.textContent : ''; };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };

  say('panel queue check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Queue A').then(function () { return wait(50); });
    })
    .then(function () {
      check('clicking a row puts the cursor in its box', document.activeElement === box());
      check('a session that waits on you shows what’s left', /3 more waiting on you/.test(strip()), strip());
      check('the box says what ⌘⏎ does', /⌘⏎ send \+ next/.test(note()), note());
      type(box(), 'Yes, go ahead.');
      key(box(), { key: 'Enter', metaKey: true });
      return on('Queue B');
    })
    .then(function () {
      check('⌘⏎ sends and goes on to the next', title() === 'Queue B' && calls('send_to_session').length === 1, title());
      check('and says it went', /Queue A: sent/.test(strip()), strip());
      check('the next one’s box has the cursor', document.activeElement === box());
      return until(function () { return document.querySelector('.ca-sessinsp .ca-composer.is-approval'); }, 3000).then(function () { return wait(50); });
    })
    .then(function () {
      check('the box says ⌘⏎ allows', /⌘⏎ allow \+ next/.test(note()), note());
      key(box(), { key: 'Enter', metaKey: true });
      return on('Queue C');
    })
    .then(function () {
      var answer = calls('session_answer')[0];
      check('⌘⏎ on a call allows it and goes on', title() === 'Queue C' && !!answer && answer[1].answer === 'allow', title());
      check('and says how that went', /Queue B: Allowed/.test(strip()), strip());
      key(box(), { key: ']', code: 'BracketRight', metaKey: true });
      return on('Queue D');
    })
    .then(function () {
      check('⌘] skips to the next, from the box', title() === 'Queue D', title());
      key(box(), { key: ']', code: 'BracketRight', metaKey: true });
      return on('Queue C');
    })
    .then(function () {
      check('a skipped one comes round again, the ones dealt with don’t', title() === 'Queue C', title());
      key(box(), { key: '[', code: 'BracketLeft', metaKey: true });
      return on('Queue D');
    })
    .then(function () {
      check('⌘[ goes back, from the box', title() === 'Queue D', title());
      key(box(), { key: ']', code: 'BracketRight', metaKey: true });
      return on('Queue C');
    })
    .then(function () {
      type(box(), 'Thanks.');
      key(box(), { key: 'Enter' });
      return wait(500);
    })
    .then(function () {
      check('a plain ⏎ sends and stays', title() === 'Queue C' && calls('send_to_session').length === 2, title());
      check('once it no longer waits on you the strip counts only what else does', !/more waiting on you/.test(strip()), strip());
      // The strip is always there in Sessions, with what's commonly done to a session and its keys.
      var keys = Array.prototype.map.call(document.querySelectorAll('.ca-sessinsp .ca-hotkeys .ca-hotkey'), function (b) { return b.textContent; });
      check('the strip has the common actions with their keys', keys.join('|') === 'Seen⌘E|Snooze⌃⌘1–4|Close terminal⌃⌘W|Archive⌃⌘A|Next⌘]', keys.join('|'));
      key(box(), { key: 'Escape' });
      check('Escape in the box gives the list its keys back', document.activeElement === document.querySelector('[role="listbox"]'), document.activeElement && document.activeElement.className);
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
