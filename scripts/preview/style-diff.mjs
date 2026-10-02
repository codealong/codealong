#!/usr/bin/env node
// Shows that a change to the stylesheets changed nothing on the page: renders every scene in
// style-scenes.mjs (each view, window, dialog and pane the preview can reach, in light and in
// dark) in headless Chrome, records every element's computed style, and compares two such
// recordings. For moving rules between files, changing the order of the index, or deleting
// rules thought dead: record before, change, record after, compare.
//
//   node scripts/preview/style-diff.mjs data <data dir>
//       Takes the engine's snapshots once (the board, the bar's feed, accounts, settings, two
//       sessions' conversations, one PR's detail) with the engine binary in the shared build directory. They are
//       your real data: keep the folder out of the repo. Every recording made from one folder
//       renders the same board, which is what makes two of them comparable; taking them again
//       also spends GitHub API points.
//   node scripts/preview/style-diff.mjs record <out dir> --data <data dir> [--dist dist]
//       [--only <words>] [--shots] [--jobs 3]
//       Renders each scene from <dist> (an `npm run build` output; copy the one from before
//       your change somewhere first) and writes <out dir>/<scene>.json.gz. --only keeps scenes
//       whose name has one of the comma-separated words; --shots also writes a PNG a scene, to
//       see that it shows what it should.
//   node scripts/preview/style-diff.mjs compare <before dir> <after dir> [--max 40]
//       Prints, a scene, how many elements were compared and each property that differs, and
//       exits 1 if any does or if the two pages aren't the same elements.
//   node scripts/preview/style-diff.mjs built <dist before> <dist after>
//       Compares the stylesheets two builds wrote (dist folders, or the .css files): byte for
//       byte the same, the same rules in the same order, the same rules moved, or the rules only
//       one has. A change that only moves text between files without reordering it should build
//       byte for byte the same; Vite inlines the index's imports in order and minifies, so
//       comments and spacing never show.
//   node scripts/preview/style-diff.mjs coverage <dir> [--list]
//       Says how many of the stylesheets' selectors had an element to match in some scene of a
//       recording, and with --list which didn't: what a comparison says nothing about.
//
// How it holds still: the clock is frozen at the board's own time (so "3m ago" reads the same
// in both), each scene gets a browser context of its own (nothing kept from the last), a scene
// is recorded once its elements have stopped changing, and animations are finished (or held at
// their start, the ones that never end) first. Two recordings of one build should compare
// equal; check that before trusting a difference.
//
// What it can't see: styles that need the pointer or the keyboard (:hover, :focus-visible,
// :active) except where a scene puts the pointer somewhere, and states no scene reaches. A
// change of order is argued for those from the rules themselves (see src/styles/app.css).
// The page setup for each view mirrors scripts/preview.sh; a view added there needs adding
// to `setupFor` here.
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync, cpSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gunzipSync, gzipSync } from 'node:zlib';
import { scenes as allScenes, steady } from './style-scenes.mjs';

const root = resolve(import.meta.dirname, '../..');
const chrome = process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const [mode, ...rest] = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = rest.indexOf(`--${name}`);
  return i < 0 ? fallback : rest[i + 1];
};
const has = (name) => rest.includes(`--${name}`);

// ---- data: the engine's snapshots, once ----

function takeData(dir) {
  const bin = process.env.PREVIEW_BIN || join(execFileSync(join(root, 'scripts/target-dir.sh'), { encoding: 'utf8' }).trim(), 'release/codealong');
  mkdirSync(dir, { recursive: true });
  const snap = (args, file) => {
    try {
      writeFileSync(join(dir, file), execFileSync(bin, ['--snapshot', ...args], { stdio: ['ignore', 'pipe', 'ignore'], maxBuffer: 1 << 28 }));
    } catch (e) {
      console.error(`${file}: ${e.message}`);
    }
  };
  snap([], 'board.json');
  for (const name of ['bar', 'accounts', 'start-options', 'editor-icons', 'config', 'found']) snap([`--${name}`], name === 'bar' ? 'bar-feed.json' : `${name}.json`);
  // Two conversations to show in a session's panel: the latest session's, and the latest of
  // another harness's.
  const board = JSON.parse(readFileSync(join(dir, 'board.json'), 'utf8'));
  const sessions = board.lanes.flatMap((l) => l.sessions.filter((s) => !s.visitor && !s.archived)).sort((a, b) => b.lastActivity - a.lastActivity);
  const picks = [sessions[0], sessions.find((s) => s.harness !== sessions[0]?.harness)].filter(Boolean);
  picks.forEach((s, i) => {
    const sub = join(dir, `transcript-${i + 1}`);
    mkdirSync(sub, { recursive: true });
    writeFileSync(join(sub, 'transcript-key.json'), JSON.stringify({ key: s.key }));
    for (const what of ['transcript', 'pending']) {
      try {
        writeFileSync(join(sub, `${what}.json`), execFileSync(bin, ['--snapshot', `--${what}`, s.key], { stdio: ['ignore', 'pipe', 'ignore'], maxBuffer: 1 << 28 }));
      } catch (e) {
        console.error(`${what} of ${s.key}: ${e.message}`);
      }
    }
  });
  // One PR's detail, for the PR panel (the stub answers with it for any PR): the latest open one.
  const pr = (board.prs || []).find((p) => p.state === 'open') || (board.prs || [])[0];
  if (pr) snap(['--pr', pr.repoSlug, String(pr.number)], 'pr-detail.json');
  const images = process.env.PREVIEW_IMAGES || join(process.env.HOME, 'Library/Application Support/Codealong/images');
  if (existsSync(images)) cpSync(images, join(dir, 'images'), { recursive: true });
  console.log(`${dir}: ${board.lanes.length} worktrees, ${sessions.length} sessions, ${(board.prs || []).length} PRs`);
}

