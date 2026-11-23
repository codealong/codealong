#!/usr/bin/env node
// Runs Codealong's browser checks over persistent headless Chrome instances via CDP (Node 24
// built-in http + WebSocket, zero npm dependencies). Instead of spawning a cold Chrome, a Python
// HTTP server, curl polls, and a `ps` cleanup scan for each of the ~420 checks, this serves the
// prepared UI and per-run fixture overlays from one in-memory HTTP server and runs each check in
// an isolated incognito BrowserContext on the real clock that closes the instant `done()` fires.
//
// Usage (called by scripts/checks.sh):
//   node scripts/preview/runner.mjs --dir <dir> --plan <plan.json> [--jobs <n>] [--fixture] [--bin <engine>] [--chrome <path>]
//
// Outputs one JSON line per completed run:
//   {"id":0,"status":0,"result":"PASS 7/7","extraLines":[]}

import { execFile, execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { homedir, tmpdir } from 'node:os';
import { extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

export const FIXTURE_NAME = 'the made-up Mac';
export const FIXTURE_MARK = `${FIXTURE_NAME}, as its engine named it`;
export const MAC_USER_AGENT =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36';

const FIXTURE_FILES = {
  'todos-fixture.json': 'PREVIEW_TODOS',
  'schedules-fixture.json': 'PREVIEW_SCHEDULES',
  'insights-fixture.json': 'PREVIEW_INSIGHTS',
  'route-fixture.json': 'PREVIEW_ROUTE',
  'canvas-fixture.json': 'PREVIEW_CANVAS',
  'boards-fixture.json': 'PREVIEW_BOARDS',
  'usage-fixture.json': 'PREVIEW_USAGE',
  'devices-fixture.json': 'PREVIEW_DEVICES',
  'notices-fixture.json': 'PREVIEW_NOTICES',
  'training-fixture.json': 'PREVIEW_TRAINING',
  'resets-fixture.json': 'PREVIEW_RESETS',
  'mcp-fixture.json': 'PREVIEW_MCP',
  'skills-fixture.json': 'PREVIEW_SKILLS',
  'tokens-fixture.json': 'PREVIEW_TOKENS',
};

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.mjs': 'application/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.gif': 'image/gif',
  '.woff2': 'font/woff2',
  '.woff': 'font/woff',
  '.ttf': 'font/ttf',
  '.mp4': 'video/mp4',
  '.webm': 'video/webm',
  '.wav': 'audio/wav',
  '.mp3': 'audio/mpeg',
  '.txt': 'text/plain; charset=utf-8',
};

const BACKDROP = 'linear-gradient(135deg,rgb(93,111,134),rgb(169,182,196))';
const GLASS_STYLE =
  'html.is-glass{background:linear-gradient(135deg,color-mix(in srgb,var(--surface) 82%,rgb(120,150,60)),color-mix(in srgb,var(--surface) 82%,rgb(60,120,200)))!important}' +
  "html.is-glass.is-quick .ca-quick::before{content:'';position:absolute;inset:0;z-index:-1;border-radius:inherit;background:linear-gradient(135deg,color-mix(in srgb,var(--surface) 82%,rgb(120,150,60)),color-mix(in srgb,var(--surface) 82%,rgb(60,120,200)))}";

/** Checks that `$dir` holds the expected Mac's snapshots, matching preview.sh's PREVIEW_PREPARE=skip guard. */
export function assertSnapshotDir(dir, fixture) {
  if (!existsSync(join(dir, 'board.json'))) {
    throw new Error(`${dir} has no build and snapshots: run with PREVIEW_PREPARE=only first.`);
  }
  const want = fixture ? FIXTURE_MARK : 'this Mac';
  const markPath = join(dir, 'snapshot-of');
  const actual = existsSync(markPath) ? readFileSync(markPath, 'utf8').trim() : 'this Mac';
  if (actual !== want) {
    throw new Error(
      `${dir} holds the snapshots of ${actual}, not of ${want}: run with PREVIEW_PREPARE=only first, with PREVIEW_FIXTURE=1 for the made-up Mac and without it for this one.`,
    );
  }
}

/** Verifies the engine supports `--snapshot --fixture name`, matching preview.sh's old-engine guard. */
export function assertFixtureEngine(bin) {
  if (!bin) return;
  let answered = '';
  try {
    answered = execFileSync(bin, ['--snapshot', '--fixture', 'name'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
  } catch {
    answered = '';
  }
  if (answered !== FIXTURE_NAME) {
    throw new Error(
      `The engine at ${bin} has no made-up Mac: it was built before it had one, or isn't a debug build. scripts/engine.sh builds this checkout's, which PREVIEW_BIN is used in place of.`,
    );
  }
}

/** Whether a check should emulate a macOS user-agent/navigator.platform (on non-Mac hosts unless another platform is asked for). */
export function wantsMacEmulation(vars = {}, { hostPlatform = process.platform } = {}) {
  if (hostPlatform === 'darwin') return false;
  const explicit = vars.PREVIEW_PLATFORM || '';
  return !explicit || explicit === 'mac';
}

/** Builds the view's default size and injected preview HTML matching scripts/preview.sh. */
export function buildPreviewPage(indexHtml, run) {
  const view = run.view || 'sessions';
  const project = run.project || '';
  const vars = run.vars || {};
  const parts = [
    `localStorage.setItem('codealong.view',${JSON.stringify(view)});`,
    `localStorage.setItem('codealong.age','all');`,
    `localStorage.setItem('codealong.sidebarCollapsed','0');`,
  ];
  if (project) parts.push(`localStorage.setItem('codealong.project',${JSON.stringify(project)});`);
  if (vars.PREVIEW_GROUP) {
    const g = JSON.stringify(vars.PREVIEW_GROUP);
    parts.push(`localStorage.setItem('codealong.sessionGroup',${g});localStorage.setItem('codealong.prGroup',${g});localStorage.setItem('codealong.todoGroup',${g});localStorage.setItem('codealong.scheduleGroup',${g});`);
  }
  if (vars.PREVIEW_LOADING) parts.push('window.__PREVIEW_LOADING__=true;');
  if (vars.PREVIEW_EMBLEMS) parts.push(`window.__PREVIEW_EMBLEMS__=${JSON.stringify(vars.PREVIEW_EMBLEMS)};`);
  if (vars.PREVIEW_ANALYTICS) parts.push('window.__PREVIEW_ANALYTICS__=true;');
  if (vars.PREVIEW_GLASS) parts.push('window.__PREVIEW_GLASS__=true;');
  if (vars.PREVIEW_KEYS === 'held') parts.push('window.__PREVIEW_KEYS_HELD__=true;');
  if (vars.PREVIEW_BOARDS_OFF) {
    parts.push("window.__PREVIEW_BOARDS_OFF__=true;localStorage.setItem('codealong.boardsOn','0');");
  } else {
    parts.push("localStorage.setItem('codealong.boardsOn','1');");
  }
  if (vars.PREVIEW_PLATFORM) parts.push(`window.__CODEALONG_PLATFORM__=${JSON.stringify(vars.PREVIEW_PLATFORM)};`);
  if (vars.PREVIEW_VOICE) {
    parts.push('window.__PREVIEW_VOICE__=true;');
    if (vars.PREVIEW_VOICE !== '1') parts.push(`window.__LISTENS__=${JSON.stringify(vars.PREVIEW_VOICE)};`);
  }

  let size = vars.PREVIEW_SIZE || '1440,900';
  const styles = [];
  const onDesktop = (cls, over = '') => styles.push(`html.${cls}{background:${over}${BACKDROP}!important}`);
  if (vars.PREVIEW_GLASS) styles.push(GLASS_STYLE);

  let extraHeadScripts = '';
  if (view === 'bar') {
    size = vars.PREVIEW_SIZE || '1200,64';
    const w = Number(size.split(',')[0]);
    parts.push(`window.__CODEALONG_BAR__=true;Object.defineProperty(screen,'availWidth',{value:${w}});`);
    onDesktop('is-bar');
  } else if (view === 'notch') {
    size = vars.PREVIEW_SIZE || '1512,320';
    const w = Number(size.split(',')[0]);
    const drawn = vars.PREVIEW_NOTCH === 'drawn';
    const notch = drawn ? { x: Math.floor(w / 2), top: 1440, width: 180, height: 25, real: false } : { x: Math.floor(w / 2), top: 982, width: 185, height: 32, real: true };
    const menu = drawn ? 25 : 32;
    const screenH = drawn ? 1440 : 982;
    parts.push(
      `window.__CODEALONG_BAR__=true;window.__BAR_MODE__=${JSON.stringify({ mode: 'notch', notch })};Object.defineProperty(screen,'availWidth',{value:${w}});Object.defineProperty(screen,'height',{value:${screenH}});`,
    );
    onDesktop('is-bar', `linear-gradient(rgba(236,236,238,0.82) 0 ${menu}px,transparent ${menu}px),`);
  } else if (view === 'quick') {
    size = vars.PREVIEW_SIZE || '760,640';
    parts.push('window.__CODEALONG_QUICK__=true;');
    onDesktop('is-quick');
  } else if (view === 'waiting') {
    size = vars.PREVIEW_SIZE || '820,760';
    parts.push('window.__CODEALONG_WAITING__=true;');
    if (vars.PREVIEW_TRANSCRIPT) parts.push(`window.__WAITING_ASKED__=${JSON.stringify({ key: vars.PREVIEW_TRANSCRIPT })};`);
  } else if (view === 'desktop') {
    size = vars.PREVIEW_SIZE || '360,300';
    parts.push(`window.__CODEALONG_DESKTOP__=true;window.__CODEALONG_PIN__=${JSON.stringify(vars.PREVIEW_PIN || 'session')};`);
    onDesktop('is-desktop');
  } else if (view === 'lineage') {
    size = vars.PREVIEW_SIZE || '1440,420';
    extraHeadScripts += '<script src="./lineage-desk.js"></script>';
  } else if (view === 'peek') {
    size = vars.PREVIEW_SIZE || '520,640';
    parts.push(`localStorage.setItem('codealong.preview.peek',${JSON.stringify(vars.PREVIEW_TRANSCRIPT || '')});window.__CODEALONG_PEEK__=true;`);
    onDesktop('is-peek');
  } else if (view === 'kept') {
    size = vars.PREVIEW_SIZE || '640,720';
    parts.push(`localStorage.setItem('codealong.preview.peek',${JSON.stringify(vars.PREVIEW_TRANSCRIPT || '')});window.__CODEALONG_KEPT__=true;`);
    onDesktop('is-peek');
  } else if (view === 'side') {
    size = vars.PREVIEW_SIZE || '520,640';
    parts.push(`localStorage.setItem('codealong.preview.peek',${JSON.stringify(vars.PREVIEW_TRANSCRIPT || '')});window.__CODEALONG_KEPT__=true;window.__CODEALONG_SIDE__=true;`);
    onDesktop('is-peek');
  } else if (view === 'terminal') {
    size = vars.PREVIEW_SIZE || '720,460';
    parts.push('window.__CODEALONG_KEPT__=true;window.__CODEALONG_TERMINAL__=true;');
    onDesktop('is-peek');
  }

  parts.push(`window.__PREVIEW_SIZE__=${JSON.stringify(size)};`);
  const clock = vars.PREVIEW_DATE ? `<script>window.__PREVIEW_DATE__=${JSON.stringify(vars.PREVIEW_DATE)};</script><script src="./clock.js"></script>` : '';
  const styleTag = styles.length ? `<style>${styles.join('')}</style>` : '';
  const headInject = `${clock}<script src="./fake-tauri.js"></script><script>${parts.join('')}</script>${styleTag}${extraHeadScripts}`;

  let tail = '<script src="./prelude.js"></script>';
  if (vars.PREVIEW_INIT) tail += '<script src="./init.js"></script>';
  if (run.check) tail += '<script src="./check.js"></script>';

  const html = indexHtml.replace('<head>', `<head>${headInject}`).replace('</body>', `${tail}</body>`);
  const [width, height] = size.split(',').map((n) => Number(n));
  return { html, width, height };
}

/** Formats a page's final title into preview.sh's exit status, first line, and extra lines. */
export function formatReport(title, checkPath = '') {
  const clean = (title || '').replace(/\s+/g, ' ').trim();
  if (checkPath.endsWith('-bench.js')) {
    return { status: 0, result: clean.replace(/^done:\s*/, ''), extraLines: [] };
  }
  let raw;
  if (clean.startsWith('done:')) {
    raw = clean.slice('done:'.length).replace(/^ /, '');
  } else if (!clean) {
    raw = 'FAIL 0/0 no result: Chrome never showed the page';
  } else {
    raw = `FAIL 0/0 no result: the check never said done (the page's title is "${clean}")`;
  }
  const lines = raw.split(' ## ');
  const result = lines[0] || '';
  const extraLines = lines.slice(1);
  if (!clean) return { status: 3, result, extraLines };
  if (result.startsWith('PASS ')) return { status: 0, result, extraLines };
  return { status: 1, result, extraLines };
}

/** Finds Chrome on macOS or Linux when PREVIEW_CHROME isn't given. */
export function findChrome(explicit) {
  if (explicit) return explicit;
  if (process.platform === 'darwin') {
    const mac = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
    if (existsSync(mac)) return mac;
  }
  const candidates = ['google-chrome', 'google-chrome-stable', 'chromium', 'chromium-browser'];
  for (const name of candidates) {
    try {
      const found = execFileSync('which', [name], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
      if (found) return found;
    } catch {
      // Not on PATH.
    }
  }
  return '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
}

/** Pure in-memory static + per-run request resolver (testable without listening on any socket). */
export function createPreviewHandler({ dir, repo, fixture, bin, runs }) {
  const absDir = resolve(dir);
  const absRepo = resolve(repo);
  assertSnapshotDir(absDir, fixture);
  if (fixture && bin) assertFixtureEngine(bin);

  const fileCache = new Map();
  const readCached = (absPath) => {
    if (fileCache.has(absPath)) return fileCache.get(absPath);
    if (!existsSync(absPath) || !statSync(absPath).isFile()) {
      fileCache.set(absPath, null);
      return null;
    }
    const buf = readFileSync(absPath);
    fileCache.set(absPath, buf);
    return buf;
  };

  const indexHtml = readFileSync(join(absDir, 'index.html'), 'utf8');
  const runsById = new Map();
  const transcriptCache = new Map();

  for (const run of runs) {
    const page = buildPreviewPage(indexHtml, run);
    runsById.set(String(run.id), { run, page });
  }

  const getTranscript = (vars) => {
    const key = vars.PREVIEW_TRANSCRIPT;
    if (!key || !bin) return Promise.resolve(null);
    const before = vars.PREVIEW_BEFORE || '';
    const cacheKey = `${key}\0${before}`;
    if (transcriptCache.has(cacheKey)) return transcriptCache.get(cacheKey);
    const snap = fixture ? ['--snapshot', '--fixture'] : ['--snapshot'];
    const mode = fixture ? '' : '--';
    const tArgs = [...snap, `${mode}transcript`, key, ...(before ? ['--before', before] : [])];
    const pArgs = [...snap, `${mode}pending`, key];
    // Run asynchronously so slow real-data transcripts never block Node's event loop or trip other workers' CDP timeouts.
    const promise = (async () => {
      let transcript = Buffer.alloc(0);
      let pending = Buffer.alloc(0);
      const sentFiles = new Set();
      try {
        const res = await execFileAsync(bin, tArgs, { encoding: 'buffer', maxBuffer: 64 * 1024 * 1024 });
        transcript = res.stdout;
        const parsed = JSON.parse(transcript.toString('utf8'));
        for (const item of parsed.items || []) {
          for (const f of item.sent?.files || []) sentFiles.add(f);
        }
      } catch {
        // Empty transcript, matching preview.sh.
      }
      try {
        const res = await execFileAsync(bin, pArgs, { encoding: 'buffer', maxBuffer: 16 * 1024 * 1024 });
        pending = res.stdout;
      } catch {
        // Empty pending.
      }
      return { transcript, pending, keyJson: Buffer.from(JSON.stringify({ key })), sentFiles };
    })();
    transcriptCache.set(cacheKey, promise);
    return promise;
  };

  const defaultImages = fixture ? null : join(homedir(), 'Library/Application Support/Codealong/images');

  const resolveRunPath = async (runEntry, rawRelPath) => {
    const { run, page } = runEntry;
    const vars = run.vars || {};
    let relPath;
    try {
      relPath = decodeURIComponent(rawRelPath);
    } catch {
      return null;
    }

    if (relPath === 'preview.html') return { body: Buffer.from(page.html), type: MIME['.html'] };
    if (relPath === 'fake-tauri.js') return { path: join(absRepo, 'scripts/preview/fake-tauri.js') };
    if (relPath === 'prelude.js') return { path: join(absRepo, 'scripts/preview/prelude.js') };
    if (relPath === 'clock.js' && vars.PREVIEW_DATE) return { path: join(absRepo, 'scripts/preview/clock.js') };
    if (relPath === 'lineage-desk.js' && run.view === 'lineage') return { path: join(absRepo, 'scripts/preview/lineage-desk.js') };
    if (relPath === 'check.js' && run.check) return { path: resolve(absRepo, run.check) };
    if (relPath === 'init.js' && vars.PREVIEW_INIT) return { path: resolve(absRepo, vars.PREVIEW_INIT) };
    if (relPath === 'found.json') return { path: vars.PREVIEW_FOUND ? resolve(absRepo, vars.PREVIEW_FOUND) : join(absDir, 'found-here.json') };
    if (relPath === 'setup.json') return { path: vars.PREVIEW_SETUP ? resolve(absRepo, vars.PREVIEW_SETUP) : join(absDir, 'setup-here.json') };

    if (relPath in FIXTURE_FILES) {
      const val = vars[FIXTURE_FILES[relPath]];
      return val ? { path: resolve(absRepo, val) } : null;
    }

    if (relPath === 'transcript.json' || relPath === 'pending.json' || relPath === 'transcript-key.json') {
      const tr = await getTranscript(vars);
      if (!tr) return null;
      if (relPath === 'transcript.json') return { body: tr.transcript, type: MIME['.json'] };
      if (relPath === 'pending.json') return { body: tr.pending, type: MIME['.json'] };
      return { body: tr.keyJson, type: MIME['.json'] };
    }

    if (relPath.startsWith('files/')) {
      const tr = await getTranscript(vars);
      if (!tr) return null;
      const tail = relPath.slice('files/'.length);
      const candidate = tail.startsWith('/') ? tail : `/${tail}`;
      if (tr.sentFiles.has(candidate)) return { path: candidate };
      if (tr.sentFiles.has(tail)) return { path: tail };
      return null;
    }

    if (relPath.startsWith('images/')) {
      const name = relPath.slice('images/'.length);
      if (name.includes('..') || name.startsWith('/')) return null;
      if (vars.PREVIEW_EMBLEMS === 'drawn') {
        const emblem = join(absRepo, 'scripts/preview/emblems', name);
        if (existsSync(emblem)) return { path: emblem };
      }
      const imgDir = vars.PREVIEW_IMAGES ? resolve(absRepo, vars.PREVIEW_IMAGES) : defaultImages;
      if (imgDir) {
        const img = resolve(imgDir, name);
        if ((img.startsWith(imgDir + '/') || img === imgDir) && existsSync(img)) return { path: img };
      }
      return null;
    }

    // Shared prepared assets & snapshots from $dir (board.json, bar-feed.json, assets/*, etc.).
    const safe = resolve(absDir, relPath);
    if (!safe.startsWith(absDir + '/') && safe !== absDir) return null;
    return { path: safe };
  };

  const servePath = async (urlPath, method = 'GET') => {
    const clean = (urlPath || '/').split('?')[0];
    const m = /^\/run\/([^/]+)\/(.*)$/.exec(clean);
    if (!m) return { status: 404 };
    const entry = runsById.get(m[1]);
    if (!entry) return { status: 404 };
    const resolved = await resolveRunPath(entry, m[2]);
    if (!resolved) return { status: 404 };
    let body = resolved.body;
    let type = resolved.type;
    if (!body && resolved.path) {
      body = readCached(resolved.path);
      if (!body) return { status: 404 };
      type = MIME[extname(resolved.path).toLowerCase()] || 'application/octet-stream';
    }
    return {
      status: 200,
      type: type || 'application/octet-stream',
      body: method === 'HEAD' ? Buffer.alloc(0) : body,
      length: body.length,
    };
  };

  const handle = (req, res) => {
    servePath(req.url, req.method)
      .then((out) => {
        if (out.status !== 200) {
          res.writeHead(out.status);
          res.end();
          return;
        }
        res.writeHead(200, { 'Content-Type': out.type, 'Content-Length': out.length, 'Cache-Control': 'no-store' });
        res.end(out.body);
      })
      .catch(() => {
        if (!res.headersSent) res.writeHead(500);
        res.end();
      });
  };

  return { runsById, resolveRunPath, servePath, handle };
}

/** Starts a localhost HTTP server over `createPreviewHandler`. */
export function startPreviewServer(opts) {
  const handler = createPreviewHandler(opts);
  const server = createServer(handler.handle);
  return new Promise((res, rej) => {
    server.on('error', rej);
    server.listen(0, '127.0.0.1', () => {
      const addr = server.address();
      res({
        port: addr.port,
        runsById: handler.runsById,
        close: () => new Promise((done) => server.close(() => done())),
      });
    });
  });
}

/** Minimal Chrome DevTools Protocol client over Node 24's built-in WebSocket, with per-call timeouts. */
class CdpClient {
  constructor(ws) {
    this.ws = ws;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    this.closed = false;

    ws.addEventListener('message', (ev) => {
      let msg;
      try {
        msg = JSON.parse(String(ev.data));
      } catch {
        return;
      }
      if (msg.id) {
        const p = this.pending.get(msg.id);
        if (p) {
          this.pending.delete(msg.id);
          clearTimeout(p.timer);
          if (msg.error) p.reject(new Error(`${msg.error.message} (${msg.error.code})`));
          else p.resolve(msg.result || {});
        }
      } else if (msg.method) {
        const key = `${msg.sessionId || ''}:${msg.method}`;
        const set = this.listeners.get(key);
        if (set) {
          for (const fn of [...set]) fn(msg.params || {});
        }
      }
    });

    const failAll = (reason) => {
      this.closed = true;
      for (const p of this.pending.values()) {
        clearTimeout(p.timer);
        p.reject(new Error(reason));
      }
      this.pending.clear();
    };
    ws.addEventListener('close', () => failAll('CDP WebSocket closed'));
    ws.addEventListener('error', () => failAll('CDP WebSocket error'));
  }

  send(method, params = {}, sessionId = undefined, timeoutMs = 15_000) {
    if (this.closed) return Promise.reject(new Error('CDP client is closed'));
    const id = this.nextId++;
    const payload = sessionId ? { id, method, params, sessionId } : { id, method, params };
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.pending.delete(id)) {
          reject(new Error(`CDP ${method} timed out after ${timeoutMs} ms`));
        }
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      try {
        this.ws.send(JSON.stringify(payload));
      } catch (e) {
        this.pending.delete(id);
        clearTimeout(timer);
        reject(e);
      }
    });
  }

  on(sessionId, method, fn) {
    const key = `${sessionId || ''}:${method}`;
    let set = this.listeners.get(key);
    if (!set) {
      set = new Set();
      this.listeners.set(key, set);
    }
    set.add(fn);
    return () => {
      set.delete(fn);
      if (set.size === 0) this.listeners.delete(key);
    };
  }

  close() {
    this.closed = true;
    for (const p of this.pending.values()) {
      clearTimeout(p.timer);
      p.reject(new Error('CDP client closed'));
    }
    this.pending.clear();
    try {
      this.ws.close();
    } catch {
      // Already closed.
    }
  }
}

const FATAL_SPAWN_CODES = new Set(['ENOENT', 'EACCES', 'EPERM', 'EISDIR', 'ENOTDIR']);
const isFatalSpawnError = (err) => Boolean(err && typeof err === 'object' && FATAL_SPAWN_CODES.has(err.code));

/** One persistent headless Chrome process with isolated process-group cleanup. */
export class ChromeWorker {
  constructor(
    chromePath,
    {
      fixture = false,
      startTimeoutMs = 30_000,
      startAttempts = 3,
      retryDelayMs = 500,
      wsTimeoutMs = 15_000,
      spawnFn = spawn,
      killFn = (pid, sig) => process.kill(-pid, sig),
      wsFactory = (url) => new WebSocket(url),
    } = {},
  ) {
    this.chromePath = chromePath;
    this.fixture = fixture;
    this.startTimeoutMs = startTimeoutMs;
    this.startAttempts = Math.max(1, startAttempts);
    this.retryDelayMs = retryDelayMs;
    this.wsTimeoutMs = wsTimeoutMs;
    this.spawnFn = spawnFn;
    this.killFn = killFn;
    this.wsFactory = wsFactory;
    this.proc = null;
    this.profile = null;
    this.cdp = null;
  }

  isReady() {
    return Boolean(this.cdp && !this.cdp.closed);
  }

  async startOnce() {
    this.profile = mkdtempSync(join(tmpdir(), 'ca-cdp-'));
    const args = [
      '--headless=new',
      '--disable-features=MacAppCodeSignClone',
      '--disable-gpu',
      '--enable-unsafe-swiftshader',
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-background-timer-throttling',
      '--disable-backgrounding-occluded-windows',
      '--disable-renderer-backgrounding',
      '--remote-debugging-port=0',
      `--user-data-dir=${this.profile}`,
      ...(process.platform === 'linux' ? ['--no-sandbox', '--disable-dev-shm-usage'] : []),
      'about:blank',
    ];
    const proc = this.spawnFn(this.chromePath, args, { detached: true, stdio: ['ignore', 'ignore', 'pipe'] });
    this.proc = proc;
    const wsUrl = await new Promise((resolveUrl, rejectUrl) => {
      let buf = '';
      let settled = false;
      const cleanupListeners = () => {
        clearTimeout(timer);
        proc.off('exit', onExit);
        proc.off('error', onError);
        proc.stderr?.off('data', onStderr);
      };
      const done = (fn, arg) => {
        if (settled) return;
        settled = true;
        cleanupListeners();
        fn(arg);
      };
      const timer = setTimeout(() => {
        const tail = buf.trim().slice(-400);
        const secs = Math.round(this.startTimeoutMs / 1000);
        done(rejectUrl, new Error(`Chrome did not print DevTools WebSocket URL within ${secs}s${tail ? `: ${tail}` : ''}`));
      }, this.startTimeoutMs);
      const onExit = (code, signal) => done(rejectUrl, new Error(`Chrome exited early (${code ?? signal}): ${buf.slice(-400)}`));
      const onError = (err) => done(rejectUrl, err);
      const onStderr = (chunk) => {
        buf += chunk.toString('utf8');
        const m = /DevTools listening on (ws:\/\/[^\s]+)/.exec(buf);
        if (m) done(resolveUrl, m[1]);
      };
      proc.once('exit', onExit);
      proc.once('error', onError);
      proc.stderr?.on('data', onStderr);
    });

    // Drain stderr after startup so Chrome never blocks on a full pipe buffer.
    proc.stderr?.resume?.();

    const ws = this.wsFactory(wsUrl);
    await new Promise((res, rej) => {
      let settled = false;
      const finish = (fn, arg) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        ws.removeEventListener?.('open', onOpen);
        ws.removeEventListener?.('error', onError);
        fn(arg);
      };
      const onOpen = () => finish(res);
      const onError = () => {
        try {
          ws.close?.();
        } catch {}
        finish(rej, new Error('CDP WebSocket failed to connect'));
      };
      const timer = setTimeout(() => {
        try {
          ws.close?.();
        } catch {}
        finish(rej, new Error('CDP WebSocket open timeout'));
      }, this.wsTimeoutMs);
      ws.addEventListener('open', onOpen);
      ws.addEventListener('error', onError);
    });
    this.cdp = new CdpClient(ws);
  }

  async start() {
    let lastErr;
    for (let attempt = 1; attempt <= this.startAttempts; attempt++) {
      try {
        await this.startOnce();
        return;
      } catch (err) {
        lastErr = err;
        await this.stop();
        if (isFatalSpawnError(err) || attempt === this.startAttempts) throw err;
        if (this.retryDelayMs > 0) await new Promise((r) => setTimeout(r, this.retryDelayMs));
      }
    }
    throw lastErr;
  }

  async stop() {
    if (this.cdp) {
      this.cdp.close();
      this.cdp = null;
    }
    const proc = this.proc;
    this.proc = null;
    if (proc && proc.pid) {
      try {
        this.killFn(proc.pid, 'SIGTERM');
      } catch {
        // Process group already gone.
      }
      await new Promise((res) => {
        let done = false;
        let killTimer = null;
        const finish = () => {
          if (done) return;
          done = true;
          if (killTimer) clearTimeout(killTimer);
          proc.off?.('exit', finish);
          res();
        };
        if (proc.exitCode !== null || proc.signalCode !== null) {
          finish();
          return;
        }
        proc.once('exit', finish);
        killTimer = setTimeout(() => {
          try {
            this.killFn(proc.pid, 'SIGKILL');
          } catch {
            // Already gone.
          }
          finish();
        }, 2000);
      });
    }
    if (this.profile) {
      try {
        rmSync(this.profile, { recursive: true, force: true });
      } catch {
        // Best-effort profile cleanup.
      }
      this.profile = null;
    }
  }

  async restart() {
    await this.stop();
    await this.start();
  }

  async runCheck(run, page, port) {
    if (!this.isReady()) {
      try {
        await this.restart();
      } catch (err) {
        return {
          status: 3,
          result: 'FAIL 0/0 no result: Chrome never showed the page',
          extraLines: [],
          err: err instanceof Error ? err.message : String(err),
        };
      }
    }
    const vars = run.vars || {};
    const waitMs = vars.PREVIEW_WAIT ? Number(vars.PREVIEW_WAIT) : 30_000;
    // Matches checks.sh's outer timeout (PREVIEW_WAIT + 90s, or 150s when unset).
    const hardLimitMs = vars.PREVIEW_WAIT ? waitMs + 90_000 : 150_000;
    const url = `http://127.0.0.1:${port}/run/${run.id}/preview.html`;

    let browserContextId = null;
    let sessionId = null;
    let needsRestart = false;
    const unsubs = [];

    const runBody = async () => {
      const ctx = await this.cdp.send('Target.createBrowserContext', { disposeOnDetach: true });
      browserContextId = ctx.browserContextId;
      const created = await this.cdp.send('Target.createTarget', {
        url: 'about:blank',
        browserContextId,
        width: page.width,
        height: page.height,
      });
      const attached = await this.cdp.send('Target.attachToTarget', { targetId: created.targetId, flatten: true });
      sessionId = attached.sessionId;

      await this.cdp.send('Page.enable', {}, sessionId);
      await this.cdp.send('Runtime.enable', {}, sessionId);
      await this.cdp.send(
        'Emulation.setDeviceMetricsOverride',
        { width: page.width, height: page.height, deviceScaleFactor: 1, mobile: false },
        sessionId,
      );
      if (wantsMacEmulation(vars)) {
        await this.cdp.send(
          'Emulation.setUserAgentOverride',
          { userAgent: MAC_USER_AGENT, platform: 'MacIntel' },
          sessionId,
        );
      }
      if (vars.PREVIEW_SCHEME === 'dark' || vars.PREVIEW_SCHEME === 'light') {
        await this.cdp.send(
          'Emulation.setEmulatedMedia',
          { features: [{ name: 'prefers-color-scheme', value: vars.PREVIEW_SCHEME }] },
          sessionId,
        );
      }
      await this.cdp.send('Runtime.addBinding', { name: '__codealongDone' }, sessionId);

      const readTitle = async () => {
        try {
          const r = await this.cdp.send('Runtime.evaluate', { expression: 'document.title', returnByValue: true }, sessionId, 5_000);
          return typeof r?.result?.value === 'string' ? r.result.value : '';
        } catch {
          return '';
        }
      };

      return new Promise((resolveOutcome, rejectOutcome) => {
        let settled = false;
        const timers = [];
        const finish = (val) => {
          if (settled) return;
          settled = true;
          for (const t of timers) clearTimeout(t);
          resolveOutcome(val);
        };
        const fail = (err) => {
          if (settled) return;
          settled = true;
          for (const t of timers) clearTimeout(t);
          rejectOutcome(err);
        };

        unsubs.push(
          this.cdp.on(sessionId, 'Runtime.bindingCalled', (params) => {
            if (params.name === '__codealongDone' && typeof params.payload === 'string') {
              finish({ title: params.payload, timedOut: false });
            }
          }),
        );

        unsubs.push(
          this.cdp.on(sessionId, 'Inspector.targetCrashed', () => fail(new Error('target crashed'))),
        );

        // Resolve immediately via __codealongDone when prelude.js's done() fires, with a 1s title
        // fallback for any script that writes document.title directly without say().
        const startedAt = Date.now();
        let sawTitle = false;
        const poll = setInterval(async () => {
          if (settled) return;
          const t = await readTitle();
          if (t) sawTitle = true;
          if (t.startsWith('done:')) {
            finish({ title: t, timedOut: false });
          } else if (!sawTitle && Date.now() - startedAt > 30_000) {
            finish({ title: '', timedOut: false });
          }
        }, 1000);
        timers.push(poll);
        timers.push(
          setTimeout(async () => {
            const t = await readTitle();
            finish({ title: t, timedOut: !t.startsWith('done:') });
          }, waitMs),
        );

        this.cdp.send('Page.navigate', { url }, sessionId).catch(fail);
      });
    };

    // Start the hard-limit timer before any CDP setup call so a hung Chrome can never hold a set.
    let hardTimer;
    const hardLimit = new Promise((res) => {
      hardTimer = setTimeout(() => res({ title: '', timedOut: true, hardLimit: true }), hardLimitMs);
    });

    try {
      const outcome = await Promise.race([runBody(), hardLimit]);
      clearTimeout(hardTimer);
      if (outcome.hardLimit) {
        needsRestart = true;
        const secs = Math.round(hardLimitMs / 1000);
        return { status: 124, result: `FAIL 0/0 stopped after ${secs} s`, extraLines: [] };
      }
      return formatReport(outcome.title, run.check);
    } catch (err) {
      clearTimeout(hardTimer);
      needsRestart = true;
      return {
        status: 3,
        result: 'FAIL 0/0 no result: Chrome never showed the page',
        extraLines: [],
        err: err instanceof Error ? err.message : String(err),
      };
    } finally {
      clearTimeout(hardTimer);
      for (const u of unsubs) u();
      if (!needsRestart && browserContextId && this.cdp && !this.cdp.closed) {
        try {
          await this.cdp.send('Target.disposeBrowserContext', { browserContextId }, undefined, 5_000);
        } catch {
          needsRestart = true;
        }
      }
      if (needsRestart) {
        await this.restart().catch(() => {});
      }
    }
  }
}

