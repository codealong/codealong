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
// "$lane:N" and "$session:N" as the project's Nth most recent worktree and session.
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
  board.labels = fixture.labels || [];
  board.todos = (fixture.todos || []).map(function (t, i) {
    return Object.assign({ project: project, description: '', status: 'todo', labels: [], createdAt: now - (i + 1) * 3600e3, updatedAt: now - i * 1800e3, doneAt: null, pending: [] }, t, {
      worktrees: (t.worktrees || []).map(at).filter(Boolean),
      sessions: (t.sessions || []).map(at).filter(Boolean),
      doneAt: t.status === 'done' ? now - 7200e3 : null,
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
      ]).then(function (all) {
        var board = all[1] ? withFixture(all[0], all[1]) : all[0];
        return all[2] ? withUsage(board, all[2]) : board;
      });
    }
    var file = { get_accounts: './accounts.json', found_accounts: './found.json', get_config: './config.json', pr_detail: './pr-detail.json', start_options: './start-options.json', bar_feed: './bar-feed.json' }[cmd];
    if (file) return fetch(file).then(function (r) { return r.ok ? r.json() : Promise.reject(new Error(file + ' is missing')); });
    if (cmd === 'plugin:menu|new') return Promise.resolve([window.__calls.length, 'menu-' + window.__calls.length]);
    // Settings › Agents: one account installed, one out of date, one not installed, one with no folder.
    if (cmd === 'agents_status' || cmd === 'install_agents') {
      return Promise.resolve({
        command: { link: '/usr/local/bin/codealong', target: '/Applications/Codealong.app/Contents/MacOS/codealong', onPath: true },
        skillFound: true,
        accounts: [
          { name: 'claude', harness: 'claude', path: '/Users/you/.claude/skills/codealong', installed: true, current: true },
          { name: 'claude2', harness: 'claude', path: '/Users/you/.claude2/skills/codealong', installed: true, current: false },
          { name: 'codex', harness: 'codex', path: '/Users/you/.codex/skills/codealong', installed: false, current: false },
          { name: 'antigravity', harness: 'antigravity', path: null, installed: false, current: false },
        ],
        errors: cmd === 'install_agents' && (args || {}).accounts && args.accounts.indexOf('codex') >= 0 ? ['codex: /Users/you/.codex/skills/codealong exists and Codealong didn\'t put it there; move it away to install the skill.'] : [],
      });
    }
    // A todo change "shows" on a board that never comes, so the page keeps its own copy of it.
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