// ---- the page for a scene, as scripts/preview.sh sets it up ----

const backdrop = 'linear-gradient(135deg,rgb(93,111,134),rgb(169,182,196))';
function setupFor(scene, data) {
  const view = scene.view;
  const size = (scene.size || { bar: '1100,64', notch: '1512,320', quick: '760,640', desktop: '360,300', lineage: '1440,420', peek: '520,640', kept: '640,720' }[view] || '1440,900').split(',').map(Number);
  let js = `localStorage.setItem('codealong.view',${JSON.stringify(view)});localStorage.setItem('codealong.age','all');`;
  let css = '';
  let scripts = '';
  if (scene.project) js += `localStorage.setItem('codealong.project',${JSON.stringify(scene.project)});`;
  if (scene.loading) js += 'window.__PREVIEW_LOADING__=true;';
  const key = scene.transcript ? JSON.parse(readFileSync(join(data, scene.transcript, 'transcript-key.json'), 'utf8')).key : '';
  if (view === 'bar') {
    js += `window.__CODEALONG_BAR__=true;Object.defineProperty(screen,'availWidth',{value:${size[0]}});`;
    css = `html.is-bar{background:${backdrop}!important}`;
  }
  if (view === 'notch') {
    const drawn = scene.notch === 'drawn';
    const notch = drawn ? `{x:${Math.floor(size[0] / 2)},top:1440,width:180,height:25,real:false}` : `{x:${Math.floor(size[0] / 2)},top:982,width:185,height:32,real:true}`;
    const menu = drawn ? 25 : 32;
    js += `window.__CODEALONG_BAR__=true;window.__BAR_MODE__={mode:'notch',notch:${notch}};Object.defineProperty(screen,'availWidth',{value:${size[0]}});Object.defineProperty(screen,'height',{value:${drawn ? 1440 : 982}});`;
    css = `html.is-bar{background:linear-gradient(rgba(236,236,238,0.82) 0 ${menu}px,transparent ${menu}px),${backdrop}!important}`;
  }
  if (view === 'quick') {
    js += 'window.__CODEALONG_QUICK__=true;';
    css = `html.is-quick{background:${backdrop}!important}`;
  }
  if (view === 'desktop') {
    js += `window.__CODEALONG_DESKTOP__=true;window.__CODEALONG_PIN__=${JSON.stringify(scene.pin || 'session')};`;
    css = `html.is-desktop{background:${backdrop}!important}`;
  }
  if (view === 'lineage') scripts = '<script src="./lineage-desk.js"></script>';
  if (view === 'peek' || view === 'kept') {
    js += `localStorage.setItem('codealong.preview.peek',${JSON.stringify(key)});window.__CODEALONG_${view === 'peek' ? 'PEEK' : 'KEPT'}__=true;`;
    css = `html.is-peek{background:${backdrop}!important}`;
  }
  if (scene.setup) js += scene.setup;
  return { size, head: `<script>${js}</script>${css ? `<style>${css}</style>` : ''}${scripts}` };
}

// The clock, stopped: every `new Date()` and `Date.now()` is the board's own time, so ages
// and due dates read the same in every recording made from one data folder.
const frozen = (at) => `(()=>{const D=Date,T=${at};function F(...a){if(!new.target)return new D(T).toString();return a.length?new D(...a):new D(T);}F.prototype=D.prototype;F.now=()=>T;F.parse=D.parse;F.UTC=D.UTC;window.Date=F;})();`;

const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg', '.woff2': 'font/woff2' };