function parseCli(argv) {
  const opts = { dir: '', plan: '', jobs: 1, fixture: false, bin: '', chrome: '', repo: process.cwd() };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--dir') opts.dir = argv[++i];
    else if (a === '--plan') opts.plan = argv[++i];
    else if (a === '--jobs') opts.jobs = Math.max(1, Number(argv[++i]) || 1);
    else if (a === '--fixture') opts.fixture = true;
    else if (a === '--bin') opts.bin = argv[++i];
    else if (a === '--chrome') opts.chrome = argv[++i];
    else if (a === '--repo') opts.repo = argv[++i];
  }
  return opts;
}

/** Starts Chrome workers with a brief stagger so simultaneous cold launches don't contend,
 *  continuing with whichever workers came up and failing fast only if none could start. */
export async function startWorkers(workers, { staggerMs = 250, warn = (msg) => console.error(msg) } = {}) {
  let fatalErr = null;
  const settled = await Promise.allSettled(
    workers.map(async (worker, i) => {
      if (i > 0 && staggerMs > 0) {
        await new Promise((r) => setTimeout(r, i * staggerMs));
      }
      if (fatalErr) throw fatalErr;
      try {
        await worker.start();
        return worker;
      } catch (err) {
        if (isFatalSpawnError(err)) fatalErr = err;
        await worker.stop().catch(() => {});
        throw err;
      }
    }),
  );

  const ready = [];
  const errors = [];
  for (const outcome of settled) {
    if (outcome.status === 'fulfilled') ready.push(outcome.value);
    else errors.push(outcome.reason);
  }
  if (ready.length === 0) {
    throw errors[0] || new Error('No Chrome workers could start');
  }
  if (errors.length > 0) {
    const why = errors[0] instanceof Error ? errors[0].message : String(errors[0]);
    warn(`Warning: ${errors.length}/${workers.length} Chrome worker(s) failed to start (${why}); continuing with ${ready.length}`);
  }
  return ready;
}

