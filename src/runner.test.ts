// Unit and parity tests for scripts/preview/runner.mjs and scripts/checks.sh's CDP path.
// Opens no listening sockets so it holds in sandboxes too (as src/checks.test.ts does).
// @ts-expect-error no @types/node
import { spawnSync } from 'node:child_process';
// @ts-expect-error no @types/node
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
// @ts-expect-error no @types/node
import { tmpdir } from 'node:os';
// @ts-expect-error no @types/node
import { join } from 'node:path';
// @ts-expect-error no @types/node
import { fileURLToPath } from 'node:url';
import { describe, expect, test } from 'vitest';
// @ts-expect-error plain ESM script without .d.ts
import { assertFixtureEngine, assertSnapshotDir, buildPreviewPage, createPreviewHandler, FIXTURE_MARK, formatReport, wantsMacEmulation } from '../scripts/preview/runner.mjs';

const repo: string = fileURLToPath(new URL('..', import.meta.url));

/** Scrubbed environment so caller PREVIEW_* or CHECKS_LEGACY vars never leak into checks.sh tests. */
function cleanEnv(extra: Record<string, string> = {}): Record<string, string> {
  // @ts-expect-error no @types/node
  const base: Record<string, string | undefined> = process.env;
  const out: Record<string, string> = {};
  for (const k of ['PATH', 'HOME', 'TMPDIR', 'USER', 'LANG', 'LC_ALL']) {
    if (base[k]) out[k] = base[k] as string;
  }
  return { ...out, ...extra };
}