function serve(dist, data, scenes) {
  const board = JSON.parse(readFileSync(join(data, 'board.json'), 'utf8'));
  const index = readFileSync(join(dist, 'index.html'), 'utf8');
  const byName = new Map(scenes.map((s) => [s.name, s]));
  const fixtures = { todos: 'todos-fixture.json', usage: 'usage-fixture.json', notices: 'notices-fixture.json', devices: 'devices-fixture.json', insights: 'insights-fixture.json', route: 'route-fixture.json', canvas: 'canvas-fixture.json', found: 'found.json', prDetail: 'pr-detail.json' };
  const server = createServer((req, res) => {
    const send = (code, body, type) => {
      res.writeHead(code, { 'content-type': type || 'text/plain', 'cache-control': 'no-store' });
      res.end(body);
    };
    const m = /^\/s\/([^/]+)\/(.*?)(\?.*)?$/.exec(decodeURIComponent(req.url));
    const scene = m && byName.get(m[1]);
    if (!scene) return send(404, 'no such scene');
    const file = m[2];
    if (file.includes('..')) return send(404, 'no');
    const type = types[extname(file)] || 'application/octet-stream';
    if (file === 'preview.html') {
      const { head } = setupFor(scene, data);
      // The app starts once its fonts are in: the bar measures its pills as it first lays out,
      // and with the fonts still on their way it can fit them differently from one load to the next.
      let app = '';
      const page = index
        .replace(/<script type="module"[^>]*src="([^"]+)"[^>]*><\/script>/, (_, src) => ((app = src), ''))
        .replace('<head>', `<head><script>${frozen(board.generatedAt + 60000)}</script><script src="./fake-tauri.js"></script>${head}`)
        .replace('</body>', `<script>Promise.all([...document.fonts].map((f) => f.load())).catch(() => {}).then(() => { const s = document.createElement('script'); s.type = 'module'; s.src = ${JSON.stringify(app)}; document.head.appendChild(s); });</script></body>`);
      if (!app) return send(500, 'the build has no module script to start');
      return send(200, scene.init ? page.replace('</body>', '<script src="./init.js"></script></body>') : page, type);
    }
    // A scene's `init` is a script the page runs as it loads, as preview.sh's PREVIEW_INIT is.
    if (file === 'init.js' && scene.init) return send(200, readFileSync(resolve(root, scene.init)), type);
    if (['fake-tauri.js', 'lineage-desk.js', 'session-sample.json', 'pr-detail-sample.json'].includes(file)) return send(200, readFileSync(join(root, 'scripts/preview', file)), type);
    // A scene's sample files stand in for the data folder's, and where a scene names none the
    // page gets none (the stub treats a missing one as "use the board's own").
    for (const [prop, name] of Object.entries(fixtures)) {
      if (file !== name) continue;
      if (scene[prop]) return send(200, typeof scene[prop] === 'string' ? readFileSync(resolve(root, scene[prop])) : JSON.stringify(scene[prop]), type);
      if (prop !== 'found' && prop !== 'prDetail') return send(404, 'none');
    }
    if (['transcript.json', 'pending.json', 'transcript-key.json'].includes(file)) {
      const at = scene.transcript && join(data, scene.transcript, file);
      return at && existsSync(at) ? send(200, readFileSync(at), type) : send(404, 'none');
    }
    for (const base of [data, dist]) {
      const at = join(base, file);
      if (at.startsWith(base + sep) && existsSync(at) && statSync(at).isFile()) return send(200, readFileSync(at), type);
    }
    send(404, 'not found');
  });
  return new Promise((done) => server.listen(0, '127.0.0.1', () => done(server)));
}

// ---- Chrome, over its debugging protocol ----

