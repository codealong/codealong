// scripts/preview/runner.mjs: persistent-Chrome CDP runner for scripts/checks.sh.
// Tests the pure page builder, report parser, and in-memory per-run HTTP server isolation
// without needing a live browser or engine binary.
// @ts-expect-error no @types/node
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
// @ts-expect-error no @types/node
import { tmpdir } from 'node:os';
// @ts-expect-error no @types/node
import { join } from 'node:path';
// @ts-expect-error no @types/node
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
// @ts-expect-error plain mjs helper outside src/
import { buildPreviewPage, formatReport, startPreviewServer } from '../scripts/preview/runner.mjs';

const repo: string = fileURLToPath(new URL('..', import.meta.url));
const dirs: string[] = [];

afterEach(() => {
  while (dirs.length) rmSync(dirs.pop()!, { recursive: true, force: true });
});

describe('buildPreviewPage', () => {
  const base = '<!doctype html><html><head><title>Codealong</title></head><body><div id="root"></div></body></html>';

  it('injects view setup, default dimensions, backdrop styles, and check scripts matching preview.sh', () => {
    const notch = buildPreviewPage(base, {
      id: 1,
      view: 'notch',
      project: 'Northwind',
      check: 'scripts/preview/notch-devices-check.js',
      vars: { PREVIEW_NOTCH: 'drawn', PREVIEW_DATE: '2026-12-28', PREVIEW_GLASS: '1', PREVIEW_INIT: 'scripts/preview/notch-session-init.js' },
    });
    expect([notch.width, notch.height]).toEqual([1512, 320]);
    expect(notch.html).toContain('window.__PREVIEW_DATE__="2026-12-28"');
    expect(notch.html).toContain('<script src="./clock.js"></script>');
    expect(notch.html).toContain('window.__BAR_MODE__={"mode":"notch","notch":{"x":756,"top":1440,"width":180,"height":25,"real":false}}');
    expect(notch.html).toContain('window.__PREVIEW_GLASS__=true;');
    expect(notch.html).toContain('window.__PREVIEW_SIZE__="1512,320";');
    expect(notch.html).toContain('<script src="./prelude.js"></script><script src="./init.js"></script><script src="./check.js"></script></body>');
  });

  it('uses desktop and bar view defaults and custom PREVIEW_SIZE overrides', () => {
    const desktop = buildPreviewPage(base, {
      id: 2,
      view: 'desktop',
      project: '',
      check: 'scripts/preview/desktop-devices-check.js',
      vars: { PREVIEW_PIN: 'session:@', PREVIEW_BOARDS_OFF: '1', PREVIEW_KEYS: 'held', PREVIEW_PLATFORM: 'linux' },
    });
    expect([desktop.width, desktop.height]).toEqual([360, 300]);
    expect(desktop.html).toContain('window.__CODEALONG_DESKTOP__=true;window.__CODEALONG_PIN__="session:@";');
    expect(desktop.html).toContain('window.__PREVIEW_BOARDS_OFF__=true;localStorage.setItem(\'codealong.boardsOn\',\'0\');');
    expect(desktop.html).toContain('window.__PREVIEW_KEYS_HELD__=true;');
    expect(desktop.html).toContain('window.__CODEALONG_PLATFORM__="linux";');

    const sized = buildPreviewPage(base, { id: 3, view: 'sessions', project: '', check: 'scripts/preview/a-check.js', vars: { PREVIEW_SIZE: '1280,720' } });
    expect([sized.width, sized.height]).toEqual([1280, 720]);
    expect(sized.html).toContain('window.__PREVIEW_SIZE__="1280,720";');
  });
});

describe('formatReport', () => {
  it('parses PASS, FAIL, extra lines, and missing titles like preview.sh', () => {
    expect(formatReport('done: PASS 7/7 ## note: ok')).toEqual({ status: 0, result: 'PASS 7/7', extraLines: ['note: ok'] });
    expect(formatReport('done: FAIL 2/3 opens: missing ## unanswered: foo')).toEqual({ status: 1, result: 'FAIL 2/3 opens: missing', extraLines: ['unanswered: foo'] });
    expect(formatReport('')).toEqual({ status: 3, result: 'FAIL 0/0 no result: Chrome never showed the page', extraLines: [] });
    expect(formatReport('Codealong')).toEqual({ status: 1, result: 'FAIL 0/0 no result: the check never said done (the page\'s title is "Codealong")', extraLines: [] });
    expect(formatReport('done: 12.4ms/frame', 'scripts/preview/x-bench.js')).toEqual({ status: 0, result: '12.4ms/frame', extraLines: [] });
  });
});

describe('startPreviewServer', () => {
  it('isolates per-run fixture overlays while sharing prepared board and assets', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'ca-runner-test-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'assets'));
    writeFileSync(join(dir, 'index.html'), '<html><head></head><body></body></html>');
    writeFileSync(join(dir, 'board.json'), '{"lanes":[]}');
    writeFileSync(join(dir, 'found-here.json'), '{"from":"here"}');
    writeFileSync(join(dir, 'setup-here.json'), '{"setup":"here"}');
    writeFileSync(join(dir, 'assets/app.js'), 'console.log(1);');
    const customTodos = join(dir, 'custom-todos.json');
    writeFileSync(customTodos, '{"todos":[{"id":"t1"}]}');

    const srv = await startPreviewServer({
      dir,
      repo,
      fixture: true,
      bin: '',
      runs: [
        { id: 0, view: 'todo', project: '', check: 'scripts/preview/due-check.js', vars: { PREVIEW_TODOS: customTodos } },
        { id: 1, view: 'sessions', project: '', check: 'scripts/preview/due-check.js', vars: {} },
      ],
    });
    try {
      const base = `http://127.0.0.1:${srv.port}`;
      // Run 0 gets its todos-fixture.json; Run 1 gets 404 (no fixture bleed between concurrent runs).
      const r0Todos = await fetch(`${base}/run/0/todos-fixture.json`);
      const r1Todos = await fetch(`${base}/run/1/todos-fixture.json`);
      expect(r0Todos.status).toBe(200);
      expect(await r0Todos.json()).toEqual({ todos: [{ id: 't1' }] });
      expect(r1Todos.status).toBe(404);

      // Both share board.json, assets, and fake-tauri.js cleanly.
      const board = await fetch(`${base}/run/1/board.json`);
      expect(board.status).toBe(200);
      expect(await board.json()).toEqual({ lanes: [] });
      const html = await (await fetch(`${base}/run/0/preview.html`)).text();
      expect(html).toContain('codealong.view');
      expect(html).toContain('./prelude.js');
      // Directory traversal outside $dir is rejected.
      const escaped = await fetch(`${base}/run/0/..%2F..%2Fpackage.json`);
      expect(escaped.status).toBe(404);
    } finally {
      await srv.close();
    }
  });
});
