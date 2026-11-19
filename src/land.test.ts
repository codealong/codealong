// scripts/land.sh, which lands a branch on main: which checks it runs for which files, and what
// it asks before it touches anything. The landing itself (the lock, the rebase, the push) isn't
// run here: it needs a remote to push to. It's here rather than beside the script so that
// typecheck and lint read it, as within.test.ts is.
// The project has no types for node (the tests are the only code that runs there).
// @ts-expect-error no @types/node
import { spawn } from 'node:child_process';
// @ts-expect-error no @types/node
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

// Decoded, for a checkout whose path has a space in it: a URL's pathname keeps it as %20.
const script: string = fileURLToPath(new URL('../scripts/land.sh', import.meta.url));

type Run = { status: number | null; out: string; err: string };

// Under a deadline of its own, and its process group killed after, as within.test.ts runs its
// script: the test's timer can't reach a script that hangs.
function land(args: string[], deadline = 30_000): Promise<Run> {
  return new Promise((done, fail) => {
    const child = spawn(script, args, { detached: true });
    const killGroup = () => {
      try {
        // @ts-expect-error no @types/node
        process.kill(-child.pid, 'SIGKILL');
      } catch {
        // The group is gone already: everything in it ended.
      }
    };
    let out = '';
    let err = '';
    child.stdout.on('data', (d: unknown) => (out += String(d)));
    child.stderr.on('data', (d: unknown) => (err += String(d)));
    const timer = setTimeout(() => {
      killGroup();
      fail(new Error(`land.sh ${args.join(' ')} was still running after ${deadline} ms`));
    }, deadline);
    child.on('error', (e: Error) => {
      clearTimeout(timer);
      fail(e);
    });
    child.on('close', (status: number | null) => {
      clearTimeout(timer);
      killGroup();
      done({ status, out, err });
    });
  });
}

/** The checks a change to these files calls for: `ui`, `engine`, and `code` for one that needs a review. */
const plan = async (...paths: string[]) => (await land(['--plan', ...paths])).out.trim();

test('the docs alone run no checks and need no review', { timeout: 60_000 }, async () => {
  expect(await plan('docs/guide.md', 'README.md', 'CONTRIBUTING.md', '.github/pull_request_template.md')).toBe('');
  // A plan written in design/ is words; the tokens there are read by both sides' tests.
  expect(await plan('design/plans/ios.md')).toBe('');
});

test('the words the engine’s tests and build hold run them, without a review', { timeout: 60_000 }, async () => {
  // CLAUDE.md's size and its list of the rules files, the snapshot modes, the command's reference
  // and the plugin's copies of the skills are each pinned by a cargo test, and the bundle copies
  // LICENSE and NOTICE (tauri.conf.json `resources`), so a build without one fails.
  for (const path of ['CLAUDE.md', '.claude/rules/snapshot.md', 'skills/codealong/SKILL.md', 'skills/codealong/references/reference.md', 'plugin/skills/orchestrate/SKILL.md', 'LICENSE', 'NOTICE']) {
    expect(await plan(path), path).toBe('engine');
  }
});

test('what the bundle carries of the skill and the plugin runs the engine’s checks and needs a review', { timeout: 60_000 }, async () => {
  for (const path of ['.claude-plugin/marketplace.json', 'plugin/bin/codealong', 'plugin/hooks/hooks.json']) {
    expect(await plan(path), path).toBe('engine code');
  }
});

test('the windows’ code runs their checks and needs a review', { timeout: 60_000 }, async () => {
  for (const path of ['src/App.tsx', 'src/ui/Quick.css', 'scripts/preview/queue-check.js', 'vite.config.ts']) {
    expect(await plan(path), path).toBe('ui code');
  }
  // A name with a space, a quote or a tab in it is still the path it is.
  expect(await plan('src-tauri/src/odd "name"\tx.rs')).toBe('ui engine code');
});

test('a change to what’s installed installs it first, as a clean checkout would', { timeout: 60_000 }, async () => {
  // One of the two changed without the other passes on what's installed here and fails CI's
  // clean install.
  expect(await plan('package.json')).toBe('ui code install');
  expect(await plan('package-lock.json')).toBe('ui code install');
});

test('the engine’s code runs both sides’ checks, since the windows’ tests read it', { timeout: 60_000 }, async () => {
  expect(await plan('src-tauri/src/board.rs')).toBe('ui engine code');
  expect(await plan('src-tauri/peer/src/model.rs')).toBe('ui engine code');
});

test('what the engine’s tests read of the windows’ runs both', { timeout: 60_000 }, async () => {
  for (const path of ['src/testing/ruleCases.json', 'src/styles/tokens.css', 'src/ui/SettingsDialog.tsx', 'design/tokens.json']) {
    expect(await plan(path), path).toBe('ui engine code');
  }
});

test('several files call for what any of them does', { timeout: 60_000 }, async () => {
  expect(await plan('docs/guide.md', 'src/menus.ts', 'CLAUDE.md')).toBe('ui engine code');
  expect(await plan('.github/workflows/linux.yml')).toBe('code');
});

test('it says what it doesn’t understand before doing anything', { timeout: 60_000 }, async () => {
  const unknown = await land(['--push-anyway']);
  expect(unknown.status).toBe(2);
  expect(unknown.err).toContain('Unknown argument --push-anyway');
  expect((await land(['--reviewed-by'])).status).toBe(2);
  expect((await land(['--no-review', ''])).status).toBe(2);
});

test('an option is never taken for who reviewed it', { timeout: 60_000 }, async () => {
  // `--reviewed-by --check` read `--check` as the reviewer, and so would have pushed.
  for (const flag of ['--reviewed-by', '--no-review']) {
    const run = await land([flag, '--check']);
    expect(run.status, flag).toBe(2);
    expect(run.out, flag).toBe('');
  }
});
