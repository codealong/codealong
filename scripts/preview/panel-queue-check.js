// Checks the Sessions view's panel working through what waits on you (App.tsx `goOn`,
// SessionsView.tsx `waitingQueue`): four sessions that need you, two of them on a call to allow,
// and one whose turn ended. ⌘⏎ in the box sends and goes on; on a call, ⌘⏎ with words in the box
// denies it with them and goes on, and in an empty box allows it and goes on; ⌘] skips and ⌘[
// goes back, a skipped one comes round again, and a plain ⏎ sends and stays. The strip says how
// the last one went, on one line beside its buttons, and counts nothing; the box's keys are said in
// its foot. Clicking a row puts the cursor in the box, and Escape
// there gives the list its keys back. (A synthetic press doesn't focus the list first as a real
// one does.)
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/panel-queue-check.js
(function () {
  // This runs before the app's script: the board it gets has five sessions waiting on you, made
  // from real ones, and nothing else waiting. The second and third wait on a call to allow.
  localStorage.setItem('codealong.project', 'All projects');
  var now = Date.now();
  var plan = [
    { status: 'needs', since: now - 3 * 3600e3, asks: true },
    { status: 'needs', since: now - 2 * 3600e3, asks: false },
    { status: 'needs', since: now - 3600e3, asks: false },
    { status: 'needs', since: now - 5 * 60e3, asks: true },
    { status: 'unread', since: now - 10 * 60e3, asks: false },
  ];
  var picked = [];
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var calling = [1, 2].filter(function (i) { return picked[i] && (args || {}).key === picked[i].key; })[0];
    if (cmd === 'session_pending' && calling) {
      window.__calls.push([cmd, args, options]);
      return Promise.resolve({ id: 'p' + calling, label: 'Run `git push`', input: 'git push origin HEAD:main', cannot: null });
    }
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var seen = {};
      board.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread') s.status = 'idle';
          s.snoozedUntil = undefined;
          s.raised = undefined;
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
          s.title = 'Queue ' + 'ABCDE'[picked.length];
          picked.push({ key: s.key, lane: l.id });
        });
      });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-sessinsp .ca-reply-queue-note'); return t ? t.textContent : ''; };
  // What the box says, in its foot.
  var note = function () { var t = document.querySelector('.ca-sessinsp .ca-composer-footrow > .ca-composer-note'); return t ? t.textContent : ''; };
  var says = function (what) { return until(function () { return what.test(note()); }, 6000).catch(function () {}); };
  var box = function () { return document.querySelector('.ca-sessinsp .ca-composer textarea'); };
  var row = function (i) { return document.getElementById('row-' + picked[i].lane + '::' + picked[i].key); };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };
  // How the strip is laid out, where its buttons fit on one line: they stay on its one line, and
  // what it says is beside them, cut short there rather than put on a line of its own. Where the
  // buttons don't fit, nothing to check.
  var laidOut = function () {
    var s = q('.ca-sessinsp .ca-reply-queue');
    if (!s) return 'no strip';
    var keys = q('.ca-hotkeys', s);
    var cs = getComputedStyle(s);
    var room = s.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    if (keys.scrollWidth > room) return 'no room: ' + Math.round(keys.scrollWidth) + ' of ' + Math.round(room);
    var sr = s.getBoundingClientRect();
    var k = keys.getBoundingClientRect();
    var noteEl = q('.ca-reply-queue-note', s);
    var n = noteEl.getBoundingClientRect();
    if (k.top - sr.top > parseFloat(cs.paddingTop) + 4) return 'the buttons moved down, ' + Math.round(k.top - sr.top) + 'px';
    if (Math.abs(n.top + n.height / 2 - (k.top + k.height / 2)) >= 4) return 'not beside the buttons';
    if (s.getBoundingClientRect().height > k.height + parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom) + 1) return 'more than one line';
    return 'beside the buttons';
  };

  say('panel queue check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      row(0).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return on('Queue A').then(function () { return wait(50); });
    })
    .then(function () {
      check('clicking a row puts the cursor in its box', document.activeElement === box());
      check('the strip counts nothing of what’s left', !q('.ca-sessinsp .ca-reply-queue .ca-chip'), q('.ca-sessinsp .ca-reply-queue').textContent);
      check('the strip is one line, what it says beside its buttons', /^(beside the buttons|no room)/.test(laidOut()), laidOut());
      window.note('strip at ' + Math.round(q('.ca-sessinsp .ca-reply-queue').getBoundingClientRect().width) + ': ' + laidOut());
      check('the box’s foot says what ⌘⏎ does in the box', /⌘⏎ send \+ next/.test(note()), note());
      check('and nothing is said under the box', !q('.ca-sessinsp .ca-composer:not(.is-approval) > .ca-composer-note'));
      type(box(), 'Yes, go ahead.');
      key(box(), { key: 'Enter', metaKey: true });
      return on('Queue B');
    })
    .then(function () {
      check('⌘⏎ sends and goes on to the next', title() === 'Queue B' && calls('send_to_session').length === 1, title());
      check('and says it went', /Queue A: sent/.test(strip()), strip());
      check('beside the buttons, on the strip’s one line', /^(beside the buttons|no room)/.test(laidOut()), laidOut());
      check('the next one’s box has the cursor', document.activeElement === box());
      return until(function () { return document.querySelector('.ca-sessinsp .ca-composer.is-approval'); }, 3000).then(function () { return wait(50); });
    })
    .then(function () {
      type(box(), 'Use the other remote.');
      return says(/^⏎ denies it and sends this instead · ⌘⏎ \+ next/);
    })
    .then(function () {
      check('with words in the box, it says ⌘⏎ denies with them and goes on', /^⏎ denies it and sends this instead · ⌘⏎ \+ next/.test(note()), note());
      key(box(), { key: 'Enter', metaKey: true });
      return on('Queue C');
    })
    .then(function () {
      var answer = calls('session_answer')[0];
      check('⌘⏎ with words denies the call with them and goes on', title() === 'Queue C' && !!answer && answer[1].answer === 'deny' && answer[1].message === 'Use the other remote.', JSON.stringify(answer && answer[1]));
      check('typing nothing at the prompt', calls('send_to_session').length === 1, calls('send_to_session').length);
      check('and says how that went', /Queue B: Denied it/.test(strip()), strip());
      return until(function () { return document.querySelector('.ca-sessinsp .ca-composer.is-approval'); }, 3000).then(function () { return says(/^⌘⏎ allow \+ next/); });
    })
    .then(function () {
      check('in an empty box it says ⌘⏎ allows', /^⌘⏎ allow \+ next/.test(note()), note());
      key(box(), { key: 'Enter', metaKey: true });
      return on('Queue D');
    })
    .then(function () {
      var answer = calls('session_answer')[1];
      check('⌘⏎ in an empty box allows the call and goes on', title() === 'Queue D' && !!answer && answer[1].answer === 'allow', title());
      check('and says how that went', /Queue C: Allowed/.test(strip()), strip());
      key(box(), { key: ']', code: 'BracketRight', metaKey: true });
      return on('Queue E');
    })
    .then(function () {
      check('⌘] skips to the next, from the box', title() === 'Queue E', title());
      key(box(), { key: ']', code: 'BracketRight', metaKey: true });
      return on('Queue D');
    })
    .then(function () {
      check('a skipped one comes round again, the ones dealt with don’t', title() === 'Queue D', title());
      key(box(), { key: '[', code: 'BracketLeft', metaKey: true });
      return on('Queue E');
    })
    .then(function () {
      check('⌘[ goes back, from the box', title() === 'Queue E', title());
      key(box(), { key: ']', code: 'BracketRight', metaKey: true });
      return on('Queue D');
    })
    .then(function () {
      type(box(), 'Thanks.');
      key(box(), { key: 'Enter' });
      return wait(500);
    })
    .then(function () {
      check('a plain ⏎ sends and stays', title() === 'Queue D' && calls('send_to_session').length === 2, title());
      // The strip is always there in Sessions, with what's commonly done to a session and its keys:
      // Close, or End job where the session is a background job, as the board has it today.
      var keys = Array.prototype.map.call(document.querySelectorAll('.ca-sessinsp .ca-hotkeys .ca-hotkey:not(.is-more)'), function (b) { return b.textContent; });
      check('the strip has the common actions with their keys', /^Seen⌘E\|Done⇧⌘E\|Snooze⌃⌘1–7\|(Close|End job)⌃⌘W\|Archive⌃⌘A\|Next⌘\]$/.test(keys.join('|')), keys.join('|'));
      key(box(), { key: 'Escape' });
      check('Escape in the box gives the list its keys back', document.activeElement === document.querySelector('[role="listbox"]'), document.activeElement && document.activeElement.className);
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(done);
})();
