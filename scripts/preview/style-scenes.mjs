// The scenes style-diff.mjs records: every view, window, dialog and pane the preview can
// reach. A scene is a view of scripts/preview.sh with its sample files, and steps run in the
// page to get somewhere a click or a key goes. Each is recorded in light and in dark.
//
//   view, project, size, loading, notch, pin   as preview.sh's arguments and PREVIEW_* take them
//   todos, usage, notices, devices, insights, route, canvas, found
//                                               a sample file, as PREVIEW_TODOS and the rest
//   transcript                                  a conversation in the data folder (transcript-1,
//                                               transcript-2), as PREVIEW_TRANSCRIPT
//   init                                        a script the page loads, as PREVIEW_INIT
//   setup                                       JavaScript run before the page's own
//   steps                                       what to do once the page has settled
//   wait                                        a selector that must match before it's recorded
//
// A step is JavaScript run in the page (async; `sd` has waitFor, click, rightClick, key, type,
// byText, menu, emit and sleep), or {hover: selector} / {press: selector} to put the real
// pointer on an element (so :hover and :active apply), {click: selector} / {dblclick: selector}
// to click with it (the canvas takes the pointer for itself), {drag: selector, by: [dx, dy]} to press
// there and move, still held when the page is recorded, or {tab: n} to press Tab n times
// (:focus-visible). The page settles after each before the next runs.
// Nothing here may name a project, repo or person: scenes pick from the board they're given.
const S = 'scripts/preview/';
const all = { todos: S + 'todos-sample.json', usage: S + 'usage-sample.json', notices: S + 'notices-sample.json', devices: S + 'devices-sample.json', insights: S + 'insights-sample.json' };

