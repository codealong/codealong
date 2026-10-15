// A stand-in for Tauri's IPC, for rendering the UI in a plain browser against a snapshot of
// your real data. Every command the UI sends is recorded in `window.__calls`, so a scripted
// check can click something and assert what the app would have done. Reads come from files
// next to the page: board.json (get_board), accounts.json (get_accounts), found.json
// (found_accounts), config.json (get_config), start-options.json (start_options),
// editor-icons.json (editor_icons), shell-commands.json (shell_commands), bar-feed.json
// (bar_feed, for the floating bar) and, when present, pr-detail.json (pr_detail, for any PR),
// todos-fixture.json (todos to show in place of the board's), usage-fixture.json (plan usage,
// likewise) and skills.json or skills-fixture.json (skills, for the `/` lists).
//
// Native menus aren't drawn, but they're made and "shown" as if they were, so a right-click
// runs to the end. `window.__menu()` lists the last menu's items as [label, pick] pairs;
// pick() runs an item as choosing it would.
//
// A command with no answer here resolves null, which is right for the many that only do
// something, and wrong for a new one the page shows the answer of: `window.__unknown` names each
// command that went unanswered, once, and prelude.js says them beside a check's result.
window.__CODEALONG_PREVIEW__ = true;
window.__calls = [];
window.__unknown = [];
// Todos from todos-fixture.json (PREVIEW_TODOS in preview.sh) in place of the board's own, with
// "$lane:N" and "$session:N" as the project's Nth most recent worktree and session (`primary`
// names one, made primary a minute ago), a wait's `sinceAgo` in minutes, and `dueIn` as a due
// date that many days from today.
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
      due: t.dueIn == null ? t.due || null : dayFrom(now, t.dueIn),
      primary: t.primary && at(t.primary) ? { session: at(t.primary), since: now - 60e3 } : null,
    });
  });
  return board;
}
// The day `days` from `now`'s, as a due date is written.
function dayFrom(now, days) {
  var d = new Date(now);
  d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + days);
  return d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
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
// Emblems on every session (PREVIEW_EMBLEMS in preview.sh), made up from its key, the same every
// time: an icon from emblems.rs's list, one of its pairs of colors either way round, and one of
// its eight lights, as `[jev] emblems` would give them once Jev had answered.
var EMBLEM_ICONS = ['bug', 'palette', 'database', 'rocket', 'book', 'compass', 'bolt', 'key', 'device-mobile', 'test-pipe', 'chart-line', 'terminal-2', 'sparkles', 'recycle', 'network', 'puzzle'];
var EMBLEM_PAIRS = [['red', 'rose'], ['red', 'pink'], ['red', 'magenta'], ['red', 'violet'], ['rose', 'pink'], ['rose', 'magenta'], ['rose', 'violet'], ['rose', 'purple'], ['pink', 'magenta'], ['pink', 'violet'], ['pink', 'purple'], ['pink', 'indigo'], ['magenta', 'violet'], ['magenta', 'purple'], ['magenta', 'indigo'], ['magenta', 'blue'], ['violet', 'purple'], ['violet', 'indigo'], ['violet', 'blue'], ['purple', 'indigo'], ['purple', 'blue'], ['purple', 'sky'], ['indigo', 'blue'], ['indigo', 'sky'], ['indigo', 'teal'], ['blue', 'sky'], ['blue', 'teal'], ['blue', 'mint'], ['sky', 'teal'], ['sky', 'mint'], ['sky', 'green'], ['teal', 'mint'], ['teal', 'green'], ['teal', 'lime'], ['mint', 'green'], ['mint', 'lime'], ['green', 'lime']];
function emblemOf(key) {
  var pick = function (list, salt) { return list[Math.floor(hashOf(salt + key) * list.length)]; };
  var pair = pick(EMBLEM_PAIRS, 'p');
  var swap = hashOf('s' + key) < 0.5;
  return { color: pair[swap ? 1 : 0], accent: pair[swap ? 0 : 1], light: Math.floor(hashOf('l' + key) * 8), icon: pick(EMBLEM_ICONS, 'i') };
}
function withEmblems(board) {
  board.lanes.forEach(function (l) { l.sessions.forEach(function (s) { s.emblem = emblemOf(s.key); }); });
  board.jev = Object.assign({ model: 'jev-latest', quick_panel: false, sessions: false, todos: false, pull_requests: false }, board.jev, { emblems: true });
  return board;
}
function optionalJson(file) {
  return fetch(file).then(function (r) { return r.ok ? r.json() : null; }).catch(function () { return null; });
}
// What the token ledger would report (ledger.rs `Report`), for the Usage window: the report in
// tokens-fixture.json (PREVIEW_TOKENS; `--snapshot --tokens` prints a real one), else one made up
// from the board, the same every time: its accounts, the worktrees' folders and sessions, a cost
// for each day that rises and falls, and the first account paying $200 a month.
function hashOf(s) {
  var h = 2166136261;
  for (var i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return ((h >>> 0) % 1000) / 1000;
}
function tokensOf(api) {
  var out = Math.round(api * 9000);
  return { input: Math.round(api * 400), cacheRead: Math.round(api * 2600000), cacheWrite: Math.round(api * 40000), output: out, reasoning: Math.round(out * 0.3) };
}
function spentOf(key, api, extra) {
  var t = tokensOf(api);
  return Object.assign({ key: key, api: api, paid: 0, tokens: t, calls: Math.round(api * 11), unpriced: 0, lastAt: Date.now() - Math.round(hashOf(key) * 864e5) }, extra || {});
}
function sampleReport(board, days, account) {
  var accounts = (board.usage || []).map(function (u) { return u.account; }).filter(function (a) { return !account || a === account; });
  var weight = {};
  accounts.forEach(function (a, i) { weight[a] = [1, 0.6, 0.35, 0.2, 0.1, 0.05][i] || 0.05; });
  var out = [];
  var today = new Date();
  for (var i = days - 1; i >= 0; i--) {
    var d = new Date(today.getFullYear(), today.getMonth(), today.getDate() - i);
    var day = d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
    var api = {};
    accounts.forEach(function (a) {
      if (a.indexOf('cursor') === 0 || a.indexOf('antigravity') === 0) return;
      var v = Math.round((40 + 260 * hashOf(day + a)) * weight[a] * 100) / 100;
      if (v > 0) api[a] = v;
    });
    var dim = new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();
    out.push({ day: day, api: api, paid: accounts[0] && !account ? 200 / dim : account === accounts[0] ? 200 / dim : 0, tokens: 0 });
  }
  var total = 0;
  var byAccount = {};
  out.forEach(function (d) {
    Object.keys(d.api).forEach(function (a) {
      total += d.api[a];
      byAccount[a] = (byAccount[a] || 0) + d.api[a];
    });
    d.tokens = Object.values(d.api).reduce(function (s, v) { var t = tokensOf(v); return s + t.input + t.cacheRead + t.cacheWrite + t.output; }, 0);
  });
  var paid = out.reduce(function (s, d) { return s + d.paid; }, 0);
  var lanes = (board.lanes || []).filter(function (l) { return !l.device; });
  var folders = lanes.slice(0, 12).map(function (l, i) { return spentOf(l.path, Math.round(total * (0.3 / (i + 1)) * 100) / 100); });
  var sessions = [];
  lanes.forEach(function (l) {
    l.sessions.forEach(function (s) {
      if (sessions.length < 40 && (!account || s.account === account)) sessions.push(spentOf(s.key, Math.round(total * 0.12 * hashOf(s.key) * 100) / 100, { account: s.account, folder: l.path }));
    });
  });
  sessions.sort(function (a, b) { return b.api - a.api; });
  var models = [spentOf('claude-opus-5-5', total * 0.62), spentOf('claude-fable-5-1', total * 0.2), spentOf('gpt-6-astra', total * 0.17), spentOf('claude-haiku-5-5', total * 0.01), spentOf('mimo-v2.6-flash-free', 0, { unpriced: 33690 })];
  return {
    days: out,
    total: spentOf('', total),
    paid: paid,
    accounts: Object.keys(byAccount).map(function (a) { return spentOf(a, byAccount[a]); }).sort(function (a, b) { return b.api - a.api; }),
    models: models,
    folders: folders,
    sessions: sessions,
    paying: accounts.map(function (a, i) { return { account: a, monthly: i === 0 ? 200 : null, perToken: false, plan: i === 0 ? paid : 0, extra: 0, extraNow: i === 0 ? [0, 100] : undefined, tokens: 0 }; }),
    unpriced: ['mimo-v2.6-flash-free'],
    untracked: accounts.filter(function (a) { return a.indexOf('cursor') === 0 || a.indexOf('antigravity') === 0; }),
    asOf: Date.now() - 90e3,
    filling: false,
  };
}
// The board and the bar's feed as the engine hands them out before its first scan finishes
// (PREVIEW_LOADING in preview.sh): loading, with nothing read yet but the settings. Other
// devices (PREVIEW_DEVICES) still show, as their feeds kept from last time do.
function loadingBoard(board) {
  return Object.assign(board, { loading: true, generatedAt: 0, projects: [], lanes: [], notices: [], githubAt: null, usage: [], pinned: [], pinnedWorktrees: [], pinnedPrs: [], prs: [], githubLogin: null, todos: [], labels: [], todoHints: {} });
}
function loadingFeed(feed) {
  return Object.assign(feed, { loading: true, generatedAt: 0, projects: [], items: [], recent: [], snoozed: [], reviews: [], todos: [] });
}
window.__TAURI_INTERNALS__ = {
  invoke: function (cmd, args, options) {
    window.__calls.push([cmd, args, options]);
    if (cmd === 'get_board' && window.__PREVIEW_LOADING__) return fetch('./board.json').then(function (r) { return r.json(); }).then(loadingBoard);
    if (cmd === 'bar_feed' && window.__PREVIEW_LOADING__) return fetch('./bar-feed.json').then(function (r) { return r.json(); }).then(loadingFeed);
    if (cmd === 'get_board') {
      return Promise.all([
        fetch('./board.json').then(function (r) { return r.json(); }),
        optionalJson('./todos-fixture.json'),
        optionalJson('./usage-fixture.json'),
        optionalJson('./route-fixture.json'),
        optionalJson('./insights-fixture.json'),
        optionalJson('./notices-fixture.json'),
        optionalJson('./training-fixture.json'),
      ]).then(function (all) {
        var board = all[1] ? withFixture(all[0], all[1]) : all[0];
        // Background failures from notices-fixture.json (PREVIEW_NOTICES), `untilIn` minutes
        // from now where one says when it ends.
        if (all[5]) {
          board.notices = all[5].notices.map(function (n) {
            return Object.assign({}, n, { until: n.untilIn == null ? null : Date.now() + n.untilIn * 60e3 });
          });
        }
        // Which accounts' providers may train on them, from training-fixture.json
        // (PREVIEW_TRAINING), each read `readAgo` minutes ago.
        if (all[6]) {
          board.training = all[6].training.map(function (t) {
            return Object.assign({ canStop: false, url: null }, t, { asOf: t.readAgo == null ? null : Date.now() - t.readAgo * 60e3 });
          });
        }
        if (all[4]) board = withInsights(board, all[4]);
        if (window.__PREVIEW_EMBLEMS__) board = withEmblems(board);
        // Once a check keeps side conversations here (`window.__sides`, `window.__past`), the
        // board lists those, as the engine lists what it keeps (side.rs `glances`).
        if (window.__sides || window.__past) board.sides = sideGlances();
        if (all[3]) board.jev = Object.assign({ model: 'jev-latest', quick_panel: false, sessions: false, todos: false, pull_requests: false }, board.jev, { quick_panel: true });
        return all[2] ? withUsage(board, all[2]) : board;
      });
    }
    if (cmd === 'token_report') {
      return Promise.all([fetch('./board.json').then(function (r) { return r.json(); }), optionalJson('./tokens-fixture.json')]).then(function (all) {
        return all[1] || sampleReport(all[0], (args || {}).days || 30, (args || {}).account || null);
      });
    }
    if (cmd === 'session_tokens') {
      var key = (args || {}).key || '';
      var api = Math.round(hashOf(key) * 4000) / 100;
      return Promise.resolve({ total: spentOf(key, api), models: [spentOf('claude-opus-5-5', api * 0.9), spentOf('claude-haiku-5-5', api * 0.1)] });
    }
    // One session with its worktree, for the bar's pill menus (lib.rs `session_at`).
    if (cmd === 'session_at') {
      return window.__TAURI_INTERNALS__.invoke('get_board').then(function (board) {
        var found = null;
        board.lanes.forEach(function (l) {
          l.sessions.forEach(function (s) { if (!found && s.key === args.key && !s.visitor) found = { lane: l, session: s, apps: board.apps }; });
        });
        return found;
      });
    }
    // Training turned off at an account's provider (lib.rs `stop_training`): the reading after
    // is off, or what the fixture's `stopsTo` says, for a turn-off that didn't take.
    if (cmd === 'stop_training') {
      return optionalJson('./training-fixture.json').then(function (f) {
        var t = ((f && f.training) || []).filter(function (x) { return x.account === args.name; })[0];
        if (!t) return Promise.reject('There\'s no account named ' + args.name + '.');
        return Object.assign({ url: null }, t, { state: t.stopsTo || 'off', canStop: false, asOf: Date.now() });
      });
    }
    // With a devices fixture, Settings reads `[devices]` from it.
    if (cmd === 'get_config') {
      return Promise.all([fetch('./config.json').then(function (r) { return r.json(); }), optionalJson('./devices-fixture.json')]).then(function (all) {
        if (all[1] && all[1].cfg) all[0].config.devices = all[1].cfg;
        return all[0];
      });
    }
    // The window of lines between desktop cards (preview.sh lineage): lineage-desk.js's sample.
    if (cmd === 'lineage_feed') return Promise.resolve(window.__LINEAGE_FEED__ || { theme: 'system', lines: [] });
    // A card pinned to the desktop (preview.sh desktop): PREVIEW_PIN's session, worktree, todo or
    // PR (`<ref>:<key>`), cut from the board as desktop.rs cuts it. A ref alone takes the latest
    // worktree with sessions, the latest session, the first todo, or the first PR with a session.
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
        if (!key && ref === 'pr') key = ((board.prs || []).find(function (p) { return p.sessions.length > 0; }) || (board.prs || [])[0] || {}).key;
        var feed = { pin: { ref: ref, key: key }, title: null, loading: false, generatedAt: board.generatedAt, theme: board.theme, terminal: board.apps.terminal, editor: board.apps.editor, editorId: board.apps.editorId, editors: board.apps.editors || [], lanes: [], prs: [], todos: [], labels: [] };
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
        } else if (ref === 'pr') {
          var p = (board.prs || []).find(function (x) { return x.key === key; });
          if (p) {
            var worked = function (l) { return function (s) { return p.sessions.some(function (x) { return x.key === s.key && x.lane === l.id; }); }; };
            feed.lanes = board.lanes.filter(function (l) { return p.lanes.indexOf(l.id) >= 0 || l.sessions.some(worked(l)); }).map(function (l) { return cut(l, worked(l)); });
            feed.prs = [p];
            feed.title = '#' + p.number + ' ' + p.title;
          }
        }
        return feed;
      });
    }
    if (cmd === 'bar_feed' && window.__PREVIEW_EMBLEMS__) {
      return fetch('./bar-feed.json').then(function (r) { return r.json(); }).then(function (feed) {
        (feed.items || []).forEach(function (i) { i.emblem = emblemOf(i.key); });
        return feed;
      });
    }
    var file = { get_accounts: './accounts.json', found_accounts: './found.json', get_config: './config.json', pr_detail: './pr-detail.json', start_options: './start-options.json', device_start_options: './start-options.json', bar_feed: './bar-feed.json', editor_icons: './editor-icons.json', shell_commands: './shell-commands.json' }[cmd];
    if (file) return fetch(file).then(function (r) { return r.ok ? r.json() : Promise.reject(new Error(file + ' is missing')); });
    // The `/` lists' skills: from skills-fixture.json (PREVIEW_SKILLS) where there is one, else
    // what the engine listed for every worktree on the board (skills.json), a folder inside a
    // worktree getting its worktree's.
    if (cmd === 'skills') {
      var asked = args || {};
      var none = { folders: [], accounts: [] };
      var read = function (file) { return fetch(file).then(function (r) { return r.ok ? r.json() : null; }).catch(function () { return null; }); };
      return Promise.all([read('./skills-fixture.json'), read('./skills.json')]).then(function (got) {
        var folders = asked.folders || [];
        var accounts = asked.accounts || [];
        if (got[0]) {
          return {
            folders: folders.map(function (f, i) { return { folder: f, skills: (i === 0 ? got[0].first || [] : []).concat(got[0].every || []) }; }),
            accounts: accounts.map(function (a, i) { return { account: a, skills: (got[0].account || []).concat(i === 0 ? got[0].firstAccount || [] : []) }; }),
          };
        }
        var all = got[1] || none;
        var within = function (f) {
          return all.folders.filter(function (x) { return f === x.folder || f.indexOf(x.folder + '/') === 0; }).sort(function (a, b) { return b.folder.length - a.folder.length; })[0];
        };
        return {
          folders: folders.map(function (f) { return { folder: f, skills: (within(f) || { skills: [] }).skills }; }),
          accounts: accounts.map(function (a) { return all.accounts.filter(function (x) { return x.account === a; })[0] || { account: a, skills: [] }; }),
        };
      });
    }
    if (cmd === 'plugin:menu|new') return Promise.resolve([window.__calls.length, 'menu-' + window.__calls.length]);
    // A listener's id is where its call sits in __calls, counted from 1, so an unlisten can
    // find it again.
    if (cmd === 'plugin:event|listen') return Promise.resolve(window.__calls.length);
    // Settings › Agents: Claude Code accounts with the plugin installed, installed from the repo's
    // marketplace beside an old skill copy, behind, and not installed beside an edited copy; a
    // Codex account without the skill, and one with no skills folder known.
    if (cmd === 'agents_status' || cmd === 'install_agents') {
      var plugin = function (version, from) { return { version: version, disabled: false, from: from || null }; };
      return Promise.resolve({
        command: { link: '/usr/local/bin/codealong', target: '/Applications/Codealong.app/Contents/MacOS/codealong', onPath: true },
        skillFound: true,
        pluginVersion: '0.2.0+1a2b3c4d',
        accounts: [
          { name: 'claude', harness: 'claude', path: '/Users/you/.claude/skills/codealong', installed: false, current: false, edited: false, plugin: plugin('0.2.0+1a2b3c4d') },
          { name: 'claude2', harness: 'claude', path: '/Users/you/.claude2/skills/codealong', installed: true, current: true, edited: false, plugin: plugin('0.2.0', 'codealong/codealong') },
          { name: 'claude3', harness: 'claude', path: '/Users/you/.claude3/skills/codealong', installed: false, current: false, edited: false, plugin: plugin('0.2.0+0f9e8d7c') },
          { name: 'claude-work', harness: 'claude', path: '/Users/you/.claude-work/skills/codealong', installed: true, current: false, edited: true, plugin: plugin(null) },
          { name: 'codex', harness: 'codex', path: '/Users/you/.codex/skills/codealong', installed: false, current: false, edited: false, plugin: null },
          { name: 'antigravity', harness: 'antigravity', path: null, installed: false, current: false, edited: false, plugin: null },
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
    // A PR the board lists, with the worktrees that hold it or its sessions, for its card in the
    // notch (lib.rs `pr_at`). A review sent is taken as done.
    if (cmd === 'pr_at') {
      return window.__TAURI_INTERNALS__.invoke('get_board').then(function (board) {
        var pr = null;
        (board.prs || []).forEach(function (p) { if (!pr && p.key === args.key) pr = p; });
        if (!pr) return null;
        var lanes = board.lanes.filter(function (l) {
          return pr.lanes.indexOf(l.id) >= 0 || pr.sessions.some(function (s) { return s.lane === l.id; });
        });
        return { pr: pr, lanes: lanes, apps: board.apps };
      });
    }
    // A review whose text is "fail" is refused, as GitHub might, for the checks. No PR has a draft
    // review of yours (pr_draft) unless a check says so, so none is submitted with one.
    if (cmd === 'review_pr') return args.body === 'fail' ? Promise.reject(new Error('GitHub said no')) : Promise.resolve({ draft: null });
    if (cmd === 'pr_draft') return Promise.resolve({ draft: null, submitted: null });
    // A PR's merge box (prmerge.rs `MergeBox`) for a PR the board lists: ready to squash and
    // merge, with GitHub's title and message for each way, and two emails. merge_pr answers as
    // lib.rs does, a merge after a moment, as GitHub takes one, so what shows meanwhile can be
    // seen; one whose title is "fail" is refused, as GitHub refuses one whose branch has moved.
    if (cmd === 'merge_box') {
      return window.__TAURI_INTERNALS__.invoke('get_board').then(function (board) {
        var want = args || {};
        var p = (board.prs || []).find(function (x) { return x.repoSlug === want.repo && x.number === want.number; }) || { title: 'A PR', head: 'feature', author: 'you' };
        var merged = p.state === 'merged';
        return {
          id: 'PR_' + want.number, head: '0ef4ed2', branch: p.head, state: merged ? 'MERGED' : p.state === 'closed' ? 'CLOSED' : 'OPEN', draft: p.state === 'draft',
          mergeable: 'MERGEABLE', mergeState: 'CLEAN', reviewDecision: 'APPROVED', canMerge: true, methods: ['merge', 'squash', 'rebase'], defaultMethod: 'squash',
          mergeText: { title: 'Merge pull request #' + want.number + ' from ' + (p.author || 'you') + '/' + p.head, body: p.title },
          squashText: { title: p.title + ' (#' + want.number + ')', body: '* ' + p.title + '\n\n* Say what it does in the docs' },
          emails: ['you@example.com', 'you@work.example'], defaultEmail: 'you@example.com', canReady: true, canBypass: false, canAuto: false, canStopAuto: false, auto: null,
          canUpdateBranch: false, queue: false, queued: null, deletesBranch: false, branchRef: 'REF_' + want.number, mergedBy: merged ? 'you' : null,
        };
      });
    }
    if (cmd === 'merge_pr') {
      var op = (args || {}).op || {};
      return new Promise(function (resolve, reject) {
        setTimeout(function () {
          if (op.title === 'fail') reject(new Error('Couldn’t merge #' + args.number + ': Head branch was modified. Review and try the merge again.'));
          else resolve({ note: null, shownAt: null });
        }, op.op === 'merge' ? 400 : 0);
      });
    }
    // A PR the board doesn't list, as GitHub would have it: merged a while ago, no sessions here.
    if (cmd === 'pr_card') {
      var want = args || {};
      var daysAgo = function (days) { return Date.now() - days * 864e5; };
      return Promise.resolve({
        key: want.repo + '#' + want.number, repoSlug: want.repo, repo: want.repo.split('/')[1], project: '', number: want.number,
        title: 'An older PR, as GitHub has it', url: 'https://github.com/' + want.repo + '/pull/' + want.number, state: 'merged',
        author: 'someone', mine: false, reviewRequested: false, head: 'older-work', base: 'main',
        checks: { passed: 4, failed: 0, pending: 0, skipped: 0, failing: [] }, merge: 'unknown',
        additions: 120, deletions: 34, changedFiles: 6, createdAt: daysAgo(40), updatedAt: daysAgo(38), mergedAt: daysAgo(38), closedAt: daysAgo(38),
        lanes: [], sessions: [],
      });
    }
    if (cmd === 'link_device') return Promise.resolve('482 915');
    if (cmd === 'device_link_code') return Promise.resolve('cal1f4k3c0d3xq7mzv2r9k8w3n5b6t1y4u7p2e9s3d8g5h1j6l0a4c7v2b');
    // A stand-in for the QR code the engine draws (link_code_qr): the right size and colors, not a real code.
    if (cmd === 'link_code_qr') return Promise.resolve('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 33 33" width="176" height="176" shape-rendering="crispEdges"><rect width="33" height="33" fill="#ffffff"/><path fill="#000000" d="M4 4h1v1h-1zM4 5h1v1h-1zM4 6h1v1h-1zM4 7h1v1h-1zM4 8h1v1h-1zM4 9h1v1h-1zM4 10h1v1h-1zM4 14h1v1h-1zM4 15h1v1h-1zM4 22h1v1h-1zM4 23h1v1h-1zM4 24h1v1h-1zM4 25h1v1h-1zM4 26h1v1h-1zM4 27h1v1h-1zM4 28h1v1h-1zM5 4h1v1h-1zM5 10h1v1h-1zM5 15h1v1h-1zM5 19h1v1h-1zM5 22h1v1h-1zM5 28h1v1h-1zM6 4h1v1h-1zM6 6h1v1h-1zM6 7h1v1h-1zM6 8h1v1h-1zM6 10h1v1h-1zM6 13h1v1h-1zM6 14h1v1h-1zM6 15h1v1h-1zM6 18h1v1h-1zM6 22h1v1h-1zM6 24h1v1h-1zM6 25h1v1h-1zM6 26h1v1h-1zM6 28h1v1h-1zM7 4h1v1h-1zM7 6h1v1h-1zM7 7h1v1h-1zM7 8h1v1h-1zM7 10h1v1h-1zM7 12h1v1h-1zM7 13h1v1h-1zM7 14h1v1h-1zM7 17h1v1h-1zM7 20h1v1h-1zM7 22h1v1h-1zM7 24h1v1h-1zM7 25h1v1h-1zM7 26h1v1h-1zM7 28h1v1h-1zM8 4h1v1h-1zM8 6h1v1h-1zM8 7h1v1h-1zM8 8h1v1h-1zM8 10h1v1h-1zM8 12h1v1h-1zM8 13h1v1h-1zM8 14h1v1h-1zM8 15h1v1h-1zM8 16h1v1h-1zM8 18h1v1h-1zM8 22h1v1h-1zM8 24h1v1h-1zM8 25h1v1h-1zM8 26h1v1h-1zM8 28h1v1h-1zM9 4h1v1h-1zM9 10h1v1h-1zM9 13h1v1h-1zM9 14h1v1h-1zM9 15h1v1h-1zM9 18h1v1h-1zM9 22h1v1h-1zM9 28h1v1h-1zM10 4h1v1h-1zM10 5h1v1h-1zM10 6h1v1h-1zM10 7h1v1h-1zM10 8h1v1h-1zM10 9h1v1h-1zM10 10h1v1h-1zM10 12h1v1h-1zM10 14h1v1h-1zM10 16h1v1h-1zM10 19h1v1h-1zM10 22h1v1h-1zM10 23h1v1h-1zM10 24h1v1h-1zM10 25h1v1h-1zM10 26h1v1h-1zM10 27h1v1h-1zM10 28h1v1h-1zM11 14h1v1h-1zM11 16h1v1h-1zM11 19h1v1h-1zM12 4h1v1h-1zM12 5h1v1h-1zM12 10h1v1h-1zM12 12h1v1h-1zM12 13h1v1h-1zM12 14h1v1h-1zM12 15h1v1h-1zM12 16h1v1h-1zM12 19h1v1h-1zM12 23h1v1h-1zM12 24h1v1h-1zM12 25h1v1h-1zM12 27h1v1h-1zM13 4h1v1h-1zM13 5h1v1h-1zM13 6h1v1h-1zM13 7h1v1h-1zM13 8h1v1h-1zM13 12h1v1h-1zM13 13h1v1h-1zM13 14h1v1h-1zM13 16h1v1h-1zM13 17h1v1h-1zM13 19h1v1h-1zM13 22h1v1h-1zM13 23h1v1h-1zM13 25h1v1h-1zM13 26h1v1h-1zM13 27h1v1h-1zM14 5h1v1h-1zM14 8h1v1h-1zM14 10h1v1h-1zM14 12h1v1h-1zM14 13h1v1h-1zM14 14h1v1h-1zM14 15h1v1h-1zM14 16h1v1h-1zM14 18h1v1h-1zM14 22h1v1h-1zM14 25h1v1h-1zM14 28h1v1h-1zM15 4h1v1h-1zM15 5h1v1h-1zM15 6h1v1h-1zM15 13h1v1h-1zM15 14h1v1h-1zM15 17h1v1h-1zM15 19h1v1h-1zM15 21h1v1h-1zM15 22h1v1h-1zM15 23h1v1h-1zM15 25h1v1h-1zM15 26h1v1h-1zM15 27h1v1h-1zM16 7h1v1h-1zM16 9h1v1h-1zM16 12h1v1h-1zM16 14h1v1h-1zM16 23h1v1h-1zM16 24h1v1h-1zM16 25h1v1h-1zM16 27h1v1h-1zM16 28h1v1h-1zM17 4h1v1h-1zM17 5h1v1h-1zM17 6h1v1h-1zM17 8h1v1h-1zM17 12h1v1h-1zM17 16h1v1h-1zM17 18h1v1h-1zM17 22h1v1h-1zM17 23h1v1h-1zM17 27h1v1h-1zM17 28h1v1h-1zM18 4h1v1h-1zM18 5h1v1h-1zM18 6h1v1h-1zM18 7h1v1h-1zM18 9h1v1h-1zM18 12h1v1h-1zM18 17h1v1h-1zM18 18h1v1h-1zM18 21h1v1h-1zM18 24h1v1h-1zM18 25h1v1h-1zM18 26h1v1h-1zM18 28h1v1h-1zM19 6h1v1h-1zM19 7h1v1h-1zM19 9h1v1h-1zM19 10h1v1h-1zM19 11h1v1h-1zM19 14h1v1h-1zM19 15h1v1h-1zM19 17h1v1h-1zM19 18h1v1h-1zM19 19h1v1h-1zM19 20h1v1h-1zM19 22h1v1h-1zM19 28h1v1h-1zM20 4h1v1h-1zM20 6h1v1h-1zM20 7h1v1h-1zM20 8h1v1h-1zM20 12h1v1h-1zM20 14h1v1h-1zM20 15h1v1h-1zM20 18h1v1h-1zM20 22h1v1h-1zM20 23h1v1h-1zM20 25h1v1h-1zM21 12h1v1h-1zM21 14h1v1h-1zM21 15h1v1h-1zM21 16h1v1h-1zM21 21h1v1h-1zM21 22h1v1h-1zM21 23h1v1h-1zM21 24h1v1h-1zM21 25h1v1h-1zM21 27h1v1h-1zM22 4h1v1h-1zM22 5h1v1h-1zM22 6h1v1h-1zM22 7h1v1h-1zM22 8h1v1h-1zM22 9h1v1h-1zM22 10h1v1h-1zM22 13h1v1h-1zM22 14h1v1h-1zM22 16h1v1h-1zM22 17h1v1h-1zM22 18h1v1h-1zM22 25h1v1h-1zM22 26h1v1h-1zM22 27h1v1h-1zM22 28h1v1h-1zM23 4h1v1h-1zM23 10h1v1h-1zM23 14h1v1h-1zM23 16h1v1h-1zM23 17h1v1h-1zM23 18h1v1h-1zM23 19h1v1h-1zM23 20h1v1h-1zM23 21h1v1h-1zM23 25h1v1h-1zM23 27h1v1h-1zM24 4h1v1h-1zM24 6h1v1h-1zM24 7h1v1h-1zM24 8h1v1h-1zM24 10h1v1h-1zM24 16h1v1h-1zM24 17h1v1h-1zM24 18h1v1h-1zM24 20h1v1h-1zM24 22h1v1h-1zM24 24h1v1h-1zM24 25h1v1h-1zM24 28h1v1h-1zM25 4h1v1h-1zM25 6h1v1h-1zM25 7h1v1h-1zM25 8h1v1h-1zM25 10h1v1h-1zM25 15h1v1h-1zM25 16h1v1h-1zM25 19h1v1h-1zM25 20h1v1h-1zM25 23h1v1h-1zM25 24h1v1h-1zM25 27h1v1h-1zM25 28h1v1h-1zM26 4h1v1h-1zM26 6h1v1h-1zM26 7h1v1h-1zM26 8h1v1h-1zM26 10h1v1h-1zM26 12h1v1h-1zM26 15h1v1h-1zM26 18h1v1h-1zM26 19h1v1h-1zM26 20h1v1h-1zM26 23h1v1h-1zM26 26h1v1h-1zM26 27h1v1h-1zM26 28h1v1h-1zM27 4h1v1h-1zM27 10h1v1h-1zM27 12h1v1h-1zM27 13h1v1h-1zM27 17h1v1h-1zM27 20h1v1h-1zM27 21h1v1h-1zM27 24h1v1h-1zM27 25h1v1h-1zM27 26h1v1h-1zM27 27h1v1h-1zM27 28h1v1h-1zM28 4h1v1h-1zM28 5h1v1h-1zM28 6h1v1h-1zM28 7h1v1h-1zM28 8h1v1h-1zM28 9h1v1h-1zM28 10h1v1h-1zM28 12h1v1h-1zM28 13h1v1h-1zM28 14h1v1h-1zM28 20h1v1h-1zM28 21h1v1h-1zM28 22h1v1h-1zM28 23h1v1h-1zM28 24h1v1h-1zM28 26h1v1h-1zM28 28h1v1h-1z"/></svg>');
    if (cmd === 'answer_link' || cmd === 'unlink_device' || cmd === 'set_device_color') return Promise.resolve(null);
    // Where the bar shows: what preview.sh's notch view sets, else floating.
    if (cmd === 'bar_mode') return Promise.resolve(window.__BAR_MODE__ || { mode: 'floating', notch: null });
    if (cmd === 'link_with_code') return Promise.resolve('Studio');
    if (cmd === 'search_devices') return Promise.resolve([]);
    // A zoom step answers where the engine would get to from actual size; nothing is zoomed.
    if (cmd === 'zoom') return Promise.resolve({ in: 110, out: 90, actual: 100 }[(args || {}).step] || 100);
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
    // Updates: a release build that checked two hours ago, or `window.__update`. A check script
    // shows another state by setting that and pushing it, as the engine would:
    // `window.__emit('update-status', window.__update = {current, state, …})`.
    if (cmd === 'update_status') return Promise.resolve(window.__update || { current: '0.1.0', state: 'upToDate', checkedAt: Date.now() - 2 * 3600e3 });
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
        // The reply links the PR the session worked on, if the board has one, and an older PR in
        // its repo the board doesn't list (pr_card answers for that one).
        var pr = (all[2].prs || []).find(function (p) { return p.sessions.some(function (x) { return x.key === key; }); });
        var older = 1;
        while (pr && all[2].prs.some(function (p) { return p.repoSlug === pr.repoSlug && p.number === older; })) older++;
        var links = pr ? '\n\nOpened ' + pr.url + ', which follows [the earlier fix](' + pr.url.replace(/\/pull\/\d+$/, '/pull/' + older) + ').' : '';
        return { before: null, after: 4, items: [
          item('you', s.title, 0),
          item('tool', 'Bash · `npm test`', 1, { id: 't1', input: 'npm test' }),
          item('result', '', 2, { id: 't1', output: 'Tests  12 passed (12)' }),
          item('agent', (s.excerpt || 'Done.') + links, 3),
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
    // How a message would reach a session, as the engine would guess it from the session alone
    // (send.rs `way_in` also looks at its process and registry): board.json's, or the one a check
    // gave the window in its place (made running, say), which it puts in
    // `window.__madeSessions[key]`, so the box sends the way the session it shows would.
    if (cmd === 'session_way') {
      return fetch('./board.json').then(function (r) { return r.json(); }).then(function (board) {
        var key = (args || {}).key;
        var s = (window.__madeSessions || {})[key] || null;
        if (!s) board.lanes.forEach(function (l) { l.sessions.forEach(function (x) { if (x.key === key) s = x; }); });
        if (!s || s.hosted) return null;
        var terminal = board.apps.terminal;
        if (s.harness === 'antigravity') return { kind: 'antigravity' };
        if (s.harness === 'cursor') return s.surface === 'app' ? { kind: 'cursor' } : s.live ? { kind: 'typed', app: terminal } : { kind: 'resume', app: terminal };
        if (s.harness === 'opencode') return { kind: 'opencode' };
        if (s.harness === 'codex') return s.surface === 'app' ? { kind: 'codex-app' } : s.live ? { kind: 'typed', app: terminal } : { kind: 'resume', app: terminal };
        if (!s.live) return s.surface === 'app' ? { kind: 'claude-app' } : { kind: 'resume', app: terminal };
        if (s.attachCommand) return { kind: 'background' };
        return s.surface === 'app' ? { kind: 'message' } : { kind: 'typed', app: terminal };
      });
    }
    if (cmd === 'session_answer') return Promise.resolve((args || {}).answer === 'allow' ? 'Allowed it (in the preview nothing is pressed anywhere).' : 'Denied it (in the preview nothing is pressed anywhere).');
    if (cmd === 'session_newer') return Promise.resolve({ items: [], before: null, after: (args || {}).after || 0 });
    // A session Codealong runs: the reply it's writing, which a check can set.
    if (cmd === 'session_stream') return Promise.resolve(window.__stream || null);
    if (cmd === 'session_cancel') return Promise.resolve('Stopped it (in the preview nothing is pressed anywhere).');
    // Side conversations: a question is kept and its answer never comes by itself; a check
    // writes it with `window.__emit('side', {id, key, turns, running, ended})`, or sets
    // `window.__sides[key]` for a window that opens on one. Closing one keeps its turns in
    // `window.__past`, where a check can put the ones from before.
    // A PR's questions (`pr_side_ask`, `pr_side_close`) are kept with them, by the PR's key, with
    // the account that answers.
    if (cmd === 'side_ask' || cmd === 'pr_side_ask') {
      window.__sides = window.__sides || {};
      window.__sideIds = (window.__sideIds || 0) + 1;
      var side = window.__sides[args.key] || { id: 'side-' + window.__sideIds, key: args.key, turns: [], running: false, ended: false, account: args.account, startedAt: Date.now() };
      side = { id: side.id, key: side.key, turns: side.turns.concat([{ question: args.text, answer: '', done: false, at: Date.now() }]), running: true, ended: false, account: side.account, startedAt: side.startedAt };
      window.__sides[args.key] = side;
      // The engine tells every window of the question, the one that asked included, and makes
      // the board again, which lists it.
      setTimeout(function () {
        window.__emit('side', side);
        window.__pushSides();
      }, 0);
      return Promise.resolve(side);
    }
    if (cmd === 'side_get') return Promise.resolve((window.__sides || {})[(args || {}).key] || null);
    if (cmd === 'side_close' || cmd === 'pr_side_close') {
      var open = (window.__sides || {})[(args || {}).key];
      if (open) {
        window.__past = (window.__past || []).filter(function (k) { return k.id !== open.id; });
        window.__past.push({ id: open.id, key: open.key, title: '', startedAt: Date.now(), lastAt: Date.now(), turns: open.turns.map(function (t) { return t.done ? t : Object.assign({}, t, { done: true, error: 'Closed before it answered.' }); }) });
        delete window.__sides[open.key];
        // The engine tells every window it's closed, once it's kept.
        setTimeout(function () {
          window.__emit('side', { id: open.id, key: open.key, turns: [], running: false, ended: false });
          window.__pushSides();
        }, 0);
      }
      return Promise.resolve(null);
    }
    if (cmd === 'side_past') {
      return Promise.resolve((window.__past || []).filter(function (k) { return k.key === (args || {}).key; }).sort(function (a, b) { return b.lastAt - a.lastAt; }));
    }
    if (cmd === 'side_forget') {
      window.__past = (window.__past || []).filter(function (k) { return k.id !== (args || {}).id; });
      setTimeout(window.__pushSides, 0);
      return Promise.resolve(null);
    }
    // A side conversation popped out into a window of its own: no window opens in the preview,
    // but every window hears it's out, as the engine tells them (`side-windows`). A check can set
    // `window.__sideWindows` for a window that opens with one out.
    if (cmd === 'side_windows') return Promise.resolve((window.__sideWindows || []).slice());
    if (cmd === 'side_window') {
      var outs = window.__sideWindows || [];
      if (outs.indexOf(args.key) < 0) {
        window.__sideWindows = outs.concat([args.key]);
        setTimeout(function () { window.__emit('side-windows', window.__sideWindows.slice()); }, 0);
      }
      return Promise.resolve(null);
    }
    if (cmd === 'send_to_session') return Promise.resolve({ message: 'Sent (in the preview nothing is typed anywhere).', opened: false });
    if (cmd === 'dismiss_suggestions') return Promise.resolve(null);
    if (cmd === 'resume_session') return Promise.resolve({ message: 'Resumed (in the preview nothing starts anywhere).', opened: false });
    if (cmd === 'switch_session_model') return Promise.resolve('Switched its model (in the preview nothing is typed anywhere).');
    if (cmd === 'switch_session_effort') return Promise.resolve('Switched its effort (in the preview nothing is typed anywhere).');
    if (cmd === 'switch_session_mode') return Promise.resolve({ message: 'Switched its mode (in the preview no agent is asked).', mode: args.mode });
    // A todo change "shows" on a board that never comes, so the page keeps its own copy of it.
    // Close idle sessions: the board's live sessions quiet that long, without the engine's checks
    // for Ghostty, Jev or questions. A terminal's tab closes, a job stops, and an app's session
    // or one Codealong runs is listed only to be archived; nothing is closed. The first key
    // asked to close stays open, as the tab you're in would.
    if (cmd === 'idle_sessions' || cmd === 'close_idle_sessions') {
      return fetch('./board.json').then(function (r) { return r.json(); }).then(function (board) {
        var after = (args || {}).afterMs || 864e5;
        var archive = !!(args || {}).archive;
        var seen = {};
        var rows = [];
        board.lanes.forEach(function (l) {
          l.sessions.forEach(function (s) {
            if (s.visitor || seen[s.key] || !s.live) return;
            seen[s.key] = true;
            var ends = s.attachCommand ? 'job' : s.surface === 'cli' && s.pid && !s.hosted ? 'terminal' : 'archive';
            var place = s.hosted ? 'in Codealong' : s.surface === 'app' ? 'in the ' + (s.harness === 'codex' ? 'Codex' : 'Claude') + ' app' : 'in a terminal Codealong can\u2019t close';
            rows.push({ s: s, l: l, ends: ends, place: place });
          });
        });
        var now = Date.now();
        var quiet = function (r) { return (r.s.status === 'idle' || r.s.status === 'unread') && now - r.s.lastActivity >= after; };
        var listed = function (r) { return quiet(r) && (archive || r.ends !== 'archive'); };
        if (cmd === 'close_idle_sessions') {
          return (args.keys || []).map(function (k, i) {
            var r = rows.find(function (x) { return x.s.key === k; });
            var ends = r ? r.ends : 'terminal';
            var did = ends === 'job' ? 'Stopped' : 'Closed';
            if (i === 0 && ends !== 'archive') return { key: k, title: r ? r.s.title : k, ends: ends, closed: false, archived: false, ok: false, message: 'It\u2019s the tab you\u2019re in.' };
            return { key: k, title: r ? r.s.title : k, ends: ends, closed: ends !== 'archive', archived: archive, ok: true, message: ends === 'archive' ? 'Archived; it stays open where it is.' : did + (archive ? ' and archived.' : '.') };
          });
        }
        return {
          afterMs: after,
          archive: archive,
          close: rows.filter(listed).map(function (r) {
            var days = Math.floor((now - r.s.lastActivity) / 864e5);
            var why = (days >= 2 ? 'idle ' + days + ' days' : 'idle ' + Math.floor((now - r.s.lastActivity) / 36e5) + ' hours') + ' \u00b7 last reply asks nothing';
            return { key: r.s.key, title: r.s.title, account: r.s.account, harness: r.s.harness, ends: r.ends, pid: r.ends === 'terminal' ? r.s.pid : null, cwd: r.s.cwd, worktree: r.l.id, callsign: r.l.callsign, lastActivity: r.s.lastActivity, why: r.ends === 'archive' ? why + ' \u00b7 stays open ' + r.place : why };
          }),
          kept: rows.filter(function (r) { return !listed(r); }).map(function (r) {
            var why = quiet(r) ? r.place : r.s.status === 'working' ? 'working' : r.s.status === 'needs' ? 'needs you' : 'active 3 hours ago';
            return { key: r.s.key, title: r.s.title, account: r.s.account, why: why };
          }),
        };
      });
    }
    if (cmd === 'todo_op') return Promise.resolve(Number.MAX_SAFE_INTEGER);
    // Mark as done does what it would to a session in a terminal of its own; the board never shows
    // it archived, so it comes back once its change has waited long enough (ahead.ts).
    if (cmd === 'session_done') return Promise.resolve({ done: true, message: 'Done with it: its terminal is closed and it’s archived.', at: null });
    // The waiting window (preview.sh waiting) opens on PREVIEW_TRANSCRIPT's session when there is
    // one, else on the first that waits on you; asking for it again only brings it up.
    if (cmd === 'waiting_asked') return Promise.resolve(window.__WAITING_ASKED__ || null);
    if (cmd === 'show_waiting') return Promise.resolve(null);
    // A PR's nudge is turned on or off without an answer; the board that follows says so.
    if (cmd === 'set_pr_nudge') return Promise.resolve(null);
    // The accounts' order is written without an answer; the usage bar shows it until a board
    // lists them so, which here none does.
    if (cmd === 'order_accounts') return Promise.resolve(null);
    // Nothing is pinned to the desktop until a check pushes `desktop-pins`.
    if (cmd === 'desktop_pins') return Promise.resolve([]);
    // A PNG is kept as an image to show; anything else as a file, by the name it came with.
    if (cmd === 'save_attachment' || cmd === 'import_attachment') {
      var png = args instanceof Uint8Array && args[0] === 0x89;
      var name = cmd === 'import_attachment' ? String(args.path).split('/').pop() : decodeURIComponent(((options || {}).headers || {})['x-name'] || 'file');
      return Promise.resolve(png ? { kind: 'image', path: 'images/0123456789abcdef.png', name: name } : { kind: 'file', path: 'files/fedcba9876543210/' + name.replace(/[^\w.-]+/g, '-'), name: name });
    }
    // The shortcuts that work in every app, as Settings and the tips (Tip.tsx) ask for them: the
    // defaults, or what a check put in `window.__SHORTCUTS__` ({ quick, chat, waiting, improve,
    // main }, '' for off).
    if (cmd === 'quick_shortcut') {
      var which = (args && args.which) || 'quick';
      var set = window.__SHORTCUTS__ || {};
      var defaults = { quick: 'Option+Cmd+/', chat: "Option+Cmd+'", waiting: 'Option+Cmd+.', improve: 'Option+Cmd+,', main: 'Option+Cmd+;' };
      return Promise.resolve({ which: which, shortcut: which in set ? set[which] : defaults[which], error: null });
    }
    if (window.__unknown.indexOf(cmd) < 0) window.__unknown.push(cmd);
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
// The side conversations a check keeps here (`window.__sides`, open, and `window.__past`, kept),
// as the board lists them (side.rs `glances`): by session, the latest started first, an open one
// in place of what's kept of it.
function sideGlances() {
  var by = {};
  var open = window.__sides || {};
  var first = function (turns) { return ((turns[0] || {}).question || '').split('\n').map(function (l) { return l.trim(); }).filter(Boolean)[0] || ''; };
  var add = function (key, g) { (by[key] = by[key] || []).push(g); };
  (window.__past || []).forEach(function (k) {
    if (open[k.key] && open[k.key].id === k.id) return;
    add(k.key, { id: k.id, question: first(k.turns), asked: k.turns.length, running: false, open: false, startedAt: k.startedAt, lastAt: k.lastAt });
  });
  Object.keys(open).forEach(function (key) {
    var s = open[key];
    if (!s.turns.length) return;
    var at = s.turns.reduce(function (m, t) { return Math.max(m, t.at || 0); }, 0);
    add(key, { id: s.id, question: first(s.turns), asked: s.turns.length, running: !!s.running, open: true, startedAt: s.startedAt || at, lastAt: at });
  });
  Object.keys(by).forEach(function (key) { by[key].sort(function (a, b) { return b.startedAt - a.startedAt; }); });
  return by;
}
// The board again, as the engine makes it when what it lists of the side conversations changes:
// a check calls it after telling of an answer (`window.__emit('side', …)`) that ends one.
window.__pushSides = function () {
  return window.__TAURI_INTERNALS__.invoke('get_board').then(function (b) { window.__emit('board', b); });
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
