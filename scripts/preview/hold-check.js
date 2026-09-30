// Checks that the Sessions view's panel holds on the session you're looking at (App.tsx
// `selected`): with nothing clicked yet, a session that comes to the top later doesn't take the
// panel; a session listed under another worktree once its commands move there stays selected;
// and a session started from the quick panel isn't selected when it shows up.
//   PREVIEW_WAIT=60000 scripts/preview.sh sessions '' scripts/preview/hold-check.js
// Every step should say ok.
(function () {
  var out = [];
  var say = function (t) { window.__t = t; };
  setInterval(function () { if (window.__t) document.title = window.__t; }, 100);
  var wait = function (ms) { return new Promise(function (r) { setTimeout(r, ms); }); };
  var until = function (f, ms) {
    var s = Date.now();
    return new Promise(function (res, rej) {
      (function p() { if (f()) return res(); if (Date.now() - s > ms) return rej(new Error('timeout')); setTimeout(p, 10); })();
    });
  };
  var check = function (name, ok, why) { out.push(name + (ok ? ' ok' : ' FAIL') + (why ? ' (' + why + ')' : '')); };

  // This runs before the app's script: four real sessions from different worktrees, A waiting on
  // you, C with a turn to read, B and D idle; every other session idle.
  localStorage.setItem('codealong.project', 'All projects');
  var now = Date.now();
  var plan = [
    { title: 'Hold A', status: 'needs', since: now - 3600e3 },
    { title: 'Hold B', status: 'idle', since: now - 2 * 3600e3 },
    { title: 'Hold C', status: 'unread', since: now - 10 * 60e3 },
    { title: 'Hold D', status: 'idle', since: now - 3 * 3600e3 },
  ];
  var picked = [];
  var board = null;
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (b) {
      var lanes = {};
      b.lanes.forEach(function (l) {
        l.sessions.forEach(function (s) {
          if (s.status === 'needs' || s.status === 'error' || s.status === 'unread' || s.status === 'working') s.status = 'idle';
          s.snoozedUntil = undefined;
          if (l.hidden || l.device || s.visitor || s.archived || lanes[l.id] || picked.length >= plan.length) return;
          lanes[l.id] = true;
          var p = plan[picked.length];
          s.status = p.status;
          s.statusSince = p.since;
          s.lastActivity = now - 60e3;
          s.title = p.title;
          picked.push({ key: s.key, lane: l.id });
        });
      });
      board = b;
      return JSON.parse(JSON.stringify(b));
    });
  };

  var title = function () { var t = document.querySelector('.ca-sessinsp-title'); return t ? t.textContent : null; };
  var row = function (i, lane) { return document.getElementById('row-' + (lane || picked[i].lane) + '::' + picked[i].key); };
  var find = function (b, i) {
    var lane = b.lanes.find(function (l) { return l.id === picked[i].lane; });
    return { lane: lane, session: lane.sessions.find(function (s) { return s.key === picked[i].key; }) };
  };
  var push = function (change) {
    var b = JSON.parse(JSON.stringify(board));
    b.generatedAt = Date.now();
    change(b);
    board = b;
    window.__emit('board', JSON.parse(JSON.stringify(b)));
    return wait(400);
  };

  say('hold check: waiting for the list');
  until(function () { return picked.length === plan.length && row(0) && title(); }, 20000)
    .then(function () { return wait(300); })
    .then(function () {
      check('with nothing clicked the panel shows the top row', title() === 'Hold A', title());
      // B comes to wait on you longer than A, which puts it at the top.
      return push(function (b) {
        var s = find(b, 1).session;
        s.status = 'needs';
        s.statusSince = now - 5 * 3600e3;
      });
    })
    .then(function () {
      var first = document.querySelector('[role="option"]');
      check('a session that comes to the top is listed first', !!first && first.id === row(1).id, first && first.id);
      check('and the panel stays where it was', title() === 'Hold A', title());
      row(2).dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      return until(function () { return title() === 'Hold C'; }, 3000).catch(function () {});
    })
    .then(function () {
      check('clicking a row shows its session', title() === 'Hold C', title());
      // C's commands move to A's worktree, where it's listed now.
      return push(function (b) {
        var from = find(b, 2);
        from.lane.sessions = from.lane.sessions.filter(function (s) { return s !== from.session; });
        find(b, 0).lane.sessions.push(from.session);
      });
    })
    .then(function () {
      check('C is listed under its new worktree', !!row(2, picked[0].lane));
      check('and stays selected there', title() === 'Hold C', title());
      // A start from the quick panel ends with D's key, and D shows up waiting on you.
      window.__emit('quick-start', { phase: 'end', id: 1, key: picked[3].key, message: null });
      return push(function (b) {
        var s = find(b, 3).session;
        s.status = 'working';
        s.statusSince = Date.now();
      });
    })
    .then(function () {
      check('a started session shows', !!row(3));
      check('and isn’t selected when it does', title() === 'Hold C', title());
    })
    .catch(function (e) { check('ran', false, String(e) + ' at ' + title()); })
    .then(function () { say('done: ' + out.join('; ')); });
})();