describe('scripts/preview/runner.mjs', () => {
  test('buildPreviewPage injects view config, clock, platform, and scripts matching preview.sh', () => {
    const base = '<!doctype html><html><head><title>x</title></head><body><div id="root"></div></body></html>';
    const page = buildPreviewPage(base, {
      view: 'bar',
      project: 'Northwind',
      check: 'scripts/preview/bar-clicks-check.js',
      vars: {
        PREVIEW_DATE: '2026-12-28',
        PREVIEW_GLASS: '1',
        PREVIEW_PLATFORM: 'linux',
        PREVIEW_BOARDS_OFF: '1',
      },
    });
    expect(page.width).toBe(1200);
    expect(page.height).toBe(64);
    expect(page.html).toContain('window.__PREVIEW_DATE__="2026-12-28"');
    expect(page.html).toContain('<script src="./clock.js"></script>');
    expect(page.html).toContain('window.__CODEALONG_BAR__=true;');
    expect(page.html).toContain('window.__PREVIEW_GLASS__=true;');
    expect(page.html).toContain('window.__CODEALONG_PLATFORM__="linux";');
    expect(page.html).toContain("window.__PREVIEW_BOARDS_OFF__=true;localStorage.setItem('codealong.boardsOn','0');");
    expect(page.html).toContain('<script src="./prelude.js"></script><script src="./check.js"></script></body>');
  });

  test('wantsMacEmulation emulates macOS only on non-macOS hosts unless another platform is asked for', () => {
    expect(wantsMacEmulation({}, { hostPlatform: 'linux' })).toBe(true);
    expect(wantsMacEmulation({ PREVIEW_PLATFORM: 'mac' }, { hostPlatform: 'linux' })).toBe(true);
    expect(wantsMacEmulation({ PREVIEW_PLATFORM: 'linux' }, { hostPlatform: 'linux' })).toBe(false);
    expect(wantsMacEmulation({ PREVIEW_PLATFORM: 'windows' }, { hostPlatform: 'linux' })).toBe(false);
    expect(wantsMacEmulation({}, { hostPlatform: 'darwin' })).toBe(false);
    expect(wantsMacEmulation({ PREVIEW_PLATFORM: 'mac' }, { hostPlatform: 'darwin' })).toBe(false);
  });

  test('formatReport classifies pass, fail, empty title (status 3), non-done title, and bench', () => {
    expect(formatReport('done: PASS 7/7 ## note: ok')).toEqual({ status: 0, result: 'PASS 7/7', extraLines: ['note: ok'] });
    expect(formatReport('done: FAIL 5/7 chip: cut off ## unanswered: foo')).toEqual({
      status: 1,
      result: 'FAIL 5/7 chip: cut off',
      extraLines: ['unanswered: foo'],
    });
    expect(formatReport('')).toEqual({
      status: 3,
      result: 'FAIL 0/0 no result: Chrome never showed the page',
      extraLines: [],
    });
    expect(formatReport('Codealong')).toEqual({
      status: 1,
      result: 'FAIL 0/0 no result: the check never said done (the page\'s title is "Codealong")',
      extraLines: [],
    });
    expect(formatReport('done: 42 ms', 'scripts/preview/scroll-bench.js')).toEqual({ status: 0, result: '42 ms', extraLines: [] });
  });

  test('createPreviewHandler isolates per-run fixtures, blocks traversal, and enforces snapshot-of and engine guards without sockets', async () => {
    const tmp = mkdtempSync(join(tmpdir(), 'ca-runner-test-'));
    try {
      const dir = join(tmp, 'preview');
      const repoDir = join(tmp, 'repo');
      mkdirSync(join(dir, 'assets'), { recursive: true });
      mkdirSync(join(repoDir, 'scripts/preview'), { recursive: true });
      writeFileSync(join(dir, 'index.html'), '<html><head></head><body></body></html>');
      writeFileSync(join(dir, 'board.json'), '{"lanes":[]}');
      writeFileSync(join(dir, 'snapshot-of'), FIXTURE_MARK);
      writeFileSync(join(tmp, 'secret.txt'), 'nope');
      writeFileSync(join(repoDir, 'scripts/preview/todos-a.json'), '[{"id":"a"}]');
      writeFileSync(join(repoDir, 'scripts/preview/todos-b.json'), '[{"id":"b"}]');
      writeFileSync(join(repoDir, 'scripts/preview/fake-tauri.js'), '// stub');
      writeFileSync(join(repoDir, 'scripts/preview/prelude.js'), '// prelude');

      // Refuses wrong snapshot-of mark.
      expect(() => assertSnapshotDir(dir, false)).toThrow(/not of this Mac/);
      writeFileSync(join(dir, 'snapshot-of'), 'the made-up Mac');
      expect(() => assertSnapshotDir(dir, true)).toThrow(/not of the made-up Mac, as its engine named it/);
      writeFileSync(join(dir, 'snapshot-of'), FIXTURE_MARK);

      // Refuses an old engine binary that doesn't answer `--snapshot --fixture name`.
      const oldEngine = join(tmp, 'old-engine');
      writeFileSync(oldEngine, '#!/usr/bin/env bash\necho \'{"lanes":[]}\'\n');
      chmodSync(oldEngine, 0o755);
      expect(() => assertFixtureEngine(oldEngine)).toThrow(/has no made-up Mac/);

      const goodEngine = join(tmp, 'good-engine');
      writeFileSync(goodEngine, '#!/usr/bin/env bash\n[ "${3:-}" = name ] && echo "the made-up Mac" || echo \'{"items":[]}\'\n');
      chmodSync(goodEngine, 0o755);

      const handler = createPreviewHandler({
        dir,
        repo: repoDir,
        fixture: true,
        bin: goodEngine,
        runs: [
          { id: 0, view: 'sessions', project: '', check: '', vars: { PREVIEW_TODOS: 'scripts/preview/todos-a.json', PREVIEW_TRANSCRIPT: 'claude:a:1' } },
          { id: 1, view: 'sessions', project: '', check: '', vars: { PREVIEW_TODOS: 'scripts/preview/todos-b.json' } },
        ],
      });

      const a = await handler.servePath('/run/0/todos-fixture.json');
      const b = await handler.servePath('/run/1/todos-fixture.json');
      const tr = await handler.servePath('/run/0/transcript.json');
      expect(a.status).toBe(200);
      expect(b.status).toBe(200);
      expect(tr.status).toBe(200);
      expect(a.body.toString('utf8')).toBe('[{"id":"a"}]');
      expect(b.body.toString('utf8')).toBe('[{"id":"b"}]');
      expect(tr.body.toString('utf8').trim()).toBe('{"items":[]}');

      // Both plain and percent-encoded path traversal must hit the guard and fail closed.
      expect((await handler.servePath('/run/0/../secret.txt')).status).toBe(404);
      expect((await handler.servePath('/run/0/..%2Fsecret.txt')).status).toBe(404);
      expect((await handler.servePath('/run/0/images/..%2F..%2Fsecret.txt')).status).toBe(404);
      expect((await handler.servePath('/run/0/%E0%A4%A')).status).toBe(404);
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });

  test('runner.mjs handles every PREVIEW_* page/fixture variable preview.sh uses', () => {
    // Keeps preview.sh and runner.mjs in step so adding a new PREVIEW_* knob to preview.sh fails
    // unit tests unless runner.mjs handles it too.
    const previewSh: string = readFileSync(join(repo, 'scripts/preview.sh'), 'utf8');
    const runnerJs: string = readFileSync(join(repo, 'scripts/preview/runner.mjs'), 'utf8');
    const runnerOnlyNonPage = new Set([
      'PREVIEW_DIR',
      'PREVIEW_CHROME',
      'PREVIEW_SCHEME',
      'PREVIEW_NAME',
      'PREVIEW_PREPARE',
      'PREVIEW_BIN',
      'PREVIEW_FIXTURE',
      'PREVIEW_PORT',
      'PREVIEW_POLL',
      'PREVIEW_WAIT_POLL',
    ]);
    const previewVars = new Set<string>();
    for (const line of previewSh.split('\n')) {
      if (line.trimStart().startsWith('#')) continue;
      for (const m of line.matchAll(/\b(PREVIEW_[A-Z0-9_]+)\b/g)) {
        if (!runnerOnlyNonPage.has(m[1])) previewVars.add(m[1]);
      }
    }
    const missing = [...previewVars].filter((v) => !runnerJs.includes(v)).sort();
    expect(missing, 'add new PREVIEW_* variables to scripts/preview/runner.mjs too').toEqual([]);

    // Default viewport sizes per view must also match scripts/preview.sh.
    const base = '<html><head></head><body></body></html>';
    const viewSizes = new Map<string, string>();
    let currentView = 'sessions';
    for (const line of previewSh.split('\n')) {
      const vm = /^\s*(?:el)?if \[ "\$view" = ([a-z]+) \]; then/.exec(line);
      if (vm) currentView = vm[1];
      const sm = /^\s*size="\$\{PREVIEW_SIZE:-(\d+,\d+)\}"/.exec(line);
      if (sm) viewSizes.set(currentView, sm[1]);
    }
    expect(viewSizes.size).toBeGreaterThanOrEqual(8);
    for (const [view, expected] of viewSizes) {
      const page = buildPreviewPage(base, { view, project: '', check: '', vars: {} });
      expect(`${page.width},${page.height}`, `default viewport for view=${view}`).toBe(expected);
    }
  });

  test('checks.sh run_cdp pins listed filtering, NONE retry, static rows, PREVIEW_* forwarding, and fail-fast startup errors', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'ca-cdp-checks-'));
    try {
      mkdirSync(join(tmp, 'scripts/preview'), { recursive: true });
      mkdirSync(join(tmp, '.preview/fixture'), { recursive: true });
      writeFileSync(join(tmp, 'scripts/checks.sh'), readFileSync(join(repo, 'scripts/checks.sh'), 'utf8'));
      chmodSync(join(tmp, 'scripts/checks.sh'), 0o755);
      writeFileSync(join(tmp, 'scripts/preview.sh'), '#!/usr/bin/env bash\necho "unexpected preview.sh call" >&2; exit 9\n');
      chmodSync(join(tmp, 'scripts/preview.sh'), 0o755);
      writeFileSync(join(tmp, '.preview/fixture/board.json'), '{"lanes":[]}');
      writeFileSync(join(tmp, '.preview/fixture/index.html'), '<html><head></head><body></body></html>');
      writeFileSync(join(tmp, '.preview/fixture/snapshot-of'), 'this Mac');
      writeFileSync(join(tmp, '.preview/board.json'), '{"lanes":[]}');
      writeFileSync(join(tmp, '.preview/index.html'), '<html><head></head><body></body></html>');
      writeFileSync(join(tmp, '.preview/snapshot-of'), 'this Mac');

      writeFileSync(
        join(tmp, 'scripts/preview/alpha-check.js'),
        "//   scripts/preview.sh sessions '' scripts/preview/alpha-check.js\n//   PREVIEW_SCHEME=dark scripts/preview.sh sessions '' scripts/preview/alpha-check.js\n",
      );
      writeFileSync(
        join(tmp, 'scripts/preview/hand-check.js'),
        "//   PREVIEW_TRANSCRIPT=<key> scripts/preview.sh sessions '' scripts/preview/hand-check.js\n",
      );
      writeFileSync(join(tmp, 'scripts/preview/noheader-check.js'), '// no run command\n');
      writeFileSync(
        join(tmp, 'scripts/preview/fixture-passes.txt'),
        "scripts/preview.sh sessions '' scripts/preview/alpha-check.js\n",
      );

      // Stub runner.mjs enforcing the real snapshot-of check and recording seen plans.
      const seenPlans = join(tmp, 'plans.ndjson');
      writeFileSync(
        join(tmp, 'scripts/preview/runner.mjs'),
        `import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
const args = process.argv.slice(2);
const dir = args[args.indexOf('--dir') + 1];
const planPath = args[args.indexOf('--plan') + 1];
const fixture = args.includes('--fixture');
const want = fixture ? ${JSON.stringify(FIXTURE_MARK)} : 'this Mac';
const got = existsSync(join(dir, 'snapshot-of')) ? readFileSync(join(dir, 'snapshot-of'), 'utf8').trim() : 'this Mac';
if (got !== want) {
  console.error(\`\${dir} holds the snapshots of \${got}, not of \${want}\`);
  process.exit(1);
}
const plan = JSON.parse(readFileSync(planPath, 'utf8'));
appendFileSync(${JSON.stringify(seenPlans)}, JSON.stringify({ fixture, plan }) + '\\n');
const marker = ${JSON.stringify(join(tmp, 'seen.count'))};
const n = existsSync(marker) ? Number(readFileSync(marker, 'utf8')) + 1 : 1;
writeFileSync(marker, String(n));
for (const r of plan) {
  const out = fixture && n === 1
    ? { id: r.id, status: 3, result: 'FAIL 0/0 no result: Chrome never showed the page', extraLines: [] }
    : { id: r.id, status: 0, result: 'PASS 2/2', extraLines: ['unanswered: fake_cmd'] };
  console.log(JSON.stringify(out));
}
`,
      );

      // 1) Startup refusal (mismatched snapshot-of) fails fast without retry or bogus gate rows.
      const wrongMark = spawnSync('bash', ['scripts/checks.sh', '--fixture', '--prepared', '--listed', 'alpha'], {
        cwd: tmp,
        env: cleanEnv(),
        encoding: 'utf8',
        timeout: 30_000,
      });
      expect(wrongMark.status).toBe(1);
      expect(wrongMark.stderr).toContain('holds the snapshots of this Mac, not of the made-up Mac, as its engine named it');
      expect(wrongMark.stdout).not.toContain('Trying again');
      expect(wrongMark.stdout).not.toContain('lists a run no check has');

      // 2) Fixture gate: strips caller PREVIEW_SCHEME, filters listed run, retries NONE once, reports unanswered.
      writeFileSync(join(tmp, '.preview/fixture/snapshot-of'), FIXTURE_MARK);
      const ok = spawnSync('bash', ['scripts/checks.sh', '--fixture', '--prepared', '--listed', 'alpha'], {
        cwd: tmp,
        env: cleanEnv({ PREVIEW_SCHEME: 'dark' }),
        encoding: 'utf8',
        timeout: 30_000,
      });
      expect(ok.status, ok.stdout + ok.stderr).toBe(0);
      expect(ok.stdout).toContain('Trying again the runs that gave no result, of: alpha');
      expect(ok.stdout).toContain('1 passed, 0 failed.');
      expect(ok.stdout).toContain('Commands the stub had no answer for: fake_cmd');

      // 3) Real data: forwards caller PREVIEW_SCHEME=dark, SKIPs <key> hand-run, FAILs headerless check.
      const real = spawnSync('bash', ['scripts/checks.sh', '--prepared', 'alpha', 'hand', 'noheader'], {
        cwd: tmp,
        env: cleanEnv({ PREVIEW_SCHEME: 'dark' }),
        encoding: 'utf8',
        timeout: 30_000,
      });
      expect(real.status).toBe(1);
      expect(real.stdout).toContain('SKIP  hand');
      expect(real.stdout).toContain('FAIL  noheader');
      expect(real.stdout).toContain('2 passed, 1 failed, 1 not run.');

      const plans = readFileSync(seenPlans, 'utf8')
        .trim()
        .split('\n')
        .map((l: string) => JSON.parse(l));
      // Fixture runs stripped PREVIEW_SCHEME=dark; real-data run carried it into alpha's first run.
      expect(plans[0].fixture).toBe(true);
      expect(plans[0].plan[0].vars.PREVIEW_SCHEME).toBeUndefined();
      const realPlan = plans.find((p: { fixture: boolean }) => !p.fixture);
      expect(realPlan.plan[0].vars.PREVIEW_SCHEME).toBe('dark');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});
