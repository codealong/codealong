// A stand-in for Tauri's IPC, for rendering the UI in a plain browser against a snapshot of
// your real data. Every command the UI sends is recorded in `window.__calls`, so a scripted
// check can click something and assert what the app would have done. Reads come from files
// next to the page: board.json (get_board), accounts.json (get_accounts), found.json
// (found_accounts), config.json (get_config), start-options.json (start_options),
// editor-icons.json (editor_icons), bar-feed.json (bar_feed, for the floating bar) and, when
// present, pr-detail.json (pr_detail, for any PR), todos-fixture.json (todos to show in place
// of the board's) and usage-fixture.json (plan usage, likewise).
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
    // With a devices fixture, Settings reads `[devices]` from it.
    if (cmd === 'get_config') {
      return Promise.all([fetch('./config.json').then(function (r) { return r.json(); }), optionalJson('./devices-fixture.json')]).then(function (all) {
        if (all[1] && all[1].cfg) all[0].config.devices = all[1].cfg;
        return all[0];
      });
    }
    // A card pinned to the desktop (preview.sh desktop): PREVIEW_PIN's session, worktree or todo
    // (`<ref>:<key>`), cut from the board as desktop.rs cuts it. A ref alone takes the latest
    // worktree with sessions, the latest session, or the first todo.
    if (cmd === 'desktop_feed') {
      return window.__TAURI_INTERNALS__.invoke('get_board').then(function (board) {
        var pin = window.__CODEALONG_PIN__ || 'session';
        var at = pin.indexOf(':');
        var ref = at < 0 ? pin : pin.slice(0, at);
        var key = at < 0 ? '' : pin.slice(at + 1);
        var lanes = board.lanes.filter(function (l) { return !l.hidden; }).sort(function (a, b) { return b.lastActivity - a.lastActivity; });
        var own = function (l) { return l.sessions.filter(function (s) { return !s.visitor && !s.archived; }); };
        if (!key && ref === 'worktree') key = (lanes.find(function (l) { return own(l).length > 1; }) || lanes[0] || {}).id;
        if (!key && ref === 'session') key = (lanes.flatMap(own).sort(function (a, b) { return b.lastActivity - a.lastActivity; })[0] || {}).key;
        if (!key && ref === 'todo') key = ((board.todos || [])[0] || {}).id;
        var feed = { pin: { ref: ref, key: key }, title: null, loading: false, generatedAt: board.generatedAt, theme: board.theme, terminal: board.apps.terminal, editor: board.apps.editor, editorId: board.apps.editorId, editors: board.apps.editors || [], lanes: [], todos: [], labels: [] };
        var cut = function (l, keep) { return Object.assign({}, l, { sessions: l.sessions.filter(keep) }); };
        if (ref === 'session') {
          var home = board.lanes.find(function (l) { return l.sessions.some(function (s) { return s.key === key && !s.visitor; }); });
          if (home) {
            feed.lanes = [cut(home, function (s) { return s.key === key && !s.visitor; })];
            feed.title = feed.lanes[0].sessions[0].title;
          }
        } else if (ref === 'worktree') {
          var lane = board.lanes.find(function (l) { return l.id === key; });
          if (lane) {
            feed.lanes = [lane];
            feed.title = lane.callsign;
          }
        } else if (ref === 'todo') {
          var t = (board.todos || []).find(function (x) { return x.id === key; });
          if (t) {
            var mine = function (s) { return !s.visitor && t.sessions.indexOf(s.key) >= 0; };
            feed.lanes = board.lanes.filter(function (l) { return t.worktrees.indexOf(l.id) >= 0 || l.sessions.some(mine); }).map(function (l) { return cut(l, mine); });
            feed.todos = [t];
            feed.labels = (board.labels || []).filter(function (l) { return t.labels.indexOf(l.id) >= 0; });
            feed.title = t.title;
          }
        }
        return feed;
      });
    }
    var file = { get_accounts: './accounts.json', found_accounts: './found.json', get_config: './config.json', pr_detail: './pr-detail.json', start_options: './start-options.json', device_start_options: './start-options.json', bar_feed: './bar-feed.json', editor_icons: './editor-icons.json' }[cmd];
    if (file) return fetch(file).then(function (r) { return r.ok ? r.json() : Promise.reject(new Error(file + ' is missing')); });
    if (cmd === 'plugin:menu|new') return Promise.resolve([window.__calls.length, 'menu-' + window.__calls.length]);
    // A listener's id is where its call sits in __calls, counted from 1, so an unlisten can
    // find it again.
    if (cmd === 'plugin:event|listen') return Promise.resolve(window.__calls.length);
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
    // Your other devices from devices-fixture.json (PREVIEW_DEVICES in preview.sh): its view, with
    // some of the board's own worktrees shown as if they were on those devices (`lanesFromBoard`,
    // device short id → how many), their ids, keys and tags made the way the engine makes them.
    if (cmd === 'devices') {
      return Promise.all([optionalJson('./devices-fixture.json'), fetch('./board.json').then(function (r) { return r.json(); })]).then(function (all) {
        var fx = all[0];
        if (!fx) return { enabled: false, me: { key: '', short: '', name: 'This Mac', tailscale: 'off', tailnet: null, address: null, iroh: 'off', remoteControl: false, error: null }, devices: [], phones: [], found: [], asks: [], lanes: [] };
        var view = JSON.parse(JSON.stringify(fx.view));
        view.devices.concat(view.phones || []).forEach(function (d) { if (d.lastSeenAgo) d.lastSeen = Date.now() - d.lastSeenAgo; });
        view.phones = view.phones || [];
        var skip = 1;
        Object.keys(fx.lanesFromBoard || {}).forEach(function (short) {
          var dev = view.devices.find(function (d) { return d.short === short; });
          var picked = all[1].lanes.filter(function (l) { return !l.hidden && l.sessions.length > 0; }).slice(skip, skip + fx.lanesFromBoard[short]);
          skip += fx.lanesFromBoard[short];
          picked.forEach(function (l) {
            var copy = JSON.parse(JSON.stringify(l));
            copy.id = '@' + short + ':' + l.id;
            copy.device = { name: dev ? dev.name : short, short: short, color: dev ? dev.color : 'gray', online: dev ? dev.online : true, lastSeen: dev ? dev.lastSeen : null };
            copy.targets = [];
            copy.defaultTarget = null;
            copy.sessions.forEach(function (x) { x.key = '@' + short + ':' + x.key; x.openTarget = ''; x.pid = null; x.resumeCommand = null; });
            view.lanes.push(copy);
          });
        });
        return view;
      });
    }
    if (cmd === 'link_device') return Promise.resolve('482 915');
    if (cmd === 'device_link_code') return Promise.resolve('cal1f4k3c0d3xq7mzv2r9k8w3n5b6t1y4u7p2e9s3d8g5h1j6l0a4c7v2b');
    // A stand-in for the QR code the engine draws (link_code_qr): the right size and colors, not a real code.
    if (cmd === 'link_code_qr') return Promise.resolve('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 33 33" width="176" height="176" shape-rendering="crispEdges"><rect width="33" height="33" fill="#ffffff"/><path fill="#000000" d="M4 4h1v1h-1zM4 5h1v1h-1zM4 6h1v1h-1zM4 7h1v1h-1zM4 8h1v1h-1zM4 9h1v1h-1zM4 10h1v1h-1zM4 14h1v1h-1zM4 15h1v1h-1zM4 22h1v1h-1zM4 23h1v1h-1zM4 24h1v1h-1zM4 25h1v1h-1zM4 26h1v1h-1zM4 27h1v1h-1zM4 28h1v1h-1zM5 4h1v1h-1zM5 10h1v1h-1zM5 15h1v1h-1zM5 19h1v1h-1zM5 22h1v1h-1zM5 28h1v1h-1zM6 4h1v1h-1zM6 6h1v1h-1zM6 7h1v1h-1zM6 8h1v1h-1zM6 10h1v1h-1zM6 13h1v1h-1zM6 14h1v1h-1zM6 15h1v1h-1zM6 18h1v1h-1zM6 22h1v1h-1zM6 24h1v1h-1zM6 25h1v1h-1zM6 26h1v1h-1zM6 28h1v1h-1zM7 4h1v1h-1zM7 6h1v1h-1zM7 7h1v1h-1zM7 8h1v1h-1zM7 10h1v1h-1zM7 12h1v1h-1zM7 13h1v1h-1zM7 14h1v1h-1zM7 17h1v1h-1zM7 20h1v1h-1zM7 22h1v1h-1zM7 24h1v1h-1zM7 25h1v1h-1zM7 26h1v1h-1zM7 28h1v1h-1zM8 4h1v1h-1zM8 6h1v1h-1zM8 7h1v1h-1zM8 8h1v1h-1zM8 10h1v1h-1zM8 12h1v1h-1zM8 13h1v1h-1zM8 14h1v1h-1zM8 15h1v1h-1zM8 16h1v1h-1zM8 18h1v1h-1zM8 22h1v1h-1zM8 24h1v1h-1zM8 25h1v1h-1zM8 26h1v1h-1zM8 28h1v1h-1zM9 4h1v1h-1zM9 10h1v1h-1zM9 13h1v1h-1zM9 14h1v1h-1zM9 15h1v1h-1zM9 18h1v1h-1zM9 22h1v1h-1zM9 28h1v1h-1zM10 4h1v1h-1zM10 5h1v1h-1zM10 6h1v1h-1zM10 7h1v1h-1zM10 8h1v1h-1zM10 9h1v1h-1zM10 10h1v1h-1zM10 12h1v1h-1zM10 14h1v1h-1zM10 16h1v1h-1zM10 19h1v1h-1zM10 22h1v1h-1zM10 23h1v1h-1zM10 24h1v1h-1zM10 25h1v1h-1zM10 26h1v1h-1zM10 27h1v1h-1zM10 28h1v1h-1zM11 14h1v1h-1zM11 16h1v1h-1zM11 19h1v1h-1zM12 4h1v1h-1zM12 5h1v1h-1zM12 10h1v1h-1zM12 12h1v1h-1zM12 13h1v1h-1zM12 14h1v1h-1zM12 15h1v1h-1zM12 16h1v1h-1zM12 19h1v1h-1zM12 23h1v1h-1zM12 24h1v1h-1zM12 25h1v1h-1zM12 27h1v1h-1zM13 4h1v1h-1zM13 5h1v1h-1zM13 6h1v1h-1zM13 7h1v1h-1zM13 8h1v1h-1zM13 12h1v1h-1zM13 13h1v1h-1zM13 14h1v1h-1zM13 16h1v1h-1zM13 17h1v1h-1zM13 19h1v1h-1zM13 22h1v1h-1zM13 23h1v1h-1zM13 25h1v1h-1zM13 26h1v1h-1zM13 27h1v1h-1zM14 5h1v1h-1zM14 8h1v1h-1zM14 10h1v1h-1zM14 12h1v1h-1zM14 13h1v1h-1zM14 14h1v1h-1zM14 15h1v1h-1zM14 16h1v1h-1zM14 18h1v1h-1zM14 22h1v1h-1zM14 25h1v1h-1zM14 28h1v1h-1zM15 4h1v1h-1zM15 5h1v1h-1zM15 6h1v1h-1zM15 13h1v1h-1zM15 14h1v1h-1zM15 17h1v1h-1zM15 19h1v1h-1zM15 21h1v1h-1zM15 22h1v1h-1zM15 23h1v1h-1zM15 25h1v1h-1zM15 26h1v1h-1zM15 27h1v1h-1zM16 7h1v1h-1zM16 9h1v1h-1zM16 12h1v1h-1zM16 14h1v1h-1zM16 23h1v1h-1zM16 24h1v1h-1zM16 25h1v1h-1zM16 27h1v1h-1zM16 28h1v1h-1zM17 4h1v1h-1zM17 5h1v1h-1zM17 6h1v1h-1zM17 8h1v1h-1zM17 12h1v1h-1zM17 16h1v1h-1zM17 18h1v1h-1zM17 22h1v1h-1zM17 23h1v1h-1zM17 27h1v1h-1zM17 28h1v1h-1zM18 4h1v1h-1zM18 5h1v1h-1zM18 6h1v1h-1zM18 7h1v1h-1zM18 9h1v1h-1zM18 12h1v1h-1zM18 17h1v1h-1zM18 18h1v1h-1zM18 21h1v1h-1zM18 24h1v1h-1zM18 25h1v1h-1zM18 26h1v1h-1zM18 28h1v1h-1zM19 6h1v1h-1zM19 7h1v1h-1zM19 9h1v1h-1zM19 10h1v1h-1zM19 11h1v1h-1zM19 14h1v1h-1zM19 15h1v1h-1zM19 17h1v1h-1zM19 18h1v1h-1zM19 19h1v1h-1zM19 20h1v1h-1zM19 22h1v1h-1zM19 28h1v1h-1zM20 4h1v1h-1zM20 6h1v1h-1zM20 7h1v1h-1zM20 8h1v1h-1zM20 12h1v1h-1zM20 14h1v1h-1zM20 15h1v1h-1zM20 18h1v1h-1zM20 22h1v1h-1zM20 23h1v1h-1zM20 25h1v1h-1zM21 12h1v1h-1zM21 14h1v1h-1zM21 15h1v1h-1zM21 16h1v1h-1zM21 21h1v1h-1zM21 22h1v1h-1zM21 23h1v1h-1zM21 24h1v1h-1zM21 25h1v1h-1zM21 27h1v1h-1zM22 4h1v1h-1zM22 5h1v1h-1zM22 6h1v1h-1zM22 7h1v1h-1zM22 8h1v1h-1zM22 9h1v1h-1zM22 10h1v1h-1zM22 13h1v1h-1zM22 14h1v1h-1zM22 16h1v1h-1zM22 17h1v1h-1zM22 18h1v1h-1zM22 25h1v1h-1zM22 26h1v1h-1zM22 27h1v1h-1zM22 28h1v1h-1zM23 4h1v1h-1zM23 10h1v1h-1zM23 14h1v1h-1zM23 16h1v1h-1zM23 17h1v1h-1zM23 18h1v1h-1zM23 19h1v1h-1zM23 20h1v1h-1zM23 21h1v1h-1zM23 25h1v1h-1zM23 27h1v1h-1zM24 4h1v1h-1zM24 6h1v1h-1zM24 7h1v1h-1zM24 8h1v1h-1zM24 10h1v1h-1zM24 16h1v1h-1zM24 17h1v1h-1zM24 18h1v1h-1zM24 20h1v1h-1zM24 22h1v1h-1zM24 24h1v1h-1zM24 25h1v1h-1zM24 28h1v1h-1zM25 4h1v1h-1zM25 6h1v1h-1zM25 7h1v1h-1zM25 8h1v1h-1zM25 10h1v1h-1zM25 15h1v1h-1zM25 16h1v1h-1zM25 19h1v1h-1zM25 20h1v1h-1zM25 23h1v1h-1zM25 24h1v1h-1zM25 27h1v1h-1zM25 28h1v1h-1zM26 4h1v1h-1zM26 6h1v1h-1zM26 7h1v1h-1zM26 8h1v1h-1zM26 10h1v1h-1zM26 12h1v1h-1zM26 15h1v1h-1zM26 18h1v1h-1zM26 19h1v1h-1zM26 20h1v1h-1zM26 23h1v1h-1zM26 26h1v1h-1zM26 27h1v1h-1zM26 28h1v1h-1zM27 4h1v1h-1zM27 10h1v1h-1zM27 12h1v1h-1zM27 13h1v1h-1zM27 17h1v1h-1zM27 20h1v1h-1zM27 21h1v1h-1zM27 24h1v1h-1zM27 25h1v1h-1zM27 26h1v1h-1zM27 27h1v1h-1zM27 28h1v1h-1zM28 4h1v1h-1zM28 5h1v1h-1zM28 6h1v1h-1zM28 7h1v1h-1zM28 8h1v1h-1zM28 9h1v1h-1zM28 10h1v1h-1zM28 12h1v1h-1zM28 13h1v1h-1zM28 14h1v1h-1zM28 20h1v1h-1zM28 21h1v1h-1zM28 22h1v1h-1zM28 23h1v1h-1zM28 24h1v1h-1zM28 26h1v1h-1zM28 28h1v1h-1z"/></svg>');
    if (cmd === 'answer_link' || cmd === 'unlink_device' || cmd === 'set_device_color') return Promise.resolve(null);
    if (cmd === 'link_with_code') return Promise.resolve('Studio');
    if (cmd === 'search_devices') return Promise.resolve([]);
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
    // A session Codealong runs: the reply it's writing, which a check can set.
    if (cmd === 'session_stream') return Promise.resolve(window.__stream || null);
    if (cmd === 'session_cancel') return Promise.resolve(null);
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
    // Nothing is pinned to the desktop until a check pushes `desktop-pins`.
    if (cmd === 'desktop_pins') return Promise.resolve([]);
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
// Listeners the page has let go of, by id: they hear nothing more.
var unlistened = {};
window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: function (event, eventId) {
    unlistened[eventId] = true;
  },
};
// Pushes an event to the page's listeners, as the engine's emit would: `window.__emit('board', b)`.
window.__emit = function (event, payload) {
  window.__calls.forEach(function (c, i) {
    if (c[0] === 'plugin:event|listen' && c[1].event === event && !unlistened[i + 1]) window['_' + c[1].handler]({ event: event, id: 0, payload: payload });
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
