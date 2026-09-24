// A stand-in for Tauri's IPC, for rendering the UI in a plain browser against a snapshot of
// your real data. Every command the UI sends is recorded in `window.__calls`, so a scripted
// check can click something and assert what the app would have done. Reads come from files
// next to the page: board.json (get_board), accounts.json (get_accounts), config.json
// (get_config), start-options.json (start_options) and, when present, pr-detail.json
// (pr_detail, for any PR) and todos-fixture.json (todos to show in place of the board's).
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
window.__TAURI_INTERNALS__ = {
  invoke: function (cmd, args) {
    window.__calls.push([cmd, args]);
    if (cmd === 'get_board') {
      return Promise.all([
        fetch('./board.json').then(function (r) { return r.json(); }),
        fetch('./todos-fixture.json').then(function (r) { return r.ok ? r.json() : null; }).catch(function () { return null; }),
      ]).then(function (both) { return both[1] ? withFixture(both[0], both[1]) : both[0]; });
    }
    var file = { get_accounts: './accounts.json', get_config: './config.json', pr_detail: './pr-detail.json', start_options: './start-options.json' }[cmd];
    if (file) return fetch(file).then(function (r) { return r.ok ? r.json() : Promise.reject(new Error(file + ' is missing')); });
    if (cmd === 'plugin:menu|new') return Promise.resolve([window.__calls.length, 'menu-' + window.__calls.length]);
    // A todo change "shows" on a board that never comes, so the page keeps its own copy of it.
    if (cmd === 'todo_op') return Promise.resolve(Number.MAX_SAFE_INTEGER);
    if (cmd === 'save_image' || cmd === 'import_image') return Promise.resolve('0123456789abcdef.png');
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