/** Runs queued checks across healthy workers, retiring a dead worker and handing its check to
 *  surviving workers instead of aborting the whole batch. */
export async function runWorkerPool(
  workers,
  runs,
  runOne,
  { onResult, isShuttingDown = () => false, warn = (msg) => console.error(msg) },
) {
  const queue = [...runs];
  let active = workers.length;
  let inFlight = 0;
  const waiters = [];
  const wakeAll = () => {
    while (waiters.length > 0) waiters.shift()();
  };
  const waitForWork = () => new Promise((res) => waiters.push(res));

  const runWorker = async (worker) => {
    while (!isShuttingDown()) {
      if (queue.length === 0) {
        if (inFlight === 0) break;
        await waitForWork();
        continue;
      }
      const run = queue.shift();
      inFlight += 1;
      try {
        let out = await runOne(worker, run);
        if (out.status === 3 && !isShuttingDown()) {
          if (worker.isReady()) {
            // Matches checks.sh: status 3 (Chrome never showed the page) gets one immediate retry.
            out = await runOne(worker, run);
          } else if (active > 1) {
            active -= 1;
            warn(`Warning: retiring Chrome worker after failed restart (${out.err || 'unknown error'}); ${active} remaining`);
            await worker.stop().catch(() => {});
            queue.unshift(run);
            return;
          } else {
            onResult(run, out);
            return;
          }
        }
        onResult(run, out);
        if (!worker.isReady() && !isShuttingDown()) {
          if (active > 1) {
            active -= 1;
            warn(`Warning: retiring Chrome worker after failed restart; ${active} remaining`);
            await worker.stop().catch(() => {});
          }
          return;
        }
      } finally {
        inFlight -= 1;
        wakeAll();
      }
    }
  };
  await Promise.all(workers.map(runWorker));
}