async function launch() {
  const profile = mkdtempSync(join(tmpdir(), 'style-diff-'));
  const child = spawn(chrome, ['--headless=new', '--disable-gpu', '--enable-unsafe-swiftshader', '--force-device-scale-factor=1', '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], { stdio: 'ignore' });
  // Chrome writes its port and its socket's path to a file in the profile once it's listening.
  const portFile = join(profile, 'DevToolsActivePort');
  const listening = () => (existsSync(portFile) ? readFileSync(portFile, 'utf8').trim().split('\n') : []);
  for (let i = 0; i < 600 && listening().length < 2; i++) await new Promise((r) => setTimeout(r, 100));
  if (listening().length < 2) {
    child.kill('SIGKILL');
    throw new Error('Chrome never opened its debugging port');
  }
  const [port, path] = listening();
  const ws = new WebSocket(`ws://127.0.0.1:${port}${path}`);
  await new Promise((ok, no) => {
    ws.onopen = ok;
    ws.onerror = () => no(new Error("Chrome's debugging socket didn't open"));
    setTimeout(() => no(new Error("Chrome's debugging socket didn't open in a minute")), 60000);
  }).catch((e) => {
    child.kill('SIGKILL');
    throw e;
  });
  let next = 0;
  const waiting = new Map();
  const listeners = new Set();
  ws.onmessage = (e) => {
    const msg = JSON.parse(e.data);
    if (msg.id && waiting.has(msg.id)) {
      const { ok, no } = waiting.get(msg.id);
      waiting.delete(msg.id);
      msg.error ? no(new Error(`${msg.error.message}`)) : ok(msg.result);
    } else for (const l of listeners) l(msg);
  };
  // A call Chrome never answers (it can hang on a busy Mac) fails after two minutes.
  const send = (method, params = {}, sessionId) =>
    new Promise((ok, no) => {
      const id = ++next;
      const timer = setTimeout(() => waiting.delete(id) && no(new Error(`Chrome didn't answer ${method}`)), 120000);
      waiting.set(id, { ok: (v) => (clearTimeout(timer), ok(v)), no: (e) => (clearTimeout(timer), no(e)) });
      ws.send(JSON.stringify({ id, method, params, sessionId }));
    });
  const close = () => {
    try {
      ws.close();
    } catch {}
    child.kill('SIGKILL');
    try {
      rmSync(profile, { recursive: true, force: true });
    } catch {}
  };
  return { send, listeners, close, pid: child.pid };
}

// What runs in the page. `sd` is what a scene's steps get: waiting for an element, clicking,
// keys, typing into React's inputs.
const helpers = `window.sd = {
  sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
  async waitFor(what, ms = 15000) {
    const find = typeof what === 'string' ? () => document.querySelector(what) : what;
    const t = performance.now();
    for (;;) {
      const hit = find();
      if (hit) return hit;
      if (performance.now() - t > ms) throw new Error('never appeared: ' + what);
      await sd.sleep(50);
    }
  },
  async click(what, opts) {
    const el = typeof what === 'string' ? await sd.waitFor(what) : what;
    for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click']) el.dispatchEvent(new (type.startsWith('pointer') ? PointerEvent : MouseEvent)(type, Object.assign({ bubbles: true, cancelable: true, view: window, button: 0 }, opts)));
    return el;
  },
  async rightClick(what) {
    const el = typeof what === 'string' ? await sd.waitFor(what) : what;
    el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, view: window, button: 2 }));
    return el;
  },
  key(key, opts) {
    const init = Object.assign({ key, bubbles: true, cancelable: true }, opts);
    (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keydown', init));
    (document.activeElement || document.body).dispatchEvent(new KeyboardEvent('keyup', init));
  },
  async type(what, text) {
    const el = typeof what === 'string' ? await sd.waitFor(what) : what;
    el.focus();
    const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, text);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    return el;
  },
  emit: (event, payload) => window.__emit(event, payload),
  // Picks an item of the native menu the page last opened (the stub keeps it), by its label.
  async menu(label) {
    const item = await sd.waitFor(() => window.__menu().find((m) => m[0] === label) || window.__menu().find((m) => m[0].includes(label)));
    item[1]();
  },
  byText(selector, text) {
    return [...document.querySelectorAll(selector)].find((e) => e.textContent.trim() === text) || [...document.querySelectorAll(selector)].find((e) => e.textContent.includes(text));
  },
};`;

// The page's elements as one number: it stops changing once the page has settled.
const signature = `(() => { let h = 0, n = 0; for (const e of document.querySelectorAll('*')) { n++; const c = e.getAttribute('class') || ''; for (let i = 0; i < c.length; i++) h = (h * 31 + c.charCodeAt(i)) | 0; h = (h * 31 + e.childElementCount) | 0; } return n + ':' + h + ':' + document.fonts.status; })()`;

// Every element's computed style. Values are kept once each (`values`), a style as the list
// of its values' numbers, and equal styles once (`styles`), so a page of thousands of elements
// is a few megabytes.
const record = `(async () => {
  for (const a of document.getAnimations()) { try { a.finish(); } catch { try { a.pause(); a.currentTime = 0; } catch {} } }
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  for (const a of document.getAnimations()) { try { a.finish(); } catch { try { a.pause(); a.currentTime = 0; } catch {} } }
  // Every property Chrome computes, and every custom property a stylesheet or an element's own
  // style sets (Chrome lists those in no fixed order, and only the ones the root has), by name.
  const names = new Set();
  const probe = getComputedStyle(document.documentElement);
  for (let i = 0; i < probe.length; i++) names.add(probe[i]);
  const customs = (style) => { for (let i = 0; i < style.length; i++) if (style[i].startsWith('--')) names.add(style[i]); };
  const rules = (list) => { for (const r of list) { if (r.style) customs(r.style); if (r.cssRules) rules(r.cssRules); } };
  for (const sheet of document.styleSheets) rules(sheet.cssRules);
  for (const el of document.querySelectorAll('[style]')) customs(el.style);
  const props = [...names].sort();
  const values = new Map(), styles = new Map(), els = [];
  const styleOf = (el, pseudo) => {
    const cs = getComputedStyle(el, pseudo);
    const row = new Array(props.length);
    for (let i = 0; i < props.length; i++) {
      const v = cs.getPropertyValue(props[i]);
      let id = values.get(v);
      if (id === undefined) values.set(v, (id = values.size));
      row[i] = id;
    }
    const key = row.join(',');
    let id = styles.get(key);
    if (id === undefined) styles.set(key, (id = styles.size));
    return id;
  };
  const skip = new Set(['SCRIPT', 'STYLE', 'HEAD', 'META', 'LINK', 'TITLE']);
  const walk = (el, path) => {
    if (skip.has(el.tagName)) return;
    const tag = el.tagName.toLowerCase();
    const entry = [path, el.getAttribute('class') || '', styleOf(el, null)];
    // Generated boxes, where the element has them, and the placeholder of what takes typing.
    for (const pseudo of ['::before', '::after', '::marker', '::placeholder']) {
      if (pseudo === '::placeholder' ? !(tag === 'input' || tag === 'textarea') : pseudo === '::marker' ? tag !== 'li' && tag !== 'summary' : getComputedStyle(el, pseudo).content === 'none') entry.push(-1);
      else entry.push(styleOf(el, pseudo));
    }
    els.push(entry);
    let i = 0;
    for (const child of el.children) walk(child, path + '/' + child.tagName.toLowerCase() + '[' + i++ + ']');
  };
  walk(document.documentElement, 'html');
  // Which of the stylesheets' selectors some element here matches, pointer and keyboard states
  // aside (\`.x:hover\` counts when there's an \`.x\`): what the scenes cover, for \`coverage\`.
  const selectors = {};
  const split = (text) => {
    const out = [];
    let depth = 0, from = 0;
    for (let i = 0; i < text.length; i++) {
      if (text[i] === '(') depth++;
      else if (text[i] === ')') depth--;
      else if (text[i] === ',' && depth === 0) { out.push(text.slice(from, i).trim()); from = i + 1; }
    }
    out.push(text.slice(from).trim());
    return out;
  };
  const plain = (sel) => {
    const s = sel.replace(/::[a-z-]+/g, '').replace(/:(hover|focus-visible|focus-within|focus|active)/g, '').replace(/:not\(\)/g, '').trim();
    return s === '' || /[>+~]$/.test(s) ? s + ' *' : s;
  };
  const sweep = (list) => {
    for (const r of list) {
      if (r.selectorText) {
        for (const sel of split(r.selectorText)) {
          if (selectors[sel]) continue;
          try { selectors[sel] = document.querySelector(plain(sel)) ? 1 : 0; } catch { selectors[sel] = 0; }
        }
      }
      if (r.cssRules) sweep(r.cssRules);
    }
  };
  for (const sheet of document.styleSheets) sweep(sheet.cssRules);
  return JSON.stringify({ props, values: [...values.keys()], styles: [...styles.keys()], els, pseudos: ['::before', '::after', '::marker', '::placeholder'], selectors });
})()`;

async function recordScene(browser, base, scene, out, shots) {
  const { send, listeners } = browser;
  const { browserContextId } = await send('Target.createBrowserContext');
  const { targetId } = await send('Target.createTarget', { url: 'about:blank', browserContextId });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  const s = (method, params) => send(method, params, sessionId);
  const errors = [];
  const listener = (msg) => {
    if (msg.sessionId !== sessionId) return;
    if (msg.method === 'Runtime.exceptionThrown') errors.push(msg.params.exceptionDetails.exception?.description?.split('\n')[0] || msg.params.exceptionDetails.text);
  };
  listeners.add(listener);
  const run = async (expression) => {
    const r = await s('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
    return r.result.value;
  };
  const settle = async (least = 1200, most = 30000) => {
    const t = Date.now();
    let last = '';
    let same = 0;
    while (Date.now() - t < most) {
      await new Promise((r) => setTimeout(r, 300));
      const now = await run(signature);
      same = now === last && now.endsWith(':loaded') ? same + 1 : 0;
      last = now;
      if (same >= 4 && Date.now() - t >= least) return true;
    }
    return false;
  };
  // Where on an element the pointer goes: its middle, or `dx` from its left (for one whose
  // middle is something else's), or `fx`, `fy` of the way across and down it.
  const point = (selector, step) =>
    run(`(async () => { const e = await sd.waitFor(${JSON.stringify(selector)}); e.scrollIntoView({ block: 'center' }); await sd.sleep(50); const r = e.getBoundingClientRect(); return [r.left + ${step.fx != null ? `r.width * ${step.fx}` : `Math.min(r.width / 2, ${step.dx ?? 1e9})`}, r.top + r.height * ${step.fy ?? 0.5}]; })()`);
  try {
    const { size } = setupFor(scene, base.data);
    await s('Page.enable');
    await s('Runtime.enable');
    await s('Emulation.setDeviceMetricsOverride', { width: size[0], height: size[1], deviceScaleFactor: 1, mobile: false });
    await s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: scene.scheme }] });
    await s('Page.navigate', { url: `${base.url}/s/${encodeURIComponent(scene.name)}/preview.html` });
    await run(`new Promise((r) => (document.readyState === 'complete' ? r() : addEventListener('load', r)))`);
    await run(helpers);
    let settled = await settle();
    for (const step of scene.steps || []) {
      if (typeof step === 'string') await run(`(async () => { ${step} })()`);
      else if (step.click || step.dblclick) {
        // The real pointer, pressed and let go: for what takes the pointer for itself (the canvas).
        const at = await point(step.click || step.dblclick, step);
        await s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
        for (let n = 1; n <= (step.dblclick ? 2 : 1); n++) {
          await s('Input.dispatchMouseEvent', { type: 'mousePressed', x: at[0], y: at[1], button: 'left', buttons: 1, clickCount: n });
          await s('Input.dispatchMouseEvent', { type: 'mouseReleased', x: at[0], y: at[1], button: 'left', clickCount: n });
        }
      } else if (step.hover || step.press) {
        // The real pointer, so :hover (and :active, held down) is the browser's own.
        const at = await point(step.hover || step.press, step);
        await s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
        if (step.press) await s('Input.dispatchMouseEvent', { type: 'mousePressed', x: at[0], y: at[1], button: 'left', clickCount: 1 });
      } else if (step.drag) {
        // Pressed, moved and still held when the page is recorded: what a drag shows while it lasts.
        const at = await point(step.drag, step);
        await s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
        await s('Input.dispatchMouseEvent', { type: 'mousePressed', x: at[0], y: at[1], button: 'left', buttons: 1, clickCount: 1 });
        for (let i = 1; i <= 8; i++) {
          await s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0] + (step.by[0] * i) / 8, y: at[1] + (step.by[1] * i) / 8, button: 'left', buttons: 1 });
          await new Promise((r) => setTimeout(r, 40));
        }
      } else if (step.tab) {
        // The real Tab key, so focus is :focus-visible.
        for (let i = 0; i < step.tab; i++) {
          await s('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Tab', code: 'Tab', windowsVirtualKeyCode: 9 });
          await s('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Tab', code: 'Tab', windowsVirtualKeyCode: 9 });
        }
      }
      settled = await settle(600);
    }
    // What the scene is there to show has to be on the page, however long the Mac takes.
    if (scene.wait) {
      await run(`sd.waitFor(${JSON.stringify(scene.wait)}, 30000).then(() => true)`);
      settled = await settle(600);
    }
    // What depends on when things happened (where a list is scrolled to), put one way.
    await run(`(async () => { ${steady} })()`);
    settled = (await settle(600)) && settled;
    const page = JSON.parse(await run(record));
    page.scene = scene.name;
    page.settled = settled;
    page.errors = errors;
    writeFileSync(join(out, `${scene.name}.json.gz`), gzipSync(JSON.stringify(page)));
    if (shots) {
      const shot = await s('Page.captureScreenshot', { format: 'png' });
      writeFileSync(join(out, `${scene.name}.png`), Buffer.from(shot.data, 'base64'));
    }
    return { elements: page.els.length, settled, errors };
  } finally {
    listeners.delete(listener);
    await send('Target.closeTarget', { targetId }).catch(() => {});
    await send('Target.disposeBrowserContext', { browserContextId }).catch(() => {});
  }
}