// The stub, answering for any session with session-sample.json's conversation (and with its
// call to allow, for a session that waits on you), and for any PR with pr-detail-sample.json.
const sampled = `(() => {
  const real = window.__TAURI_INTERNALS__.invoke;
  const json = (file) => fetch(file).then((r) => r.json());
  const aged = (list) => list.map((x) => ({ ...x, at: Date.now() - x.ago * 60e3 }));
  window.__TAURI_INTERNALS__.invoke = (cmd, args, options) => {
    const mine = cmd === 'session_page' || cmd === 'session_pending' || cmd === 'pr_detail';
    if (!mine) return real(cmd, args, options);
    window.__calls.push([cmd, args, options]);
    if (cmd === 'pr_detail') return json('./pr-detail-sample.json').then((d) => ({ ...d, reviews: aged(d.reviews), comments: aged(d.comments) }));
    return json('./session-sample.json').then((s) => {
      if (cmd === 'session_pending') return window.__PENDING__ === 'questions' ? { id: 'ask-1', label: 'Asks you · Which route first?', input: null, cannot: null, questions: s.page.items.find((it) => it.questions).questions } : window.__PENDING__ ? s.pending : null;
      return args.before == null ? { ...s.page, items: aged(s.page.items) } : { items: [], before: null, after: s.page.after };
    });
  };
})();`;
// The board with one session waiting on you, so its panel asks to allow a call (`allow`) or
// answer questions (`questions`), whatever the data folder's sessions are doing: as the checks
// for the approval box do (questions-check.js, once-check.js), every session that needs you or
// failed is made idle, and the latest in the project the scene shows (a Mac's own, not one on a
// linked device) waits on a call. Otherwise the panel or the peek could open on one that waits on
// your reply, which shows the reply box, or on none. Only that session has a call to answer, and
// the peek opens on it.
const needy = (what) => `window.__PENDING__ = ${JSON.stringify(what)}; (() => {
  const real = window.__TAURI_INTERNALS__.invoke;
  let picked = null;
  window.__TAURI_INTERNALS__.invoke = (cmd, args, options) => {
    if (cmd === 'session_pending' && picked && (args || {}).key !== picked) return Promise.resolve(null);
    const p = real(cmd, args, options);
    if (cmd !== 'get_board') return p;
    return p.then((board) => {
      let project = null;
      try { project = localStorage.getItem('codealong.project'); } catch {}
      const ours = (l) => !l.hidden && !l.device;
      const candidates = (inProject) => board.lanes.filter((l) => ours(l) && (!inProject || !project || project === 'All projects' || l.project === project)).flatMap((l) => l.sessions.filter((x) => !x.visitor && !x.archived).map((s) => [l, s])).sort((a, b) => b[1].lastActivity - a[1].lastActivity || a[1].key.localeCompare(b[1].key));
      for (const l of board.lanes) for (const s of l.sessions) {
        if (s.status === 'needs' || s.status === 'error') s.status = 'idle';
        s.snoozedUntil = undefined;
      }
      // The latest in the project shown; on a Mac whose project has none, the latest anywhere, moved into it.
      const [lane, s] = candidates(true)[0] || candidates(false)[0] || [];
      if (!s) return board;
      if (project && project !== 'All projects' && lane.project !== project) lane.project = project;
      picked = s.key;
      Object.assign(s, { status: 'needs', statusSince: Date.now() - 60e3, asks: false, hosted: false, snoozedUntil: undefined, excerpt: ${JSON.stringify(what === 'questions' ? 'Asks you · Which route first?' : 'Bash · `git push origin HEAD:main`')} });
      try { localStorage.setItem('codealong.preview.peek', s.key); } catch {}
      return board;
    });
  };
})();`;
// The stub's answer to one command, changed: \`body\` gets the answer as \`r\` (and \`args\`).
const patch = (cmd, body) => `(() => {
  const real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = (cmd, args, options) => {
    const p = real(cmd, args, options);
    return cmd === ${JSON.stringify(cmd)} ? p.then((r) => { ${body}; return r; }) : p;
  };
})();`;
// A small canvas with one of everything on it, near enough that its cards are drawn in full:
// worktrees with their sessions, todos, PRs, something that has left, notes and frames in each color.
const canvasDoc = `(() => {
  const real = window.__TAURI_INTERNALS__.invoke;
  window.__TAURI_INTERNALS__.invoke = (cmd, args, options) => {
    if (cmd !== 'canvas_load') return real(cmd, args, options);
    return real('get_board').then((board) => {
      const mine = (x) => args.project === 'All projects' || x.project === args.project;
      const lanes = board.lanes.filter((l) => !l.hidden && mine(l) && l.sessions.some((s) => !s.visitor)).sort((a, b) => b.lastActivity - a.lastActivity).slice(0, 3);
      const items = [];
      const ref = (kind, key, x, y, more) => items.push({ kind: 'ref', ref: kind, key, x, y, kept: true, ...more });
      lanes.forEach((l, i) => {
        ref('worktree', l.id, 60 + i * 300, 90);
        l.sessions.filter((s) => !s.visitor).slice(0, 2).forEach((s, j) => ref('session', s.key, 60 + i * 300, 190 + j * 90));
      });
      (board.todos || []).filter(mine).slice(0, 3).forEach((t, i) => ref('todo', t.id, 60 + i * 300, 400));
      (board.prs || []).filter(mine).slice(0, 3).forEach((p, i) => ref('pr', p.key, 60 + i * 300, 490));
      ref('session', 'gone:session', 960, 190, { title: 'A session that has left' });
      const colors = ['gray', 'red', 'pink', 'purple', 'indigo', 'blue', 'teal', 'green'];
      colors.forEach((color, i) => items.push({ kind: 'note', id: 'note-' + i, text: i === 1 ? '' : 'A note in ' + color, x: 60 + i * 150, y: 600, w: 130, h: 70, color }));
      colors.forEach((color, i) => items.push({ kind: 'frame', id: 'frame-' + i, title: 'Frame ' + color, x: 60 + i * 150, y: 700, w: 130, h: 90, color, auto: i === 0 }));
      items.push({ kind: 'frame', id: 'frame-all', title: 'Work', x: 40, y: 50, w: 900, h: 330, color: 'gray' });
      return { version: 1, items, hidden: [], camera: { x: 0, y: 0, zoom: 1 }, scope: { cleared: true, since: Date.now() } };
    });
  };
})();`;
const quickReady = `await sd.waitFor(() => window.__calls.some((c) => c[0] === 'quick_ready'));`;
const pngPaste = (selector) => `const box = await sd.waitFor(${JSON.stringify(selector)}); box.focus(); const dt = new DataTransfer(); dt.items.add(new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'shot.png', { type: 'image/png' })); dt.items.add(new File(['log'], 'send.log', { type: 'text/plain' })); box.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));`;
const settings = (section) => [`await sd.click('.ca-side-config'); await sd.waitFor('.ca-settings');`, `await sd.click(sd.byText('.ca-settings-nav button', ${JSON.stringify(section)})); await sd.sleep(300);`];
const first = (selector) => `await sd.click(await sd.waitFor(${JSON.stringify(selector)}));`;
const openTools = `for (const b of document.querySelectorAll('.ca-thread-toolhead:not(:disabled), .ca-thread-runhead')) if (b.getAttribute('aria-expanded') === 'false') b.click();`;

