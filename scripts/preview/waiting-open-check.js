// Checks the waiting window as it's opened: by the shortcut that goes through what waits on you,
// when the bar isn't docked in the notch, by Window › Waiting on you, or for a session or PR from
// another window (waiting.rs, Waiting.tsx). Three sessions wait on you, and it opens on the
// longest wait, with the rest to go through; ⌘] goes on to the next. Brought up again while it's
// up, it stays where you were; put away and brought up again, it starts afresh on the longest
// wait; asked for a session, it shows that one. With nothing waiting it says so.
//   PREVIEW_WAIT=60000 scripts/preview.sh waiting '' scripts/preview/waiting-open-check.js
(function () {
  var now = Date.now();
  var plan = [
    { status: 'unread', since: now - 10 * 60e3 },
    { status: 'needs', since: now - 3 * 3600e3 },
    { status: 'needs', since: now - 3600e3 },
  ];
  var picked = [];
  // Once nothing waits, every read of the board says so.
  var calm = false;
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
          if (calm) return;
          // The same ones on every read of the board.
          var n = picked.indexOf(s.key);
          if (n < 0) {
            if (l.hidden || s.visitor || s.archived || seen[s.key] || picked.length >= plan.length) return;
            n = picked.length;
            picked.push(s.key);
          }
          seen[s.key] = true;
          var p = plan[n];
          s.status = p.status;
          s.statusSince = p.since;
          s.asks = p.status === 'needs';
          s.title = 'Queue ' + 'CAB'[n];
        });
      });
      // No PR waits on your review either: the three sessions are all that waits.
      (board.prs || []).forEach(function (pr) { pr.reviewRequested = false; });
      return board;
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var strip = function () { var t = document.querySelector('.ca-reply-queue-note'); return t ? t.textContent : ''; };
  // What the strip counts of what else waits on you: the total, then its chip's words.
  var counted = function () { var c = q('.ca-reply-queue-counts'); if (!c) return ''; var n = all('.ca-reply-queue-count', c).reduce(function (sum, x) { return sum + Number(x.textContent); }, 0); return n + ' · ' + c.getAttribute('aria-label'); };
  var empty = function () { var e = document.querySelector('.ca-waiting-empty'); return e ? e.textContent : ''; };
  var on = function (name) { return until(function () { return title() === name; }, 3000).catch(function () {}); };

  say('waiting open check: waiting for the window');
  until(function () { return picked.length === plan.length && !!title(); }, 20000)
    .then(function () { return on('Queue A'); })
    .then(function () {
      check('it opens on the longest wait', title() === 'Queue A', title());
      check('with the rest to go through', /^2 · Waiting on you after this one: /.test(counted()), counted());
      window.dispatchEvent(new KeyboardEvent('keydown', { key: ']', code: 'BracketRight', metaKey: true, bubbles: true, cancelable: true }));
      return on('Queue B');
    })
    .then(function () {
      check('⌘] goes on to the next', title() === 'Queue B', title());
      // The shortcut, or Window › Waiting on you, while it's up: it comes forward as it was.
      window.__emit('waiting-open', null);
      return wait(300);
    })
    .then(function () {
      check('brought up again while it’s up, it stays where you were', title() === 'Queue B', title());
      window.__emit('waiting-hidden', null);
      return wait(100);
    })
    .then(function () {
      window.__emit('waiting-open', null);
      return on('Queue A');
    })
    .then(function () {
      check('put away and brought up again, it starts on the longest wait', title() === 'Queue A', title());
      // ⇥ on a session in the quick panel, or the floating bar's reviews pill.
      window.__emit('waiting-open', { key: picked[0] });
      return on('Queue C');
    })
    .then(function () {
      check('asked for one, it shows that one', title() === 'Queue C', title());
      // Put away, and nothing waits any more by the time it's brought up again, at once.
      calm = true;
      window.__emit('waiting-hidden', null);
      window.__emit('waiting-open', null);
      return until(function () { return /Nothing waits on you/.test(empty()); }, 3000).catch(function () {});
    })
    .then(function () {
      check('with nothing waiting it says so', /Nothing waits on you/.test(empty()) && !title(), title() || empty());
    })
    .then(done)
    .catch(function (e) { stopped(e.message + ' at ' + title()); });
})();