async function recordAll(out) {
  const data = resolve(flag('data', ''));
  const dist = resolve(flag('dist', join(root, 'dist')));
  if (!existsSync(join(data, 'board.json'))) throw new Error(`${data} has no board.json: run \`style-diff.mjs data <dir>\` first`);
  if (!existsSync(join(dist, 'index.html'))) throw new Error(`${dist} has no index.html: run \`npm run build\` first`);
  const only = (flag('only', '') || '').split(',').filter(Boolean);
  const board = JSON.parse(readFileSync(join(data, 'board.json'), 'utf8'));
  const scenes = allScenes(board).filter((s) => only.length === 0 || only.some((w) => s.name.includes(w)));
  mkdirSync(out, { recursive: true });
  const server = await serve(dist, data, scenes);
  const base = { url: `http://127.0.0.1:${server.address().port}`, data };
  const browser = await launch().catch(() => launch());
  let failed = 0;
  try {
    const queue = [...scenes];
    const worker = async () => {
      for (let scene = queue.shift(); scene; scene = queue.shift()) {
        // A scene that hangs or fails (Chrome can, on a busy Mac) gets a second go.
        for (let attempt = 1; ; attempt++) {
          const t = Date.now();
          try {
            const r = await Promise.race([recordScene(browser, base, scene, out, has('shots')), new Promise((_, no) => setTimeout(() => no(new Error('took over 3 minutes')), 180000))]);
            console.log(`${scene.name}: ${r.elements} elements, ${((Date.now() - t) / 1000).toFixed(1)}s${r.settled ? '' : ', still changing when recorded'}${r.errors.length ? `, page errors: ${r.errors.join(' | ')}` : ''}`);
            break;
          } catch (e) {
            if (attempt < 2) continue;
            failed++;
            console.log(`${scene.name}: FAILED: ${e.message.split('\n')[0]}`);
            break;
          }
        }
      }
    };
    await Promise.all(Array.from({ length: Number(flag('jobs', 3)) }, worker));
  } finally {
    browser.close();
    server.close();
  }
  console.log(`${scenes.length - failed} of ${scenes.length} scenes recorded in ${out}`);
  process.exit(failed ? 1 : 0);
}

