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
    expect(page.width).toBe(1100);
    expect(page.height).toBe(64);
    expect(page.html).toContain('window.__PREVIEW_DATE__="2026-12-28"');
    expect(page.html).toContain('<script src="./clock.js"></script>');
    expect(page.html).toContain('window.__CODEALONG_BAR__=true;');
    expect(page.html).toContain('window.__PREVIEW_GLASS__=true;');
    expect(page.html).toContain('window.__CODEALONG_PLATFORM__="linux";');
    expect(page.html).toContain("window.__PREVIEW_BOARDS_OFF__=true;localStorage.setItem('codealong.boardsOn','0');");
    expect(page.html).toContain('<script src="./prelude.js"></script><script src="./check.js"></script></body>');
  });

  test('wantsMacEmulation emulates macOS on Linux runners unless a non-mac platform is asked for', () => {
    expect(wantsMacEmulation({}, { fixture: true, hostPlatform: 'linux' })).toBe(true);
    expect(wantsMacEmulation({}, { fixture: false, hostPlatform: 'linux' })).toBe(true);
    expect(wantsMacEmulation({}, { fixture: false, hostPlatform: 'darwin' })).toBe(false);
    expect(wantsMacEmulation({ PREVIEW_PLATFORM: 'mac' }, { fixture: false, hostPlatform: 'linux' })).toBe(true);
    expect(wantsMacEmulation({ PREVIEW_PLATFORM: 'linux' }, { fixture: true, hostPlatform: 'linux' })).toBe(false);
    expect(wantsMacEmulation({ PREVIEW_PLATFORM: 'windows' }, { fixture: true, hostPlatform: 'linux' })).toBe(false);
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

  test('createPreviewHandler isolates per-run fixtures, blocks traversal, and enforces snapshot-of and engine guards without sockets', () => {
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
      writeFileSync(goodEngine, '#!/usr/bin/env bash\n[ "${3:-}" = name ] && echo "the made-up Mac" || echo "{}"\n');
      chmodSync(goodEngine, 0o755);

      const handler = createPreviewHandler({
        dir,
        repo: repoDir,
        fixture: true,
        bin: goodEngine,
        runs: [
          { id: 0, view: 'sessions', project: '', check: '', vars: { PREVIEW_TODOS: 'scripts/preview/todos-a.json' } },
          { id: 1, view: 'sessions', project: '', check: '', vars: { PREVIEW_TODOS: 'scripts/preview/todos-b.json' } },
        ],
      });

      const a = handler.servePath('/run/0/todos-fixture.json');
      const b = handler.servePath('/run/1/todos-fixture.json');
      expect(a.status).toBe(200);
      expect(b.status).toBe(200);
      expect(a.body.toString('utf8')).toBe('[{"id":"a"}]');
      expect(b.body.toString('utf8')).toBe('[{"id":"b"}]');

      // Both plain and percent-encoded path traversal must hit the guard and fail closed.
      expect(handler.servePath('/run/0/../secret.txt').status).toBe(404);
      expect(handler.servePath('/run/0/..%2Fsecret.txt').status).toBe(404);
      expect(handler.servePath('/run/0/images/..%2F..%2Fsecret.txt').status).toBe(404);
      expect(handler.servePath('/run/0/%E0%A4%A').status).toBe(404);
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
    // Extract every PREVIEW_<WORD> referenced in executable lines (skipping header comments).
    const previewVars = new Set<string>();
    for (const line of previewSh.split('\n')) {
      if (line.trimStart().startsWith('#')) continue;
      for (const m of line.matchAll(/\b(PREVIEW_[A-Z0-9_]+)\b/g)) {
        if (!runnerOnlyNonPage.has(m[1])) previewVars.add(m[1]);
      }
    }
    const missing = [...previewVars].filter((v) => !runnerJs.includes(v)).sort();
    expect(missing, 'add new PREVIEW_* variables to scripts/preview/runner.mjs too').toEqual([]);
  });

  test('checks.sh run_cdp filters listed runs, retries NONE once, forwards unanswered, and guards snapshot-of', () => {
    const tmp = mkdtempSync(join(tmpdir(), 'ca-cdp-checks-'));
    try {
      mkdirSync(join(tmp, 'scripts/preview'), { recursive: true });
      mkdirSync(join(tmp, '.preview/fixture'), { recursive: true });
      writeFileSync(join(tmp, 'scripts/checks.sh'), readFileSync(join(repo, 'scripts/checks.sh'), 'utf8'));
      chmodSync(join(tmp, 'scripts/checks.sh'), 0o755);
      // Stub preview.sh (should never be called when --prepared is passed).
      writeFileSync(join(tmp, 'scripts/preview.sh'), '#!/usr/bin/env bash\necho "unexpected preview.sh call" >&2; exit 9\n');
      chmodSync(join(tmp, 'scripts/preview.sh'), 0o755);
      writeFileSync(join(tmp, '.preview/fixture/board.json'), '{"lanes":[]}');
      writeFileSync(join(tmp, '.preview/fixture/index.html'), '<html><head></head><body></body></html>');
      writeFileSync(join(tmp, '.preview/fixture/snapshot-of'), 'this Mac');

      writeFileSync(
        join(tmp, 'scripts/preview/alpha-check.js'),
        '//   scripts/preview.sh sessions \'\' scripts/preview/alpha-check.js\n//   PREVIEW_SCHEME=dark scripts/preview.sh sessions \'\' scripts/preview/alpha-check.js\n',
      );
      writeFileSync(
        join(tmp, 'scripts/preview/fixture-passes.txt'),
        "scripts/preview.sh sessions '' scripts/preview/alpha-check.js\n",
      );

      // Stub runner.mjs: first invocation returns status 3 (NONE), second invocation returns PASS with unanswered.
      writeFileSync(
        join(tmp, 'scripts/preview/runner.mjs'),
        `import { existsSync, readFileSync, writeFileSync } from 'node:fs';
const args = process.argv.slice(2);
const planPath = args[args.indexOf('--plan') + 1];
const plan = JSON.parse(readFileSync(planPath, 'utf8'));
const marker = ${JSON.stringify(join(tmp, 'seen.count'))};
const n = existsSync(marker) ? Number(readFileSync(marker, 'utf8')) + 1 : 1;
writeFileSync(marker, String(n));
for (const r of plan) {
  const out = n === 1
    ? { id: r.id, status: 3, result: 'FAIL 0/0 no result: Chrome never showed the page', extraLines: [] }
    : { id: r.id, status: 0, result: 'PASS 2/2', extraLines: ['unanswered: fake_cmd'] };
  console.log(JSON.stringify(out));
}
`,
      );

      // 1) Refuses --fixture --prepared when snapshot-of says "this Mac".
      const wrongMark = spawnSync('bash', ['scripts/checks.sh', '--fixture', '--prepared', '--listed', 'alpha'], {
        cwd: tmp,
        encoding: 'utf8',
      });
      expect(wrongMark.status).toBe(1);
      expect(wrongMark.stdout).toContain('holds the snapshots of this Mac, not of the made-up Mac, as its engine named it');

      // 2) With the right mark: runs only the listed run, retries NONE once, passes, and reports unanswered.
      writeFileSync(join(tmp, '.preview/fixture/snapshot-of'), FIXTURE_MARK);
      const ok = spawnSync('bash', ['scripts/checks.sh', '--fixture', '--prepared', '--listed', 'alpha'], {
        cwd: tmp,
        encoding: 'utf8',
      });
      expect(ok.status, ok.stdout + ok.stderr).toBe(0);
      expect(ok.stdout).toContain('Trying again the runs that gave no result, of: alpha');
      expect(ok.stdout).toContain('1 passed, 0 failed.');
      expect(ok.stdout).toContain('Commands the stub had no answer for: fake_cmd');
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});
