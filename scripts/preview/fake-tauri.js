// A stand-in for Tauri's IPC, for rendering the UI in a plain browser against a snapshot of
// your real data. Every command the UI sends is recorded in `window.__calls`, so a scripted
// check can click something and assert what the app would have done. Reads come from files
// next to the page: board.json (get_board), accounts.json (get_accounts), found.json
// (found_accounts), config.json (get_config), start-options.json (start_options), bar-feed.json
// (bar_feed, for the floating bar) and, when present, pr-detail.json
// (pr_detail, for any PR), todos-fixture.json (todos to show in place of the board's) and
// usage-fixture.json (plan usage, likewise).
//
// Native menus aren't drawn, but they're made and "shown" as if they were, so a right-click
// runs to the end. `window.__menu()` lists the last menu's items as [label, pick] pairs;
// pick() runs an item as choosing it would.
window.__CODEALONG_PREVIEW__ = true;
window.__calls = [];
// Todos from todos-fixture.json (PREVIEW_TODOS in preview.sh) in place of the board's own, with
// "$lane:N" and "$session:N" as the project's Nth most recent worktree and session, and a
// wait's `sinceAgo` in minutes.
function withFixture(board, fixture) {
  var shown = null;
  try {
    shown = localStorage.getItem('codealong.project');
  } catch (e) {}
  var project = fixture.project || (board.projects.some(function (p) { return p.name === shown; }) ? shown : (board.projects[0] || {}).name);
  var lanes = board.lanes.filter(function (l) { return !l.hidden && l.project === project; }).sort(function (a, b) { return b.lastActivity - a.lastActivity; });
  var sessions = lanes.flatMap(function (l) { return l.sessions.filter(function (s) { return !s.visitor; }); }).sort(function (a, b) { return b.lastActivity - a.lastActivity; });
  var at = function (ref) {
    var m = /^\$(lane|session):(\d+)$/.exec(ref);
    if (!m) return ref;
    var hit = (m[1] === 'lane' ? lanes : sessions)[Number(m[2])];
    return hit ? (m[1] === 'lane' ? hit.id : hit.key) : null;
  };
  var now = Date.now();
  // `jev` in the fixture turns Jev's todo features on: `hints` are todos that look done, and
  // `suggest` is what suggest_todo answers.
  if (fixture.jev) {
    board.jev = Object.assign({ model: 'jev-latest', quick_panel: false, sessions: false, todos: true, pull_requests: false }, board.jev, { todos: true });
    board.todoHints = fixture.jev.hints || {};
  }
  board.labels = fixture.labels || [];
  board.todos = (fixture.todos || []).map(function (t, i) {
    return Object.assign({ project: project, description: '', status: 'todo', labels: [], createdAt: now - (i + 1) * 3600e3, updatedAt: now - i * 1800e3, doneAt: null, pending: [] }, t, {
      worktrees: (t.worktrees || []).map(at).filter(Boolean),
      sessions: (t.sessions || []).map(at).filter(Boolean),
      doneAt: t.status === 'done' ? now - 7200e3 : null,
      waiting: t.waiting ? { note: t.waiting.note || '', since: now - (t.waiting.sinceAgo || 0) * 60e3 } : null,
    });
  });
  return board;
}
// Plan usage from usage-fixture.json (PREVIEW_USAGE in preview.sh) in place of the snapshot's,
// which has no Claude numbers unless it's taken with --usage. Times in it are minutes from now:
// `readAgo` on an account, `resetsIn` on a limit.
function withUsage(board, fixture) {
  var now = Date.now();
  board.usage = fixture.usage.map(function (u) {
    return Object.assign({ plan: null, error: null, needsSignIn: false, loading: false }, u, {
      asOf: u.readAgo == null ? null : now - u.readAgo * 60e3,
      meters: (u.meters || []).map(function (m) {
        return Object.assign({ window: m.label, severity: 'normal', model: null }, m, { resetsAt: m.resetsIn == null ? null : now + m.resetsIn * 60e3 });
      }),
    });
  });
  return board;
}
// Jev's read from insights-fixture.json (PREVIEW_INSIGHTS in preview.sh), merged into the board:
// "$session:N" and "$lane:N" are the Nth most recent session and worktree on it (hidden ones
// left out), or `key` names a session outright, and each entry's other fields replace the
// session's or lane's own. A session's `pending` is what session_pending answers for it.
function withInsights(board, fixture) {
  var lanes = board.lanes.filter(function (l) { return !l.hidden; }).sort(function (a, b) { return b.lastActivity - a.lastActivity; });
  var sessions = lanes.flatMap(function (l) { return l.sessions.filter(function (s) { return !s.visitor && !s.archived; }); }).sort(function (a, b) { return b.lastActivity - a.lastActivity; });
  var at = function (list, ref) { var m = /:(\d+)$/.exec(ref); return m ? list[Number(m[1])] : null; };
  (fixture.sessions || []).forEach(function (e) {
    var hit = e.key ? { key: e.key } : at(sessions, e.ref);
    if (!hit) return;
    board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { if (s.key === hit.key) { var fields = Object.assign({}, e); delete fields.ref; delete fields.key; delete fields.pending; Object.assign(s, fields); } }); });
  });
  (fixture.lanes || []).forEach(function (e) {
    var hit = at(lanes, e.ref);
    if (hit) { var fields = Object.assign({}, e); delete fields.ref; Object.assign(hit, fields); }
  });
  return board;
}
function optionalJson(file) {
  return fetch(file).then(function (r) { return r.ok ? r.json() : null; }).catch(function () { return null; });
}
window.__TAURI_INTERNALS__ = {
  invoke: function (cmd, args, options) {
    window.__calls.push([cmd, args, options]);
    if (cmd === 'get_board') {
      return Promise.all([
        fetch('./board.json').then(function (r) { return r.json(); }),
        optionalJson('./todos-fixture.json'),
        optionalJson('./usage-fixture.json'),
        optionalJson('./route-fixture.json'),
        optionalJson('./insights-fixture.json'),
      ]).then(function (all) {
        var board = all[1] ? withFixture(all[0], all[1]) : all[0];
        if (all[4]) board = withInsights(board, all[4]);
        if (all[3]) board.jev = Object.assign({ model: 'jev-latest', quick_panel: false, sessions: false, todos: false, pull_requests: false }, board.jev, { quick_panel: true });
        return all[2] ? withUsage(board, all[2]) : board;
      });
    }
    var file = { get_accounts: './accounts.json', found_accounts: './found.json', get_config: './config.json', pr_detail: './pr-detail.json', start_options: './start-options.json', bar_feed: './bar-feed.json' }[cmd];
    if (file) return fetch(file).then(function (r) { return r.ok ? r.json() : Promise.reject(new Error(file + ' is missing')); });
    if (cmd === 'plugin:menu|new') return Promise.resolve([window.__calls.length, 'menu-' + window.__calls.length]);
    // Settings › Agents: one account installed, one out of date, one edited, one not installed, one with no folder.
    if (cmd === 'agents_status' || cmd === 'install_agents') {
      return Promise.resolve({
        command: { link: '/usr/local/bin/codealong', target: '/Applications/Codealong.app/Contents/MacOS/codealong', onPath: true },
        skillFound: true,
        accounts: [
          { name: 'claude', harness: 'claude', path: '/Users/you/.claude/skills/codealong', installed: true, current: true, edited: false },
          { name: 'claude2', harness: 'claude', path: '/Users/you/.claude2/skills/codealong', installed: true, current: false, edited: false },
          { name: 'claude-work', harness: 'claude', path: '/Users/you/.claude-work/skills/codealong', installed: true, current: false, edited: true },
          { name: 'codex', harness: 'codex', path: '/Users/you/.codex/skills/codealong', installed: false, current: false, edited: false },
          { name: 'antigravity', harness: 'antigravity', path: null, installed: false, current: false, edited: false },
        ],
        errors: cmd === 'install_agents' && (args || {}).accounts && args.accounts.indexOf('codex') >= 0 ? ['codex: /Users/you/.codex/skills/codealong exists and Codealong didn\'t put it there; move it away to install the skill.'] : [],
      });
    }
    // Text search: the two latest sessions whose titles don't have the words, as if their
    // conversations did, so the panel's text hits can be seen.
    if (cmd === 'search_text') {
      var words = (args || {}).words || [];
      return fetch('./board.json').then(function (r) { return r.json(); }).then(function (board) {
        var all = board.lanes.flatMap(function (l) { return l.sessions.filter(function (s) { return !s.visitor && !s.archived; }); });
        var free = all.filter(function (s) { return !words.every(function (w) { return s.title.toLowerCase().indexOf(w) >= 0; }); });
        free.sort(function (a, b) { return b.lastActivity - a.lastActivity; });
        return {
          hits: free.slice(0, 2).map(function (s, i) {
            return { key: s.key, snippet: '…so the ' + words.join(' and the ') + ' both go through the same path, which is what the earlier change missed.', you: i === 1, at: s.lastActivity };
          }),
          building: false,
          off: false,
        };
      });
    }
    // Jev's answer, from route-fixture.json (PREVIEW_ROUTE): its action and probabilities, with
    // "$session:N" and "$lane:N" as the board's Nth most recent live session and worktree.
    if (cmd === 'route_quick') {
      return Promise.all([fetch('./board.json').then(function (r) { return r.json(); }), optionalJson('./route-fixture.json')]).then(function (all) {
        var board = all[0];
        var fx = all[1] || {};
        var lanes = board.lanes.filter(function (l) { return !l.hidden; }).sort(function (a, b) { return b.lastActivity - a.lastActivity; });
        var sessions = lanes.flatMap(function (l) { return l.sessions.filter(function (s) { return !s.visitor && s.live && s.status !== 'needs'; }); }).sort(function (a, b) { return b.lastPromptAt - a.lastPromptAt; });
        var pick = function (list, id) { return function (x) { var m = /:(\d+)$/.exec(x.id); var hit = list[Number(m ? m[1] : 0)]; return hit ? { id: hit[id], p: x.p } : null; }; };
        return new Promise(function (done) {
          setTimeout(function () {
            done({
              action: fx.action || 'send',
              actions: fx.actions || [['send', 0.8], ['start', 0.2]],
              sessions: (fx.sessions || []).map(pick(sessions, 'key')).filter(Boolean),
              noSession: fx.noSession || 0,
              lanes: (fx.lanes || []).map(pick(lanes, 'id')).filter(Boolean),
              repos: [],
              ownWorktree: fx.ownWorktree || 0,
              duplicate: fx.duplicate ? pick(sessions, 'key')(fx.duplicate) : null,
              model: 'jev-1.13.0',
              ms: 240,
              inputTokens: 9000,
            });
          }, fx.delay || 0);
        });
      });
    }
    // Jev's word on a PR's comments: every other comment asks for a change, as if Jev said so.
    if (cmd === 'pr_comment_asks') {
      var out = {};
      ((args || {}).comments || []).forEach(function (c, i) { out[c.id] = i % 2 === 0 ? 0.85 : 0.1; });
      return Promise.resolve(out);
    }
    // PREVIEW_CANVAS's document for any project, else none: the view tidies a fresh one.
    if (cmd === 'canvas_load') return optionalJson('./canvas-fixture.json');
    if (cmd === 'suggest_todo') {
      return optionalJson('./todos-fixture.json').then(function (fx) {
        return fx && fx.jev && fx.jev.suggest ? fx.jev.suggest : Promise.reject(new Error('Todo suggestions are off.'));
      });
    }
    // A session's conversation: the real newest page for PREVIEW_TRANSCRIPT's session (older
    // pages end there), else its title as your prompt, a tool call and its excerpt as the reply.
    if (cmd === 'session_page') {
      return Promise.all([optionalJson('./transcript-key.json'), optionalJson('./transcript.json'), fetch('./board.json').then(function (r) { return r.json(); })]).then(function (all) {
        var key = (args || {}).key;
        if (all[0] && all[1] && all[0].key === key) return (args || {}).before == null ? all[1] : { items: [], before: null, after: all[1].after };
        var s = null;
        all[2].lanes.forEach(function (l) { l.sessions.forEach(function (x) { if (x.key === key) s = x; }); });
        if (!s) return Promise.reject(new Error("That session isn't on the board any more."));
        var item = function (kind, text, n, extra) { return Object.assign({ at: s.lastActivity - (3 - n) * 60e3, kind: kind, text: text, failed: false, cursor: n, n: 0 }, extra || {}); };
        return { before: null, after: 4, items: [
          item('you', s.title, 0),
          item('tool', 'Bash · `npm test`', 1, { id: 't1', input: 'npm test' }),
          item('result', '', 2, { id: 't1', output: 'Tests  12 passed (12)' }),
          item('agent', s.excerpt || 'Done.', 3),
        ] };
      });
    }
    // What PREVIEW_TRANSCRIPT's session waits on you to allow, as the engine read it, or what an
    // insights fixture says a session waits on; nothing else waits.
    if (cmd === 'session_pending') {
      return Promise.all([optionalJson('./transcript-key.json'), optionalJson('./pending.json'), optionalJson('./insights-fixture.json')]).then(function (all) {
        var key = (args || {}).key;
        var fixed = ((all[2] || {}).sessions || []).filter(function (e) { return e.key === key && e.pending; })[0];
        if (fixed) return fixed.pending;
        return all[0] && all[0].key === key ? all[1] : null;
      });
    }
    if (cmd === 'session_answer') return Promise.resolve((args || {}).answer === 'allow' ? 'Allowed it (in the preview nothing is pressed anywhere).' : 'Denied it (in the preview nothing is pressed anywhere).');
    if (cmd === 'session_newer') return Promise.resolve({ items: [], before: null, after: (args || {}).after || 0 });
    if (cmd === 'send_to_session') return Promise.resolve({ message: 'Sent (in the preview nothing is typed anywhere).', opened: false });
    // A todo change "shows" on a board that never comes, so the page keeps its own copy of it.
    // Close idle terminals: the board's live terminal sessions quiet that long, without the
    // engine's checks for Ghostty, Jev or questions; nothing is closed.
    if (cmd === 'idle_terminals' || cmd === 'close_idle_terminals') {
      return fetch('./board.json').then(function (r) { return r.json(); }).then(function (board) {
        var after = (args || {}).afterMs || 864e5;
        var seen = {};
        var rows = [];
        board.lanes.forEach(function (l) {
          l.sessions.forEach(function (s) {
            if (s.visitor || seen[s.key] || !s.live || s.surface !== 'cli' || s.attachCommand) return;
            seen[s.key] = true;
            rows.push({ s: s, l: l });
          });
        });
        var now = Date.now();
        var quiet = function (r) { return (r.s.status === 'idle' || r.s.status === 'unread') && now - r.s.lastActivity >= after; };
        if (cmd === 'close_idle_terminals') {
          return (args.keys || []).map(function (k, i) {
            return { key: k, title: k, closed: i > 0, message: i > 0 ? 'Closed.' : 'It\u2019s the tab you\u2019re in.' };
          });
        }
        return {
          afterMs: after,
          close: rows.filter(quiet).map(function (r) {
            var days = Math.floor((now - r.s.lastActivity) / 864e5);
            return { key: r.s.key, title: r.s.title, account: r.s.account, harness: r.s.harness, pid: r.s.pid || 0, cwd: r.s.cwd, worktree: r.l.id, callsign: r.l.callsign, lastActivity: r.s.lastActivity, why: (days >= 2 ? 'idle ' + days + ' days' : 'idle ' + Math.floor((now - r.s.lastActivity) / 36e5) + ' hours') + ' \u00b7 last reply asks nothing' };
          }),
          kept: rows.filter(function (r) { return !quiet(r); }).map(function (r) {
            return { key: r.s.key, title: r.s.title, account: r.s.account, why: r.s.status === 'working' ? 'working' : r.s.status === 'needs' ? 'needs you' : 'active 3 hours ago' };
          }),
        };
      });
    }
    if (cmd === 'todo_op') return Promise.resolve(Number.MAX_SAFE_INTEGER);
    // A PNG is kept as an image to show; anything else as a file, by the name it came with.
    if (cmd === 'save_attachment' || cmd === 'import_attachment') {
      var png = args instanceof Uint8Array && args[0] === 0x89;
      var name = cmd === 'import_attachment' ? String(args.path).split('/').pop() : decodeURIComponent(((options || {}).headers || {})['x-name'] || 'file');
      return Promise.resolve(png ? { kind: 'image', path: 'images/0123456789abcdef.png', name: name } : { kind: 'file', path: 'files/fedcba9876543210/' + name.replace(/[^\w.-]+/g, '-'), name: name });
    }
    return Promise.resolve(null);
  },
  transformCallback: function (cb) {
    var id = Math.floor(Math.random() * 1e9);
    window['_' + id] = cb;
    return id;
  },
  unregisterCallback: function () {},
  convertFileSrc: function (p) { return p; },
  metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
};
// Pushes an event to the page's listeners, as the engine's emit would: `window.__emit('board', b)`.
window.__emit = function (event, payload) {
  window.__calls.forEach(function (c) {
    if (c[0] === 'plugin:event|listen' && c[1].event === event) window['_' + c[1].handler]({ event: event, id: 0, payload: payload });
  });
};
window.__menu = function () {
  var made = window.__calls.filter(function (c) { return c[0] === 'plugin:menu|new'; });
  var last = made[made.length - 1];
  // Submenu items are listed as "Parent › Item"; a check item's label ends in " ✓" when checked.
  var flat = function (items, prefix) {
    return (items || []).flatMap(function (i) {
      if (!i.text) return [];
      if (i.items) return flat(i.items, prefix + i.text + ' › ');
      var label = prefix + i.text + (i.checked ? ' ✓' : '');
      return [[label, function () { window['_' + i.handler.id]({ index: 0, message: i.id }); }]];
    });
  };
  return flat(last ? last[1].options.items : [], '');
};