// ---- comparing two recordings ----

function compare(before, after) {
  const max = Number(flag('max', 40));
  const load = (dir, name) => JSON.parse(gunzipSync(readFileSync(join(dir, name))).toString());
  const names = (dir) => readdirSync(dir).filter((f) => f.endsWith('.json.gz')).sort();
  const a = names(before);
  const b = new Set(names(after));
  let bad = 0;
  let total = 0;
  for (const name of a) {
    const scene = name.replace(/\.json\.gz$/, '');
    if (!b.has(name)) {
      console.log(`${scene}: only in ${before}`);
      bad++;
      continue;
    }
    b.delete(name);
    const x = load(before, name);
    const y = load(after, name);
    const lines = [];
    // Properties go by name: a custom property only one recording has reads as unset in the other.
    const props = [...new Set([...x.props, ...y.props])].sort();
    const rows = (p) => {
      const at = new Map(p.props.map((name, i) => [name, i]));
      return p.styles.map((s) => {
        const ids = s.split(',');
        return props.map((name) => (at.has(name) ? p.values[ids[at.get(name)]] : ''));
      });
    };
    const xs = rows(x);
    const ys = rows(y);
    const index = new Map(y.els.map((e) => [e[0], e]));
    let differing = 0;
    let missing = 0;
    for (const e of x.els) {
      const f = index.get(e[0]);
      if (!f || f[1] !== e[1]) {
        missing++;
        if (lines.length < max) lines.push(f ? `  ${e[0]}: class "${e[1]}" before, "${f[1]}" after` : `  ${e[0]} .${e[1]}: not on the page after`);
        continue;
      }
      let differs = false;
      for (let k = 2; k < e.length; k++) {
        const what = k === 2 ? '' : x.pseudos[k - 3];
        if ((e[k] < 0) !== (f[k] < 0)) {
          differs = true;
          if (lines.length < max) lines.push(`  ${e[0]} .${e[1]} ${what}: ${e[k] < 0 ? 'no box before, one after' : 'a box before, none after'}`);
          continue;
        }
        if (e[k] < 0) continue;
        const p = xs[e[k]];
        const q = ys[f[k]];
        for (let i = 0; i < p.length; i++) {
          if (p[i] === q[i]) continue;
          differs = true;
          if (lines.length < max) lines.push(`  ${e[0]} .${e[1].split(' ').join('.')}${what} { ${props[i]}: ${p[i] || '(unset)'} → ${q[i] || '(unset)'} }`);
        }
      }
      if (differs) differing++;
    }
    const extra = y.els.length - (x.els.length - missing);
    if (extra > 0) lines.push(`  ${extra} elements on the page after that weren't before`);
    const ok = differing === 0 && missing === 0 && extra <= 0 && lines.length === 0;
    total += x.els.length;
    if (!ok) bad++;
    console.log(`${scene}: ${x.els.length} elements, ${ok ? 'same' : `${differing} differ${missing ? `, ${missing} not matched` : ''}`}${x.settled && y.settled ? '' : ' (a recording was taken while the page still changed)'}`);
    for (const l of lines) console.log(l);
  }
  for (const name of b) {
    console.log(`${name.replace(/\.json\.gz$/, '')}: only in ${after}`);
    bad++;
  }
  console.log(bad ? `${bad} of ${a.length} scenes differ` : `${a.length} scenes, ${total} elements: every computed style is the same`);
  process.exit(bad ? 1 : 0);
}