// Run in every page just before it's recorded. A conversation shows the prompt above what's
// in view once it's scrolled, and whether it has been yet depends on timing: each is put at
// its end and told so.
export const steady = `for (const t of document.querySelectorAll('.ca-thread')) { t.scrollTop = t.scrollHeight; t.dispatchEvent(new Event('scroll')); }`;

export function scenes(board) {
  // The project with the most in it (worktrees, PRs, todos) shows the most.
  const count = new Map();
  for (const x of [...board.lanes.filter((l) => !l.hidden), ...(board.prs || []), ...(board.todos || [])]) count.set(x.project, (count.get(x.project) || 0) + 1);
  for (const name of [...count.keys()]) if (!board.projects.some((p) => p.name === name)) count.delete(name);
  const base = [
    // The main window's views, as they open.
    { name: 'worktrees', view: 'worktrees' },
    { name: 'worktrees-all', view: 'worktrees', project: 'All projects' },
    { name: 'worktrees-samples', view: 'worktrees', ...all, files: {}, steps: [first('.ca-bay-lanes .ca-lane')] },
    { name: 'worktrees-lane', view: 'worktrees', steps: [first('.ca-bay-lanes .ca-lane'), `const c = document.querySelector('.ca-split-caret'); if (c) c.click();`] },
    { name: 'worktrees-narrow', view: 'worktrees', size: '900,700' },
    { name: 'worktrees-narrower', view: 'worktrees', size: '700,600', steps: [first('.ca-sidetoggle')] },
    { name: 'worktrees-rail', view: 'worktrees', devices: all.devices, steps: [first('.ca-sidetoggle')] },
    { name: 'worktrees-switcher', view: 'worktrees', insights: all.insights, steps: [first('.ca-projswitch-btn')] },
    { name: 'worktrees-search', view: 'worktrees', steps: [`await sd.type('.ca-search input', 'co');`] },
    { name: 'worktrees-nothing', view: 'worktrees', steps: [`await sd.type('.ca-search input', 'zzzzqqqq no such thing');`] },
    { name: 'worktrees-usage', view: 'worktrees', usage: all.usage, steps: [{ hover: '.ca-uchip' }] },
    { name: 'worktrees-notices', view: 'worktrees', notices: all.notices, usage: all.usage, steps: [first('.ca-notice-open')] },
    { name: 'worktrees-loading', view: 'worktrees', loading: true, devices: all.devices },
    { name: 'worktrees-filtered', view: 'worktrees', insights: all.insights, steps: [first('.ca-side-chip'), `await sd.click('.ca-side-save');`] },
    { name: 'worktrees-devices', view: 'worktrees', devices: all.devices, steps: [`const s = document.querySelector('.ca-side-switch'); if (s) s.click();`] },
    { name: 'sessions', view: 'sessions' },
    { name: 'sessions-real', view: 'sessions', transcript: 'transcript-1', project: 'All projects' },
    { name: 'sessions-sample', view: 'sessions', setup: sampled, steps: [openTools, openTools] },
    { name: 'sessions-allow', view: 'sessions', setup: sampled + needy('allow'), steps: [first('.ca-lane.is-agent:has(.ca-glyph--needs)'), openTools, openTools] },
    { name: 'sessions-questions', view: 'sessions', setup: sampled + needy('questions'), steps: [first('.ca-lane.is-agent:has(.ca-glyph--needs)'), `const o = await sd.waitFor('.ca-question-choices button, .ca-question-choices label, .ca-question input'); o.click();`] },
    { name: 'sessions-insights', view: 'sessions', insights: all.insights, todos: all.todos, devices: all.devices },
    { name: 'sessions-oneline', view: 'sessions', steps: [`await sd.click(sd.byText('.ca-side-rows button', 'One line'));`, `await sd.click(sd.byText('.ca-side-sorts button', 'Started'));`] },
    { name: 'sessions-max', view: 'sessions', setup: sampled, steps: [first('.ca-sessinsp-max')] },
    { name: 'sessions-details', view: 'sessions', setup: sampled, todos: all.todos, steps: [first('.ca-sessinsp-detailsbtn')] },
    { name: 'sessions-side', view: 'sessions', setup: sampled, init: S + 'side-open.js' },
    { name: 'sessions-narrow', view: 'sessions', size: '860,640', setup: sampled },
    { name: 'sessions-rename', view: 'sessions', steps: [`await sd.rightClick(await sd.waitFor('.ca-lane.is-agent')); await sd.menu('Rename…');`] },
    { name: 'sessions-reply', view: 'sessions', setup: sampled, steps: [`await sd.type('.ca-composer textarea', 'Use two tries, and say so in the log.');`] },
    { name: 'sessions-slash', view: 'sessions', setup: sampled, steps: [`await sd.type('.ca-composer textarea', '/');`] },
    { name: 'prs', view: 'prs' },
    { name: 'prs-pr', view: 'prs', setup: sampled, steps: [first('.ca-prrow'), `for (const b of document.querySelectorAll('.ca-prfile-head, .ca-more-btn')) b.click();`] },
    { name: 'prs-real', view: 'prs', project: 'All projects', steps: [first('.ca-prrow'), `const b = sd.byText('.ca-linkbtn', 'Show'); if (b) b.click();`] },
    { name: 'prs-narrow', view: 'prs', size: '900,700', setup: sampled, steps: [first('.ca-prrow')] },
    { name: 'todo', view: 'todo' },
    { name: 'todo-samples', view: 'todo', todos: all.todos, steps: [`const r = await sd.waitFor('.ca-todorow'); r.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));`] },
    { name: 'todo-jev', view: 'todo', todos: S + 'todos-jev-sample.json', steps: [`const r = [...document.querySelectorAll('.ca-todorow')].find((x) => x.querySelector('.ca-todorow-done')) || document.querySelector('.ca-todorow'); r.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));`] },
    { name: 'todo-labels', view: 'todo', todos: all.todos, steps: [`const r = await sd.waitFor('.ca-todorow'); r.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));`, first('.ca-labelpick-anchor .ca-labelchip--add, .ca-labelchip--add')] },
    { name: 'todo-due', view: 'todo', todos: all.todos, steps: [`const r = await sd.waitFor('.ca-todorow'); r.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));`, first('.ca-duenote-show, .ca-duepick-anchor button')] },
    { name: 'todo-wait', view: 'todo', todos: all.todos, steps: [`const r = await sd.waitFor('.ca-todorow'); r.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));`, first('.ca-waitpick-anchor button')] },
    { name: 'todo-several', view: 'todo', todos: all.todos, steps: [`const rows = [...document.querySelectorAll('.ca-todorow')]; rows[0].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 })); rows[2].dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, metaKey: true }));`] },
    { name: 'todo-compose', view: 'todo', todos: all.todos, steps: [`await sd.type('.ca-todonew textarea, .ca-todonew input', 'Write the release notes #docs');`, `const b = sd.byText('.ca-linkbtn', 'done'); if (b) b.click();`] },
    { name: 'canvas', view: 'canvas' },
    // Settings, a pane at a time, and the other sheets.
    { name: 'settings-general', view: 'worktrees', steps: settings('General') },
    { name: 'settings-accounts', view: 'worktrees', found: S + 'found-sample.json', usage: all.usage, steps: settings('Accounts') },
    { name: 'settings-agents', view: 'worktrees', steps: settings('Agents') },
    { name: 'settings-devices', view: 'worktrees', devices: all.devices, steps: settings('Devices') },
    { name: 'settings-devices-off', view: 'worktrees', steps: settings('Devices') },
    { name: 'settings-projects', view: 'worktrees', steps: [...settings('Projects'), `const b = sd.byText('.ca-settings-pane button', 'Edit'); if (b) b.click();`] },
    { name: 'settings-jev', view: 'worktrees', steps: settings('Jev') },
    { name: 'settings-advanced', view: 'worktrees', steps: settings('Advanced') },
    { name: 'settings-file', view: 'worktrees', steps: [`await sd.click('.ca-side-config'); await sd.waitFor('.ca-settings');`, `await sd.click([...document.querySelectorAll('.ca-settings-nav button')].pop());`] },
    { name: 'close-idle', view: 'worktrees', steps: [`sd.emit('close-idle', null); await sd.waitFor('.ca-dialog');`] },
    // The other windows.
    { name: 'quick', view: 'quick' },
    { name: 'quick-typed', view: 'quick', steps: [`await sd.waitFor(() => window.__calls.some((c) => c[0] === 'quick_ready')); await sd.type('.ca-quick-input textarea, .ca-quick textarea', 'fix the retry in send');`] },
    { name: 'quick-route', view: 'quick', route: S + 'route-sample.json', steps: [`await sd.waitFor(() => window.__calls.some((c) => c[0] === 'quick_ready')); await sd.type('.ca-quick-input textarea, .ca-quick textarea', 'fix the retry in send');`] },
    { name: 'quick-start', view: 'quick', route: S + 'route-start-sample.json', todos: all.todos, steps: [`await sd.waitFor(() => window.__calls.some((c) => c[0] === 'quick_ready')); await sd.type('.ca-quick-input textarea, .ca-quick textarea', 'start on the release notes');`] },
    { name: 'quick-agents', view: 'quick', usage: all.usage, steps: [`await sd.waitFor(() => window.__calls.some((c) => c[0] === 'quick_ready') && document.querySelector('.ca-quick-agent .ca-quick-chip:not(:disabled)')); window.dispatchEvent(new KeyboardEvent('keydown', { key: 'l', metaKey: true, bubbles: true, cancelable: true }));`] },
    { name: 'quick-project', view: 'quick', steps: [`await sd.waitFor(() => window.__calls.some((c) => c[0] === 'quick_ready')); await sd.click('.ca-quick-project');`] },
    { name: 'quick-reply', view: 'quick', setup: sampled, steps: [`await sd.waitFor(() => window.__calls.some((c) => c[0] === 'quick_ready')); await sd.waitFor('.ca-quick-row'); const t = document.querySelector('.ca-quick textarea'); t.focus(); t.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true }));`] },
    { name: 'quick-samples', view: 'quick', ...all },
    { name: 'bar', view: 'bar' },
    { name: 'bar-wide', view: 'bar', size: '1600,64', todos: all.todos, insights: all.insights },
    { name: 'bar-loading', view: 'bar', loading: true },
    { name: 'notch', view: 'notch' },
    { name: 'notch-pills', view: 'notch', steps: [first('.ca-notch-shape')] },
    { name: 'notch-session', view: 'notch', size: '1512,720', setup: sampled, init: S + 'notch-session-init.js' },
    { name: 'notch-drawn', view: 'notch', notch: 'drawn', steps: [first('.ca-notch-shape')] },
    { name: 'peek', view: 'peek', transcript: 'transcript-1' },
    { name: 'peek-other', view: 'peek', transcript: 'transcript-2' },
    { name: 'peek-sample', view: 'peek', setup: sampled, steps: [openTools, openTools] },
    { name: 'peek-allow', view: 'peek', setup: sampled + needy('allow') },
    { name: 'peek-side', view: 'peek', setup: sampled, init: S + 'side-open.js' },
    { name: 'kept', view: 'kept', setup: sampled },
    { name: 'desktop-session', view: 'desktop', pin: 'session' },
    { name: 'desktop-worktree', view: 'desktop', pin: 'worktree' },
    { name: 'desktop-todo', view: 'desktop', pin: 'todo', todos: all.todos },
    { name: 'desktop-pr', view: 'desktop', pin: 'pr' },
    { name: 'lineage', view: 'lineage' },
  ];
  base.push(
    // More of each: states a click, a drag, a failure or an odd board gets to.
    { name: 'canvas-doc', view: 'canvas', setup: canvasDoc, todos: all.todos, devices: all.devices },
    { name: 'canvas-select', view: 'canvas', setup: canvasDoc, steps: [{ click: '.ca-cv-card.is-session', dx: 20 }, `await sd.sleep(700);`] },
    { name: 'canvas-popover', view: 'canvas', setup: canvasDoc, steps: [{ hover: '.ca-cv-card.is-session', dx: 20 }, `await sd.sleep(900);`] },
    { name: 'canvas-tidy', view: 'canvas', setup: canvasDoc, steps: [{ click: '.ca-cv-toolbar button:nth-of-type(3)' }, `if (!document.querySelector('.ca-cv-tidy')) (sd.byText('.ca-cv-toolbar button', 'Tidy')).click();`] },
    { name: 'canvas-add', view: 'canvas', setup: canvasDoc, steps: [`sd.byText('.ca-cv-toolbar button', 'Add').click();`] },
    { name: 'canvas-note', view: 'canvas', setup: canvasDoc, steps: [{ dblclick: '.ca-cv-note' }] },
    { name: 'canvas-frame', view: 'canvas', setup: canvasDoc, steps: [{ dblclick: '.ca-cv-frame-title' }] },
    { name: 'canvas-marquee', view: 'canvas', setup: canvasDoc, steps: [{ drag: '.ca-cv', by: [-300, -200], fx: 0.97, fy: 0.62 }] },
    { name: 'canvas-tool', view: 'canvas', setup: canvasDoc, steps: [{ click: '.ca-cv-toolbar .ca-cv-tool' }] },
    { name: 'canvas-tool2', view: 'canvas', setup: canvasDoc, steps: [{ click: '.ca-cv-toolbar .ca-cv-tool:nth-of-type(2)' }, { drag: '.ca-cv', by: [-200, -120], fx: 0.97, fy: 0.62 }] },
    { name: 'canvas-pan', view: 'canvas', setup: canvasDoc, steps: [`window.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', code: 'Space', bubbles: true }));`, { drag: '.ca-cv', by: [-200, -120], fx: 0.97, fy: 0.62 }] },
    { name: 'canvas-move', view: 'canvas', setup: canvasDoc, steps: [{ drag: '.ca-cv-card.is-worktree', by: [80, 40], dx: 30 }] },
    { name: 'canvas-empty', view: 'canvas', setup: patch('canvas_load', `r = null`) + `(() => { const real = window.__TAURI_INTERNALS__.invoke; window.__TAURI_INTERNALS__.invoke = (c, a, o) => (c === 'canvas_load' ? Promise.resolve({ version: 1, items: [], hidden: [], camera: null, scope: { cleared: true, since: Date.now() } }) : real(c, a, o)); })();` },
    { name: 'worktrees-drag', view: 'worktrees', steps: [{ drag: '.ca-bay-lanes .ca-lane', by: [30, 90], dx: 200 }] },
    { name: 'worktrees-resize', view: 'worktrees', steps: [{ drag: '.ca-resizer', by: [60, 0] }] },
    { name: 'worktrees-preview', view: 'worktrees', steps: [`await sd.rightClick(await sd.waitFor('.ca-bay-lanes .ca-lane')); await sd.menu('preview URL…');`] },
    { name: 'worktrees-update', view: 'worktrees', setup: `window.__update = { current: '0.1.0', state: 'ready', version: '0.2.0', notes: 'Faster boards.', checkedAt: Date.now() };` },
    { name: 'worktrees-usage-odd', view: 'worktrees', usage: { usage: [{ account: 'claude', harness: 'claude', plan: 'Max', readAgo: 90, meters: [{ key: 'session', label: '5h', window: '5-hour session', percent: 104, resetsIn: 20, severity: 'critical' }, { key: 'week', label: 'Week', window: 'Weekly', percent: 81, resetsIn: 900, severity: 'warning' }] }, { account: 'claude2', harness: 'claude', needsSignIn: true, error: 'Signed out' }, { account: 'codex', harness: 'codex', error: 'The usage endpoint answered 429', readAgo: 400, meters: [{ key: 'week', label: 'Week', window: 'Weekly', percent: 12, resetsIn: 3000 }] }, { account: 'opencode', harness: 'opencode', loading: true }] }, steps: [{ hover: '.ca-uchip:not(.is-attn)' }] },
    { name: 'worktrees-prs-odd', view: 'worktrees', setup: patch('get_board', `(r.prs || []).slice(0, 6).forEach((p, i) => { if (i % 3 === 0) p.checks = { passed: 2, failed: 0, pending: 3, skipped: 0, failing: [] }; if (i % 3 === 1) { p.checks = { passed: 2, failed: 2, pending: 0, skipped: 1, failing: ['lint', 'unit tests'] }; p.state = 'changes'; } }); r.lanes.forEach((l, i) => { if (l.pr && i % 2) { l.pr.checks = { passed: 1, failed: 1, pending: 1, skipped: 0, failing: ['build'] }; } });`), steps: [first('.ca-bay-lanes .ca-lane:has(.ca-pr), .ca-bay-lanes .ca-lane')] },
    { name: 'prs-odd', view: 'prs', setup: sampled + patch('get_board', `(r.prs || []).slice(0, 6).forEach((p, i) => { if (i % 3 === 0) p.checks = { passed: 2, failed: 0, pending: 3, skipped: 0, failing: [] }; if (i % 3 === 1) { p.checks = { passed: 2, failed: 2, pending: 0, skipped: 1, failing: ['lint', 'unit tests'] }; p.state = 'changes'; } });`), steps: [`await sd.click([...document.querySelectorAll('.ca-prrow')][1]);`] },
    { name: 'prs-drag', view: 'prs', steps: [{ drag: '.ca-prrow', by: [20, 120], dx: 200 }] },
    { name: 'sessions-toast', view: 'sessions', steps: [`await sd.rightClick(await sd.waitFor('.ca-lane.is-agent')); await sd.menu('Archive');`] },
    { name: 'sessions-archived', view: 'sessions', setup: patch('get_board', `let n = 0; r.lanes.forEach((l) => l.sessions.forEach((s) => { if (!s.live && n++ < 4) s.archived = true; }));`), steps: [`const b = sd.byText('.ca-linkbtn', 'archived'); if (b) b.click();`] },
    { name: 'sessions-pinned', view: 'sessions', steps: [`await sd.rightClick(await sd.waitFor('.ca-lane.is-agent')); await sd.menu('Pin');`, { drag: '.ca-lane.is-agent', by: [10, 80], dx: 200 }] },
    { name: 'sessions-prhover', view: 'sessions', setup: sampled, steps: [{ hover: '.ca-prlink' }, `await sd.sleep(900);`] },
    { name: 'sessions-sent', view: 'sessions', setup: sampled, steps: [pngPaste('.ca-composer textarea'), `await sd.type('.ca-composer textarea', 'Use two tries.'); document.querySelector('.ca-composer textarea').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));`] },
    { name: 'sessions-attached', view: 'sessions', setup: sampled, steps: [pngPaste('.ca-composer textarea')] },
    { name: 'sessions-copied', view: 'sessions', setup: sampled + `navigator.clipboard.writeText = () => Promise.resolve();`, steps: [openTools, openTools, first('.ca-codeblock .ca-copybtn')] },
    { name: 'sessions-unread', view: 'sessions', setup: sampled + patch('session_page', `throw new Error('The transcript is gone.')`) },
    { name: 'sessions-hover', view: 'sessions', setup: sampled, steps: [{ hover: '.ca-lane.is-agent:nth-child(3), .ca-lane.is-agent' }] },
    { name: 'sessions-focus', view: 'sessions', setup: sampled, steps: [{ tab: 4 }] },
    { name: 'todo-done', view: 'todo', todos: all.todos, steps: [`const b = sd.byText('.ca-todo-done .ca-linkbtn, .ca-bay-action .ca-linkbtn', 'Show'); if (b) b.click();`, `const r = document.querySelector('.ca-todorow.is-done'); if (r) r.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));`] },
    { name: 'todo-drag', view: 'todo', todos: all.todos, steps: [{ drag: '.ca-todorow', by: [20, 110], dx: 200 }] },
    { name: 'todo-edit', view: 'todo', todos: all.todos, steps: [`const r = await sd.waitFor('.ca-todorow'); r.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0 }));`, first('.ca-desc-view'), pngPaste('.ca-desc-area')] },
    { name: 'settings-broken', view: 'worktrees', setup: patch('get_config', `r.error = 'TOML parse error at line 14, column 9: invalid string'`), steps: settings('General') },
    { name: 'settings-shortcut', view: 'worktrees', steps: [...settings('General'), first('.ca-shortcut')] },
    { name: 'settings-remove', view: 'worktrees', steps: [...settings('Accounts'), `const b = sd.byText('.ca-settings-pane button', 'Remove'); if (b) b.click();`] },
    { name: 'settings-add', view: 'worktrees', steps: [...settings('Accounts'), `const b = sd.byText('.ca-settings-pane button', 'Add'); if (b) b.click();`] },
    { name: 'settings-link', view: 'worktrees', devices: all.devices, steps: [...settings('Devices'), `const b = sd.byText('.ca-settings-pane button', 'Link'); if (b) b.click();`] },
    { name: 'quick-todo', view: 'quick', todos: all.todos, steps: [quickReady + `const b = await window.__TAURI_INTERNALS__.invoke('get_board'); window.__quickOpen({ todos: b.todos.slice(0, 2).map((t) => t.id) }, b);`] },
    { name: 'quick-attached', view: 'quick', steps: [quickReady + pngPaste('.ca-quick textarea')] },
    { name: 'quick-error', view: 'quick', setup: `(() => { const real = window.__TAURI_INTERNALS__.invoke; window.__TAURI_INTERNALS__.invoke = (c, a, o) => (c === 'start_session' || c === 'start_from_quick' ? Promise.reject(new Error('That folder is gone.')) : real(c, a, o)); })();`, steps: [quickReady + `await sd.type('.ca-quick textarea', 'start on the release notes'); document.querySelector('.ca-quick textarea').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: true, bubbles: true, cancelable: true }));`] },
    { name: 'quick-match', view: 'quick', steps: [quickReady + `const t = (document.querySelector('.ca-quick-row-title') || {}).textContent || 'a'; await sd.type('.ca-quick textarea', t.split(' ')[0]);`, { hover: '.ca-quick-row' }] },
    { name: 'quick-drag', view: 'quick', steps: [quickReady, { drag: '.ca-quick-row', by: [200, 60], dx: 100 }] },
    { name: 'bar-odd', view: 'bar', setup: patch('bar_feed', `r.logoStatus = 'error'; r.items.slice(0, 5).forEach((it, i) => { it.status = ['error', 'needs', 'working', 'unread', 'idle'][i]; it.group = ['needs', 'needs', 'working', 'unread', 'unread'][i]; it.hidden = false; it.asks = i === 1; });`), steps: [{ hover: '.ca-hud-pill' }] },
    { name: 'bar-project', view: 'bar', steps: [first('.ca-hud-project')] },
    { name: 'bar-drag', view: 'bar', steps: [{ drag: '.ca-hud-pill[data-key]', by: [120, 30] }] },
    { name: 'bar-working', view: 'bar', setup: patch('bar_feed', `r.logoStatus = 'working'`) },
    { name: 'bar-unread', view: 'bar', setup: patch('bar_feed', `r.logoStatus = 'unread'`) },
    { name: 'notch-hover', view: 'notch', steps: [`sd.emit('notch-pointer', true);`] },
    { name: 'notch-pinned', view: 'notch', steps: [first('.ca-notch-shape'), first('.ca-notch-pin')] },
    { name: 'notch-sizing', view: 'notch', size: '1512,720', setup: sampled, init: S + 'notch-session-init.js', steps: [`await sd.waitFor('.ca-notch-session');`, { drag: '.ca-notch-grip', by: [0, 60] }] },
    { name: 'notch-loading', view: 'notch', loading: true },
    { name: 'peek-gone', view: 'peek', setup: `localStorage.setItem('codealong.preview.peek', 'claude:nobody:gone');` },
    { name: 'peek-questions', view: 'peek', setup: sampled + needy('questions') },
    { name: 'desktop-gone', view: 'desktop', pin: 'session:claude:nobody:gone' },
    { name: 'desktop-hover', view: 'desktop', pin: 'worktree', steps: [{ hover: '.ca-desk-session, .ca-cv-card' }] },
    { name: 'lineage-odd', view: 'lineage', size: '900,420' },
  );
  const most = [...count].sort((a, b) => b[1] - a[1])[0]?.[0];
  // What a scene is there to show arrives when the stub answers: wait for it by name.
  const waits = [
    [(s) => s.name === 'sessions-questions' || s.name === 'peek-questions', '.ca-question-choices'],
    [(s) => s.name.endsWith('-allow'), '.ca-approval-head'],
    [(s) => s.name === 'sessions-unread', '.ca-thread.is-fallback, .ca-thread .ca-empty'],
    [(s) => /^(prs-pr|prs-narrow|prs-odd)$/.test(s.name), '.ca-prbody'],
    [(s) => /^sessions-/.test(s.name) || ['peek', 'kept'].includes(s.view), '.ca-thread-said, .ca-thread .ca-empty'],
    [(s) => s.name === 'notch-session' || s.name === 'notch-sizing', '.ca-notch-session'],
    [(s) => s.view === 'bar', '.ca-hud'],
    [(s) => s.view === 'quick', '.ca-quick'],
  ];
  for (const s of base) if (!s.wait && !s.loading && s.name !== 'peek-gone') s.wait = waits.find(([on]) => on(s))?.[1];
  return base.flatMap((s) => ['light', 'dark'].map((scheme) => ({ project: most, ...s, name: `${s.name}.${scheme}`, scheme })));
}
