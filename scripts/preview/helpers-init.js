// Gives the board the preview shows some helpers (Session.helper), for a screenshot of Sessions
// with them and for helpers-check.js: across every project, the most recent session is made idle,
// and the next three its helpers (started by it as helpers), one working, one waiting on you to
// allow a call and one finished, which Codealong runs (so it opens in its panel) and Sessions
// counts on its row but doesn't list. What was
// picked is in window.__helpers: the session's key, title and worktree (lane id), and each
// helper's, with its account.
//   PREVIEW_INIT=scripts/preview/helpers-init.js scripts/preview.sh sessions
(function () {
  localStorage.setItem('codealong.project', 'All projects');
  var real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = function (cmd, args, options) {
    var p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then(function (board) {
      var own = [];
      board.lanes.forEach(function (l) {
        if (!l.hidden) l.sessions.forEach(function (s) { if (!s.visitor && !s.archived) own.push(s); });
      });
      // The same four on every read of the board, picked by the first: not ones that work for
      // something else already (an automatic review, a headless run), which Sessions doesn't
      // list as your own, whatever this makes of them.
      var mine = own.filter(function (s) { return !s.helper && !s.headless; });
      var keys = window.__helpers ? [window.__helpers.boss.key].concat(window.__helpers.kids.map(function (k) { return k.key; })) : mine.sort(function (a, b) { return b.lastActivity - a.lastActivity; }).slice(0, 4).map(function (s) { return s.key; });
      var find = function (key) { return own.filter(function (s) { return s.key === key; })[0]; };
      var boss = find(keys[0]);
      if (!boss || keys.length < 4) return board;
      // Its own helpers, if it has any, are left out: the three below are all it has.
      own.forEach(function (s) {
        if (keys.indexOf(s.key) < 0 && s.helper && s.helper.parent === boss.key) delete s.helper;
      });
      var now = Date.now();
      var min = 60000;
      // What it and the finished helper last said have a word in common, for a search that finds both.
      Object.assign(boss, { status: 'idle', statusSince: now - 20 * min, lastActivity: now - 10 * min, asks: false, snoozedUntil: null, stalledSince: null, background: undefined, excerpt: 'Started a second opinion (zqhelpers)' });
      var helper = function (key, over) {
        var s = find(key);
        Object.assign(s, { helper: { parent: boss.key }, origin: { key: boss.key, how: 'started' }, snoozedUntil: null, stalledSince: null, background: undefined, asks: false, quiet: false }, over);
        return s;
      };
      var kids = [
        helper(keys[1], { status: 'working', statusSince: now - 3 * min, lastActivity: now - min, lastPromptAt: now - 3 * min, title: 'Review the diff for races' }),
        helper(keys[2], { status: 'needs', statusSince: now - 2 * min, lastActivity: now - 2 * min, lastPromptAt: now - 4 * min, excerpt: 'Bash · `gh pr diff 12`', title: 'Check the migration on staging' }),
        helper(keys[3], { status: 'idle', statusSince: now - 5 * min, lastActivity: now - 5 * min, lastPromptAt: now - 8 * min, title: 'Second opinion on the API shape', excerpt: 'The API shape holds up (zqhelpers)', hosted: true, openTarget: 's|' + keys[3] + '|hosted' }),
      ];
      var laneOf = function (s) {
        return board.lanes.filter(function (l) { return l.sessions.indexOf(s) >= 0; })[0].id;
      };
      window.__helpers = { boss: { key: boss.key, title: boss.title, lane: laneOf(boss) }, kids: kids.map(function (s) { return { key: s.key, title: s.title, status: s.status, lane: laneOf(s), account: s.account }; }) };
      board.pinned = [];
      return board;
    });
  };
})();