// Which selectors no scene's page had an element for: what a comparison says nothing about.
function coverage(dir) {
  const seen = new Map();
  for (const name of readdirSync(dir).filter((f) => f.endsWith('.json.gz'))) {
    const page = JSON.parse(gunzipSync(readFileSync(join(dir, name))).toString());
    for (const [sel, hit] of Object.entries(page.selectors || {})) seen.set(sel, (seen.get(sel) || 0) + hit);
  }
  const missed = [...seen].filter(([, n]) => n === 0).map(([sel]) => sel);
  if (has('list')) for (const sel of missed) console.log(sel);
  console.log(`${seen.size - missed.length} of ${seen.size} selectors match an element in some scene${has('list') ? '' : ' (--list names the rest)'}`);
}

// ---- comparing two builds' stylesheets ----

// A stylesheet as its top-level rules, each as written less comments and spacing: an at-rule
// with rules inside is one of them.
function topLevel(css) {
  const out = [];
  let depth = 0;
  let from = 0;
  let quote = '';
  const text = css.replace(/\/\*[\s\S]*?\*\//g, '');
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === '\\') i++;
      else if (c === quote) quote = '';
    } else if (c === '"' || c === "'") quote = c;
    else if (c === '{') depth++;
    else if (c === '}' || (c === ';' && depth === 0)) {
      if (c === '}') depth--;
      if (depth === 0) {
        out.push(text.slice(from, i + 1).replace(/\s+/g, ' ').trim());
        from = i + 1;
      }
    }
  }
  return out.filter(Boolean);
}