async function main() {
  const opts = parseCli(process.argv.slice(2));
  if (!opts.dir || !opts.plan) {
    console.error('Usage: runner.mjs --dir <dir> --plan <plan.json> [--jobs <n>] [--fixture] [--bin <engine>] [--chrome <path>]');
    process.exit(2);
  }
  const runs = JSON.parse(readFileSync(opts.plan, 'utf8'));
  const chromePath = findChrome(opts.chrome || process.env.PREVIEW_CHROME || '');
  const srv = await startPreviewServer({ dir: resolve(opts.dir), repo: resolve(opts.repo), fixture: opts.fixture, bin: opts.bin, runs });

  const workerCount = Math.min(opts.jobs, Math.max(1, runs.length));
  const workers = Array.from({ length: workerCount }, () => new ChromeWorker(chromePath, { fixture: opts.fixture }));
  let shuttingDown = false;
  const shutdown = async () => {
    if (shuttingDown) return;
    shuttingDown = true;
    await Promise.allSettled(workers.map((w) => w.stop()));
    await srv.close().catch(() => {});
  };
  for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    process.on(sig, () => {
      shutdown().finally(() => process.exit(130));
    });
  }

  try {
    const ready = await startWorkers(workers);
    await runWorkerPool(
      ready,
      runs,
      (worker, run) => {
        const { page } = srv.runsById.get(String(run.id));
        return worker.runCheck(run, page, srv.port);
      },
      {
        isShuttingDown: () => shuttingDown,
        onResult: (run, out) => {
          process.stdout.write(`${JSON.stringify({ id: run.id, ...out })}\n`);
        },
      },
    );
  } finally {
    await shutdown();
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((err) => {
    console.error(err instanceof Error ? err.stack || err.message : String(err));
    process.exit(1);
  });
}
