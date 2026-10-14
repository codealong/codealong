// Gives the board the preview shows some headless runs (Session.headless), as an eval's judge
// makes them, for a screenshot of Sessions with them and for runs-check.js: across every
// project, the most recent session is made the one whose command ran four of them (their origin,
// as board.rs `started_by` finds it), three at work and one done, in its worktree; and one more
// runs at work with no session above it, as a script in a terminal of yours would run it. What
// was made is in window.__runs: their worktree's id, and the session's and each run's key and
// title.
//   PREVIEW_INIT=scripts/preview/runs-init.js scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  localStorage.removeItem('codealong.helpersShown');
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var own = [];
      board.lanes.forEach(function (l) {
        if (!l.hidden && !l.device) l.sessions.forEach(function (s) { if (!s.visitor && !s.archived && !s.headless && !s.helper) own.push({ lane: l, session: s }); });
      });
      // The same session on every read of the board, picked by the first.
      var picked = window.__runs ? own.filter(function (o) { return o.session.key === window.__runs.boss.key; })[0] : own.sort(function (a, b) { return b.session.lastActivity - a.session.lastActivity; })[0];
      if (!picked) return board;
      var boss = picked.session;
      var now = Date.now();
      var sec = 1000;
      Object.assign(boss, { status: 'working', statusSince: now - 7 * 60 * sec, snoozedUntil: null, stalledSince: null, background: undefined, asks: false });
      // Every headless run already on the board is left out: these are all there are.
      board.lanes.forEach(function (l) { l.sessions = l.sessions.filter(function (s) { return !s.headless; }); });
      var id = function (n) { return '0000000' + n + '-0000-4000-8000-00000000000' + n; };
      var run = function (n, title, over) {
        var key = 'claude:' + boss.account + ':' + id(n);
        return Object.assign({}, boss, { key: key, id: id(n), shortId: id(n).slice(0, 8), title: title, name: title, headless: true, origin: { key: boss.key, how: 'started' }, helper: undefined, pid: 90000 + n, live: true, status: 'working', statusSince: now - n * 2 * sec, startedAt: now - n * 2 * sec, lastActivity: now - n * sec, lastPromptAt: now - n * 2 * sec, excerpt: null, headline: '`' + boss.account + '` · ' + title, model: 'claude-opus-5-5', emblem: undefined, todos: undefined, insight: undefined, suggestions: undefined, artifacts: undefined, chrome: undefined }, over);
      };
      var runs = [
        run(1, 'cwd-59'),
        run(2, 'cwd-cc'),
        run(3, 'cwd-1c'),
        run(4, 'cwd-06', { status: 'idle', live: false, pid: null, statusSince: now - 20 * sec }),
      ];
      var lone = run(5, 'cwd-zz', { origin: undefined });
      picked.lane.sessions = picked.lane.sessions.concat(runs, [lone]);
      window.__runs = { lane: picked.lane.id, boss: { key: boss.key, title: boss.title }, runs: runs.map(function (s) { return { key: s.key, title: s.title, status: s.status }; }), lone: { key: lone.key, title: lone.title } };
      board.pinned = [];
      return board;
    });
  };
})();