// The stylesheet `npm run build` wrote into a folder, or the file named.
function builtCss(at) {
  if (statSync(at).isFile()) return readFileSync(at, 'utf8');
  const assets = join(at, 'assets');
  const files = readdirSync(assets).filter((f) => f.endsWith('.css'));
  if (files.length !== 1) throw new Error(`${assets} has ${files.length} stylesheets`);
  return readFileSync(join(assets, files[0]), 'utf8');
}

function built(before, after) {
  const a = builtCss(before);
  const b = builtCss(after);
  if (a === b) {
    console.log(`byte for byte the same (${a.length} bytes)`);
    return;
  }
  const x = topLevel(a);
  const y = topLevel(b);
  const count = (list) => list.reduce((m, r) => m.set(r, (m.get(r) || 0) + 1), new Map());
  const cx = count(x);
  const cy = count(y);
  const gone = [...cx].filter(([r, n]) => (cy.get(r) || 0) < n);
  const added = [...cy].filter(([r, n]) => (cx.get(r) || 0) < n);
  const max = Number(flag('max', 40));
  if (gone.length === 0 && added.length === 0) {
    const moved = x.filter((r, i) => y[i] !== r).length;
    console.log(moved === 0 ? `the same ${x.length} rules in the same order; only spacing or comments differ` : `the same ${x.length} rules; ${moved} sit at a different place`);
    process.exit(moved === 0 ? 0 : 1);
  }
  console.log(`${x.length} rules before, ${y.length} after: ${gone.length} only before, ${added.length} only after`);
  for (const [r] of gone.slice(0, max)) console.log(`  - ${r.slice(0, 200)}`);
  for (const [r] of added.slice(0, max)) console.log(`  + ${r.slice(0, 200)}`);
  process.exit(1);
}

// Chrome for another script: `launch()` gives {send, listeners, close}.
export { launch };

if (resolve(process.argv[1] || '') !== fileURLToPath(import.meta.url)) {
  // Imported, not run.
} else if (mode === 'data' && rest[0]) takeData(resolve(rest[0]));
else if (mode === 'built' && rest[1]) built(resolve(rest[0]), resolve(rest[1]));
else if (mode === 'coverage' && rest[0]) coverage(resolve(rest[0]));
else if (mode === 'record' && rest[0]) await recordAll(resolve(rest[0]));
else if (mode === 'compare' && rest[1]) compare(resolve(rest[0]), resolve(rest[1]));
else {
  console.error('usage: style-diff.mjs data <dir> | record <out> --data <dir> [--dist dist] [--only words] [--shots] [--jobs n] | compare <before> <after> | built <dist before> <dist after> | coverage <dir> [--list]');
  process.exit(2);
}
