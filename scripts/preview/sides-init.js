// Gives the board the preview shows a session with side conversations and helpers, for a
// screenshot of Sessions with them and for sides-check.js: across every project, the most recent
// session of yours is made the one asked on the side three times (one answering now, two
// answered and kept) and the one whose agent started two helpers (one at work, one done), in its
// worktree. What was made is in window.__sidesMade: the worktree's id, the session's key and
// title, and each helper's.
//   PREVIEW_INIT=scripts/preview/sides-init.js scripts/preview.sh sessions
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
      var made = window.__sidesMade;
      var picked = made ? own.filter(function (o) { return o.session.key === made.boss.key; })[0] : own.sort(function (a, b) { return b.session.lastActivity - a.session.lastActivity; })[0];
      if (!picked) return board;
      var boss = picked.session;
      var now = Date.now();
      var min = 60 * 1000;
      Object.assign(boss, { status: 'idle', statusSince: now - 3 * min, snoozedUntil: null, stalledSince: null, background: undefined, asks: false, live: true });
      // Every helper and run already on the board is left out: these are all there are.
      board.lanes.forEach(function (l) { l.sessions = l.sessions.filter(function (s) { return !s.headless && !s.helper; }); });
      var id = function (n) { return '0000000' + n + '-0000-4000-8000-00000000000' + n; };
      var helper = function (n, title, over) {
        var key = 'codex:codex:' + id(n);
        return Object.assign({}, boss, { key: key, id: id(n), shortId: id(n).slice(0, 8), harness: 'codex', account: 'codex', title: title, name: title, headless: false, origin: { key: boss.key, how: 'started' }, helper: { parent: boss.key }, pid: 91000 + n, live: true, status: 'working', statusSince: now - n * min, startedAt: now - 20 * min - n * min, lastActivity: now - n * 1000, lastPromptAt: now - 20 * min, excerpt: null, headline: title, model: 'gpt-5.5', emblem: undefined, todos: undefined, insight: undefined, suggestions: undefined, artifacts: undefined, chrome: undefined }, over);
      };
      var helpers = [helper(1, 'Review the queue change'), helper(2, 'Check the migration', { status: 'idle', live: false, pid: null, statusSince: now - 9 * min })];
      picked.lane.sessions = picked.lane.sessions.concat(helpers);
      if (!made) {
        // The side conversations, kept here as the engine keeps them: one answering, asked a
        // minute ago, and two kept from before.
        window.__past = [
          { id: 'side-old', key: boss.key, title: boss.title, startedAt: now - 120 * min, lastAt: now - 118 * min, turns: [{ question: 'Is the queue tested?', answer: 'In `send.rs`.', done: true, at: now - 120 * min }] },
          { id: 'side-mid', key: boss.key, title: boss.title, startedAt: now - 30 * min, lastAt: now - 28 * min, turns: [{ question: 'Why a queue?', answer: 'Order.', done: true, at: now - 30 * min }, { question: 'And without one?', answer: 'Races.', done: true, at: now - 29 * min }] },
        ];
        window.__sides = {};
        window.__sides[boss.key] = { id: 'side-now', key: boss.key, turns: [{ question: 'Can we parallelize this?', answer: '', done: false, at: now - min }], running: true, ended: false, startedAt: now - min };
        window.__sidesMade = { lane: picked.lane.id, boss: { key: boss.key, title: boss.title }, helpers: helpers.map(function (s) { return { key: s.key, title: s.title, status: s.status }; }) };
        // This first read was made before they were kept: list them as the stub's next reads will.
        board.sides = sideGlances();
      }
      board.pinned = [];
      return board;
    });
  };
})();
