// scripts/land.sh, which lands a branch on main: which checks it calls for for which files (CI
// asks it), what it asks before it touches anything, and a landing itself, in a repository made
// for the test with a remote of its own and a stand-in for GitHub. It's here rather than beside
// the script so that typecheck and lint read it, as within.test.ts is.
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

// How long a run of the script may take before it's taken for hung. These are for a script
// that never ends, not for one that's slow: at a load near 1,000 on ten cores (2026-11-19) the
// whole landing below took 103 s by itself and over 150 s with the rest of the suite beside
// it, and failed a landing of something else for it.
const WAIT = 180_000;
const LANDING_WAIT = 900_000;
// A test that runs the script several times over, each with its own wait.
const SEVERAL = LANDING_WAIT + 20_000;

// Under a deadline of its own, and its process group killed after, as within.test.ts runs its
// script: the test's timer can't reach a script that hangs.
function run(command: string, args: string[], deadline = WAIT): Promise<Run> {
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

/** The checks a change to these files calls for: `ui`, `engine`, `checks` (the browser checks), and `code` for one that needs a review. */
const plan = async (...paths: string[]) => (await land(['--plan', ...paths])).out.trim();

test('the docs alone run no checks and need no review', { timeout: SEVERAL }, async () => {
  expect(await plan('docs/guide.md', 'README.md', 'CONTRIBUTING.md', '.github/pull_request_template.md')).toBe('');
  // A plan written in design/ is words; the tokens there are read by both sides' tests.
  expect(await plan('design/plans/ios.md')).toBe('');
});

test('the words the engine’s tests and build hold run them, without a review', { timeout: SEVERAL }, async () => {
  // CLAUDE.md's size and its list of the rules files, the snapshot modes, the command's reference
  // and the plugin's copies of the skills are each pinned by a cargo test, and the bundle copies
  // LICENSE and NOTICE (tauri.conf.json `resources`), so a build without one fails.
  for (const path of ['CLAUDE.md', '.claude/rules/snapshot.md', 'skills/codealong/SKILL.md', 'skills/codealong/references/reference.md', 'plugin/skills/orchestrate/SKILL.md', 'LICENSE', 'NOTICE']) {
    expect(await plan(path), path).toBe('engine');
  }
});

test('what the bundle carries of the skill and the plugin runs the engine’s checks and needs a review', { timeout: SEVERAL }, async () => {
  for (const path of ['.claude-plugin/marketplace.json', 'plugin/bin/codealong', 'plugin/hooks/hooks.json']) {
    expect(await plan(path), path).toBe('engine code');
  }
});

test('the windows’ code runs their checks, the browser’s among them, and needs a review', { timeout: SEVERAL }, async () => {
  for (const path of ['src/App.tsx', 'src/ui/Quick.css', 'scripts/preview/queue-check.js', 'scripts/checks.sh', 'vite.config.ts', 'index.html']) {
    expect(await plan(path), path).toBe('ui checks code');
  }
  // A name with a space, a quote or a tab in it is still the path it is.
  expect(await plan('src-tauri/src/odd "name"\tx.rs')).toBe('ui engine checks code');
});

test('a unit test, its stand-ins and a script no page loads run the unit tests alone', { timeout: SEVERAL }, async () => {
  for (const path of ['src/ui/Quick.test.ts', 'src/land.test.ts', 'src/boards/menus.test.tsx', 'scripts/land.sh', 'scripts/relaunch.sh']) {
    expect(await plan(path), path).toBe('ui code');
  }
});

test('a change to what’s installed installs it first, as a clean checkout would', { timeout: SEVERAL }, async () => {
  // One of the two changed without the other passes on what's installed here and fails CI's
  // clean install.
  expect(await plan('package.json')).toBe('ui checks code install');
  expect(await plan('package-lock.json')).toBe('ui checks code install');
});

test('the engine’s code runs every check: the windows’ tests read it, and the browser checks’ made-up Mac is its', { timeout: SEVERAL }, async () => {
  expect(await plan('src-tauri/src/board.rs')).toBe('ui engine checks code');
  expect(await plan('src-tauri/peer/src/model.rs')).toBe('ui engine checks code');
});

test('what the engine’s tests read of the windows’ runs both', { timeout: SEVERAL }, async () => {
  for (const path of ['src/testing/ruleCases.json', 'src/testing/modelFields.ts', 'src/styles/tokens.css', 'src/ui/SettingsDialog.tsx', 'design/tokens.json']) {
    expect(await plan(path), path).toBe('ui engine checks code');
  }
  // The unit tests' own helpers there are no engine test's to read.
  expect(await plan('src/testing/tauri.ts', 'src/testing/dom.ts')).toBe('ui code');
});

test('several files call for what any of them does', { timeout: SEVERAL }, async () => {
  expect(await plan('docs/guide.md', 'src/menus.ts', 'CLAUDE.md')).toBe('ui engine checks code');
  // The checks themselves changed: all of them run, to see they still do.
  expect(await plan('.github/workflows/ci.yml')).toBe('ui engine checks code');
  expect(await plan('.github/dependabot.yml')).toBe('code');
});

test('it says what it doesn’t understand before doing anything', { timeout: SEVERAL }, async () => {
  const unknown = await land(['--push-anyway']);
  expect(unknown.status).toBe(2);
  expect(unknown.err).toContain('Unknown argument --push-anyway');
  expect((await land(['--reviewed-by'])).status).toBe(2);
  expect((await land(['--no-review', ''])).status).toBe(2);
});

test('an option is never taken for who reviewed it', { timeout: SEVERAL }, async () => {
  // `--reviewed-by --check` read `--check` as the reviewer, and so would have landed.
  for (const flag of ['--reviewed-by', '--no-review']) {
    const run = await land([flag, '--check']);
    expect(run.status, flag).toBe(2);
    expect(run.out, flag).toBe('');
  }
});

// A repository with a remote, a main checkout and a worktree, as the script is run in: nothing
// of this Mac's git settings or hooks, and nothing left behind. GitHub is a stand-in for `gh`,
// written below: it keeps the one pull request the script opens, and its merge queue writes
// the branch's commits again on top of main in the test's own remote and removes the branch,
// as GitHub's rebase does, or puts the pull request out when the test says its checks fail. So
// what's tried is the landing itself: the rebase, the push of the branch, the pull request, the
// wait, and this branch and the main checkout caught up.
//
// It commits and pushes, so it must never do either anywhere but in its own folder: git's own
// variables are cleared first (under a hook or a rebase's `exec` they name the repository the
// test runs from, and every git command here would be run on that one; when git can't list
// them the test stops, since `unset` of nothing succeeds), every step of the
// setup stops the test if it fails (one that went on after a failed `cd` would commit and push
// from the checkout it started in), and `here` checks where it is before anything is written.
const LANDING = `
set -u
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
git_vars="$(git rev-parse --local-env-vars)" || exit 9
unset $git_vars
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.com GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.com
tmp="$(mktemp -d)" || exit 9
[ -n "$tmp" ] && [ -d "$tmp" ] || exit 9
tmp="$(cd "$tmp" && pwd -P)" || exit 9
trap 'rm -rf "$tmp"' EXIT
step() { "$@" || { echo "setup failed: $*"; exit 9; }; }
here() { case "$(pwd -P)" in "$tmp"/*) ;; *) echo "not in the test's own folder: $(pwd -P)"; exit 9 ;; esac; }
commits() { sed 's/[0-9a-f]\\{7,\\}/<commit>/g'; }
step git init -q --bare "$tmp/origin.git"
step git clone -q "$tmp/origin.git" "$tmp/main" 2>/dev/null
step cd "$tmp/main"
here
step git checkout -q -b main
step mkdir -p scripts docs "$tmp/bin"
step cp "$1" "$2" scripts/
echo one >docs/a.md
step git add -A
step git commit -q -m first
step git push -q origin main

# GitHub, as the script asks it. What gh would have cut out of its answer with --jq is what
# this prints: where a pull request stands as "state queue merged gate asked head". The files
# the test leaves say how its queue goes: "fail" puts the pull request out once it has been
# seen in the queue, "gone" before anyone sees it there, and "moved" has its branch pushed to.
cat >"$tmp/bin/gh" <<'GH'
#!/bin/bash
f="$FAKE"
all="$*"
case "$1 $2" in
  "repo view") echo acme/northwind ;;
  "pr list")
    while [ $# -gt 1 ]; do
      [ "$1" != --head ] || asked_for="$2"
      shift
    done
    [ ! -f "$f/pr" ] || [ "$(cat "$f/head")" != "$asked_for" ] || cat "$f/pr"
    ;;
  "pr create")
    n=$(($(cat "$f/last" 2>/dev/null || echo 6) + 1))
    echo "$n" >"$f/last"
    echo "$n" >"$f/pr"
    while [ $# -gt 0 ]; do
      case "$1" in
        --title) echo "$2" >"$f/title" ;;
        --body-file) cp "$2" "$f/body" ;;
        --head) echo "$2" >"$f/head" ;;
      esac
      shift
    done
    echo "https://github.com/acme/northwind/pull/$n"
    ;;
  "pr merge")
    case "$all" in
      *--disable-auto*) rm -f "$f/asked" ;;
      *)
        echo "$all" >"$f/asked"
        echo 0 >"$f/looks"
        ;;
    esac
    ;;
  "run list")
    case "$all" in
      *workflow_dispatch*) echo "55 completed success https://github.com/acme/northwind/actions/runs/55" ;;
      *) echo "99 https://github.com/acme/northwind/actions/runs/99" ;;
    esac
    ;;
  "api graphql")
    case "$all" in
      *"mergeQueue(branch"*) [ -f "$f/no-queue" ] || echo Q_1 ;;
      *REMOVED_FROM_MERGE_QUEUE_EVENT*) echo FAILED_CHECKS ;;
      *mergeQueueEntry*)
        at="$(git --git-dir="$f/origin.git" rev-parse -q --verify "refs/heads/$(cat "$f/head")" || cat "$f/at")"
        echo "$at" >"$f/at"
        if [ ! -f "$f/asked" ]; then
          echo "OPEN - - - - $at"
          exit 0
        fi
        looks=$(($(cat "$f/looks") + 1))
        echo "$looks" >"$f/looks"
        if [ -f "$f/moved" ]; then
          echo "OPEN QUEUED - SUCCESS asked 0000000000000000000000000000000000000000"
        elif [ -f "$f/gone" ] || { [ -f "$f/fail" ] && [ "$looks" -gt 1 ]; }; then
          rm -f "$f/asked"
          echo "OPEN - - SUCCESS - $at"
        elif [ "$looks" = 1 ]; then
          echo "OPEN AWAITING_CHECKS - SUCCESS asked $at"
        else
          # The queue: the branch's commits written again on top of main, by another hand,
          # and the branch removed, as the repository is set to.
          branch="$(cat "$f/head")"
          rm -rf "$f/queue"
          git clone -q "$f/origin.git" "$f/queue" 2>/dev/null && cd "$f/queue" && git checkout -q main &&
            GIT_COMMITTER_NAME=queue git cherry-pick "main..origin/$branch" >/dev/null &&
            git push -q origin main ":$branch" || exit 1
          rm -f "$f/pr" "$f/asked"
          echo "MERGED - $(git rev-parse HEAD) SUCCESS - $at"
        fi
        ;;
    esac
    ;;
esac
GH
step chmod +x "$tmp/bin/gh"
export FAKE="$tmp" PATH="$tmp/bin:$PATH" CODEALONG_LAND_POLL=0
onmain() { git -C "$tmp/origin.git" log --format=%s main | tr '\\n' ','; }
onorigin() { git -C "$tmp/origin.git" for-each-ref --format='%(refname:short)' refs/heads | tr '\\n' ' '; }
landed() { grep -E '^(Landed|Nothing to land|Reviewed)' | commits | paste -sd' ' -; }

step git worktree add -q "$tmp/topic" -b topic origin/main
echo x >docs/b.md
step git add -A
step git commit -q -m "main moved"
step git push -q origin main
step cd "$tmp/topic"
here
echo two >>docs/a.md
step git commit -q -am "docs: two"
was="$(git rev-parse HEAD)"
echo "landing: $(scripts/land.sh 2>&1 | landed)"
echo "main: $(onmain)"
echo "the main checkout: $(git -C "$tmp/main" log --format=%s -1)"
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] && [ "$(git rev-parse HEAD)" != "$was" ] && echo "this branch is where main is, as the queue wrote it"
echo "its pull request: $(cat "$tmp/title"), $(grep -c 'no review' "$tmp/body") word on its review"
echo "origin's branches after: $(onorigin)"
here
echo 'echo hi' >scripts/new.sh
step git add -A
step git commit -q -m "a script"
scripts/land.sh >/dev/null 2>&1
echo "code, with no word on its review: $? ($(cat "$tmp/last") pull requests so far)"
echo stray >left-here.txt
scripts/land.sh --no-review "a test" >/dev/null 2>&1
echo "with a file git doesn't know of: $?"
rm left-here.txt
# Its checks fail in the queue: it's put out, and stays a pull request.
touch "$tmp/fail"
scripts/land.sh --reviewed-by "an account, a model" >"$tmp/out" 2>&1
echo "put out of the queue: $? $(grep -c 'put pull request #8 out: FAILED_CHECKS' "$tmp/out") $(grep -c 'gh run view 99 --log-failed' "$tmp/out")"
echo "main, after: $(onmain)"
# Fixed and landed again, main having moved meanwhile: the same pull request, and the branch,
# which the rebase rewrote, pushed over this script's own earlier push of it.
step git -C "$tmp/main" pull -q origin main
echo y >"$tmp/main/docs/c.md"
step git -C "$tmp/main" add -A
step git -C "$tmp/main" commit -q -m "main moved again"
step git -C "$tmp/main" push -q origin main
rm "$tmp/fail"
echo 'echo hello' >scripts/new.sh
step git commit -q -am "a script, fixed"
echo "landing again: $(scripts/land.sh --reviewed-by "an account, a model" 2>&1 | landed)"
echo "main: $(onmain) ($(cat "$tmp/last") pull requests)"
# A branch of this name that someone else pushed, with work of theirs on it, isn't pushed over.
step git -C "$tmp/main" pull -q origin main
step git -C "$tmp/main" checkout -q -b theirs
echo t >"$tmp/main/docs/t.md"
step git -C "$tmp/main" add -A
step git -C "$tmp/main" commit -q -m "theirs"
step git -C "$tmp/main" push -q origin theirs
step git -C "$tmp/main" checkout -q main
step git -C "$tmp/main" branch -q -D theirs
step git worktree add -q "$tmp/theirs" -b theirs origin/main
step cd "$tmp/theirs"
here
echo three >>docs/a.md
step git commit -q -am "docs: three"
scripts/land.sh >"$tmp/out" 2>&1
echo "over someone else's branch: $? $(grep -c "isn't what this" "$tmp/out"), theirs still $(git -C "$tmp/origin.git" log --format=%s -1 theirs)"
# The checks alone, on a branch of their own that's gone after.
step git branch -q -m mine
echo "checked: $(scripts/land.sh --check 2>&1 | tail -1 | commits)"
echo "origin's branches after: $(onorigin)"
# Queued and left: GitHub lands it with nobody waiting, and asking again says so and catches up.
echo "queued and left: $(scripts/land.sh --no-wait 2>&1 | head -1 | commits)"
gh api graphql mergeQueueEntry >/dev/null
gh api graphql mergeQueueEntry >/dev/null
echo "asked again: $(scripts/land.sh 2>&1 | landed)"
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] && echo "and this branch is where main is"
# Put out before anyone saw it in the queue: still said, not waited on for ever.
echo four >>docs/a.md
step git commit -q -am "docs: four"
touch "$tmp/gone"
scripts/land.sh >"$tmp/out" 2>&1
echo "put out unseen: $? $(grep -c 'put pull request #10 out: FAILED_CHECKS' "$tmp/out")"
rm "$tmp/gone"
# Its branch pushed to by someone else while this waits: no longer this landing's.
touch "$tmp/moved"
scripts/land.sh >"$tmp/out" 2>&1
echo "its branch moved under it: $? $(grep -c "no longer this landing's" "$tmp/out")"
rm "$tmp/moved"
# Another landing of this branch under way here.
sleep 300 &
other=$!
mkdir -p "$tmp/main/.git/codealong-landing"
echo "$other" >"$tmp/main/.git/codealong-landing/mine"
scripts/land.sh >"$tmp/out" 2>&1
echo "while another landing of it runs: $? $(grep -c 'Another landing of mine is under way' "$tmp/out")"
kill "$other"
# Without main's merge queue, asking GitHub to merge would merge unchecked.
touch "$tmp/no-queue"
scripts/land.sh >"$tmp/out" 2>&1
echo "with no merge queue: $? $(grep -c 'has no merge queue' "$tmp/out")"
step cd "$tmp/main"
scripts/land.sh >"$tmp/out" 2>&1
echo "from main: $(grep -c 'is on main' "$tmp/out")"
echo "main, at the end: $(onmain)"
`;

test('a landing rebases onto a main that moved, queues its pull request, and brings this branch and the main checkout up', { timeout: LANDING_WAIT + 20_000 }, async () => {
  const went = await run('bash', ['-c', LANDING, 'landing', script, catchUp], LANDING_WAIT);
  expect(went.status, went.out + went.err).toBe(0);
  expect(went.out.trim().split('\n')).toEqual([
    'landing: Landed: <commit> is in origin/main. Fast-forwarded the main checkout to origin/main (<commit>).',
    'main: docs: two,main moved,first,',
    'the main checkout: docs: two',
    'this branch is where main is, as the queue wrote it',
    'its pull request: docs: two, 1 word on its review',
    "origin's branches after: main ",
    // Neither of these gets as far as a push or a pull request.
    'code, with no word on its review: 2 (7 pull requests so far)',
    "with a file git doesn't know of: 1",
    'put out of the queue: 1 1 1',
    'main, after: docs: two,main moved,first,',
    'landing again: Landed: <commit> is in origin/main. Fast-forwarded the main checkout to origin/main (<commit>). Reviewed by: an account, a model.',
    'main: a script, fixed,a script,main moved again,docs: two,main moved,first, (8 pull requests)',
    "over someone else's branch: 1 1, theirs still theirs",
    'checked: Checked, not landed: <commit> passed CI (https://github.com/acme/northwind/actions/runs/55).',
    "origin's branches after: main theirs ",
    "queued and left: Queued: pull request #9 (https://github.com/acme/northwind/pull/9) lands <commit> when main's merge queue has checked it.",
    "asked again: Nothing to land: origin/main (<commit>) already has this branch's commits. Fast-forwarded the main checkout to origin/main (<commit>).",
    'and this branch is where main is',
    'put out unseen: 1 1',
    'its branch moved under it: 1 1',
    'while another landing of it runs: 1 1',
    'with no merge queue: 1 1',
    'from main: 1',
    'main, at the end: docs: three,a script, fixed,a script,main moved again,docs: two,main moved,first,',
  ]);
});

test('the landing test stops at a setup step that fails, where it began', { timeout: SEVERAL }, async () => {
  // With no script to copy in, the copy fails: nothing after it runs, least of all a commit.
  const went = await run('bash', ['-c', LANDING, 'landing', '/nowhere/land.sh', '/nowhere/catch-up-main.sh']);
  expect(went.status).toBe(9);
  expect(went.out).toContain('setup failed: cp /nowhere/land.sh');
  expect(went.out).not.toContain('landing:');
});
