// scripts/land.sh, which lands a branch on main: which checks it runs for which files, what it
// asks before it touches anything, and a landing itself, in a repository made for the test with
// a remote of its own. It's here rather than beside the script so that typecheck and lint read
// it, as within.test.ts is.
// The project has no types for node (the tests are the only code that runs there).
// @ts-expect-error no @types/node
import { spawn } from 'node:child_process';
// @ts-expect-error no @types/node
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

// Decoded, for a checkout whose path has a space in it: a URL's pathname keeps it as %20.
const script: string = fileURLToPath(new URL('../scripts/land.sh', import.meta.url));
const catchUp: string = fileURLToPath(new URL('../scripts/catch-up-main.sh', import.meta.url));

type Run = { status: number | null; out: string; err: string };

// Under a deadline of its own, and its process group killed after, as within.test.ts runs its
// script: the test's timer can't reach a script that hangs.
function run(command: string, args: string[], deadline = 30_000): Promise<Run> {
  return new Promise((done, fail) => {
    const child = spawn(command, args, { detached: true });
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
      fail(new Error(`${command} ${args.join(' ').slice(0, 80)} was still running after ${deadline} ms`));
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

const land = (args: string[]) => run(script, args);

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

// A repository with a remote, a main checkout and a worktree, as the script is run in: nothing
// of this Mac's git settings or hooks, and nothing left behind. The changes are to docs, which
// call for no checks, so only the landing itself is tried: its turn, the rebase, the push, the
// main checkout caught up.
//
// It commits and pushes, so it must never do either anywhere but in its own folder: git's own
// variables are cleared first (under a hook or a rebase's `exec` they name the repository the
// test runs from, and every git command here would be run on that one), every step of the
// setup stops the test if it fails (one that went on after a failed `cd` would commit and push
// from the checkout it started in), and `here` checks where it is before anything is written.
const LANDING = `
set -u
unset $(git rev-parse --local-env-vars)
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.com GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.com
tmp="$(mktemp -d)" || exit 9
[ -n "$tmp" ] && [ -d "$tmp" ] || exit 9
tmp="$(cd "$tmp" && pwd -P)" || exit 9
trap 'rm -rf "$tmp"' EXIT
step() { "$@" || { echo "setup failed: $*"; exit 9; }; }
here() { case "$(pwd -P)" in "$tmp"/*) ;; *) echo "not in the test's own folder: $(pwd -P)"; exit 9 ;; esac; }
step git init -q --bare "$tmp/origin.git"
step git clone -q "$tmp/origin.git" "$tmp/main" 2>/dev/null
step cd "$tmp/main"
here
step git checkout -q -b main
step mkdir -p scripts docs
step cp "$1" "$2" scripts/
echo one >docs/a.md
step git add -A
step git commit -q -m first
step git push -q origin main
step git worktree add -q "$tmp/topic" -b topic origin/main
echo x >docs/b.md
step git add -A
step git commit -q -m "main moved"
step git push -q origin main
step cd "$tmp/topic"
here
echo two >>docs/a.md
step git commit -q -am "docs: two"
echo "landing: $(scripts/land.sh 2>&1 | head -1 | sed 's/[0-9a-f]\\{7,\\}/<commit>/g')"
echo "main: $(git -C "$tmp/origin.git" log --format=%s main | tr '\\n' ',')"
echo "the main checkout: $(git -C "$tmp/main" log --format=%s -1)"
here
echo 'echo hi' >scripts/new.sh
step git add -A
step git commit -q -m "a script"
scripts/land.sh >/dev/null 2>&1
echo "code, with no word on its review: $?"
echo stray >left-here.txt
scripts/land.sh --no-review "a test" >/dev/null 2>&1
echo "with a file git doesn't know of: $?"
echo "main, after: $(git -C "$tmp/origin.git" log --format=%s main | tr '\\n' ',')"
# Run under another landing (whose unit tests run this very test), it's handed that one's turn
# in the environment, and leaves it alone.
sleep 300 &
other=$!
CODEALONG_LANDING_HELD=1 CODEALONG_LANDING_HOLDER=$other scripts/land.sh --no-review "a test" >/dev/null 2>&1
kill -0 $other 2>/dev/null && echo "another landing's turn is left to it"
kill $other
ls "$tmp/main/.git" | grep -c 'codealong-landing.lock$'
`;

test('a landing rebases onto a main that moved, pushes, and brings the main checkout up', { timeout: 180_000 }, async () => {
  const went = await run('bash', ['-c', LANDING, 'landing', script, catchUp], 150_000);
  expect(went.status, went.out + went.err).toBe(0);
  expect(went.out.trim().split('\n')).toEqual([
    'landing: Landed: <commit> is in origin/main. Fast-forwarded the main checkout to origin/main (<commit>).',
    'main: docs: two,main moved,first,',
    'the main checkout: docs: two',
    // Neither of these gets as far as a check or a push.
    'code, with no word on its review: 2',
    "with a file git doesn't know of: 1",
    'main, after: docs: two,main moved,first,',
    "another landing's turn is left to it",
    // The turn's lock is a file in .git, there once it has been taken.
    '1',
  ]);
});

test('the landing test stops at a setup step that fails, where it began', { timeout: 60_000 }, async () => {
  // With no script to copy in, the copy fails: nothing after it runs, least of all a commit.
  const went = await run('bash', ['-c', LANDING, 'landing', '/nowhere/land.sh', '/nowhere/catch-up-main.sh']);
  expect(went.status).toBe(9);
  expect(went.out).toContain('setup failed: cp /nowhere/land.sh');
  expect(went.out).not.toContain('landing:');
});

