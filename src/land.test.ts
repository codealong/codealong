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
printf '%s\\n' one a b c d e >docs/a.md
step git add -A
step git commit -q -m first
step git push -q origin main

# GitHub, as the script asks it. What gh would have cut out of its answer with --jq is what
# this prints: where a pull request stands as "state queue merged id standing applies head",
# and a run of CI as "status conclusion id address". The files the test leaves say how it
# goes: "slow" holds how many looks the pull request's own run of CI is still going for and
# "red" has it fail; "standing" has someone's request to merge the pull request by itself on
# it, and "conflicting" has main moved under it; "lost" loses GitHub's yes to queueing it
# ("late" holds how many looks it then takes to show as queued), and with "swapped" it has
# merged by then as another commit; "refused" has GitHub say no to queueing it, and "dropped"
# has the asking never reach it, and "muddled" has it queued with an error in the answering;
# "unseen" has it merged without origin's main having the commit yet; "fail" puts it out of
# the queue once
# it has been seen there, "gone" before anyone sees it there, and "mute" has GitHub not say
# whether it was; "moved" has its branch pushed to.
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
    rm -f "$f/evicted"
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
  "run list")
    case "$all" in
      *workflow_dispatch*) echo "55 completed success https://github.com/acme/northwind/actions/runs/55" ;;
      *pull_request*)
        # The pull request's own run.
        if [ -f "$f/red" ]; then
          echo "completed failure 77 https://github.com/acme/northwind/actions/runs/77"
        elif [ -f "$f/slow" ] && [ "$(cat "$f/slow")" -gt 0 ]; then
          echo $(($(cat "$f/slow") - 1)) >"$f/slow"
          echo "in_progress - 77 https://github.com/acme/northwind/actions/runs/77"
        else
          echo "completed success 77 https://github.com/acme/northwind/actions/runs/77"
        fi
        ;;
      *) echo "99 https://github.com/acme/northwind/actions/runs/99" ;;
    esac
    ;;
  "api graphql")
    case "$all" in
      *"mergeQueue(branch"*) [ -f "$f/no-queue" ] || echo Q_1 ;;
      *REMOVED_FROM_MERGE_QUEUE_EVENT*)
        [ ! -f "$f/mute" ] || exit 1
        [ ! -f "$f/evicted" ] || echo FAILED_CHECKS
        ;;
      *enqueuePullRequest*)
        if [ -f "$f/refused" ]; then
          echo '{"data":{"enqueuePullRequest":null},"errors":[{"type":"UNPROCESSABLE","message":"Failed to add the pull request"}]}'
          exit 1
        fi
        [ ! -f "$f/dropped" ] || exit 1
        # A place in the queue, for the commit named.
        echo "$all" >"$f/asked"
        touch "$f/queued"
        rm -f "$f/evicted"
        echo 0 >"$f/looks"
        if [ -f "$f/swapped" ]; then
          number="$(cat "$f/pr")"
          rm -f "$f/pr" "$f/queued"
          echo "MERGED - $(git --git-dir="$f/origin.git" rev-parse main) PR_$number - MERGEABLE 0000000000000000000000000000000000000000" >"$f/merged.$number"
        fi
        if [ -f "$f/muddled" ]; then
          echo '{"data":{"enqueuePullRequest":{"mergeQueueEntry":null}},"errors":[{"type":"INTERNAL","path":["enqueuePullRequest","mergeQueueEntry","id"],"message":"Something went wrong"}]}'
          exit 1
        fi
        # Taken, with the answer lost on its way back.
        [ ! -f "$f/lost" ] || exit 1
        ;;
      *mergeQueueEntry*)
        # A pull request that merged stays merged, whoever asks after.
        number="\${all##*number=}"
        number="\${number%% *}"
        if [ -f "$f/merged.$number" ]; then
          cat "$f/merged.$number"
          exit 0
        fi
        at="$(git --git-dir="$f/origin.git" rev-parse -q --verify "refs/heads/$(cat "$f/head")" || cat "$f/at")"
        echo "$at" >"$f/at"
        if [ ! -f "$f/queued" ]; then
          standing=-
          [ ! -f "$f/standing" ] || standing=set
          applies=MERGEABLE
          [ ! -f "$f/conflicting" ] || applies=CONFLICTING
          echo "OPEN - - PR_$number $standing $applies $at"
          exit 0
        fi
        if [ -f "$f/late" ] && [ "$(cat "$f/late")" -gt 0 ]; then
          echo $(($(cat "$f/late") - 1)) >"$f/late"
          echo "OPEN - - PR_$number - MERGEABLE $at"
          exit 0
        fi
        looks=$(($(cat "$f/looks") + 1))
        echo "$looks" >"$f/looks"
        if [ -f "$f/moved" ]; then
          echo "OPEN QUEUED - PR_$number - MERGEABLE 0000000000000000000000000000000000000000"
        elif [ -f "$f/gone" ] || { [ -f "$f/fail" ] && [ "$looks" -gt 1 ]; }; then
          rm -f "$f/queued"
          touch "$f/evicted"
          echo "OPEN - - PR_$number - MERGEABLE $at"
        elif [ "$looks" = 1 ]; then
          echo "OPEN AWAITING_CHECKS - PR_$number - MERGEABLE $at"
        else
          # The queue: the branch's commits written again on top of main, by another hand,
          # and the branch removed, as the repository is set to.
          branch="$(cat "$f/head")"
          rm -rf "$f/queue"
          git clone -q "$f/origin.git" "$f/queue" 2>/dev/null && cd "$f/queue" && git checkout -q main &&
            GIT_COMMITTER_NAME=queue git cherry-pick "main..origin/$branch" >/dev/null &&
            { [ -f "$f/unseen" ] || git push -q origin main ":$branch"; } || exit 1
          rm -f "$f/pr" "$f/queued"
          echo "MERGED - $(git rev-parse HEAD) PR_$number - MERGEABLE $at" | tee "$f/merged.$number"
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
# Beside the lines the branch is about to change, though not on them: the queue's copy of its
# commit then isn't the same patch as the commit, and a rebase onto main writes it again (as
# nothing) rather than skip it.
printf '%s\\n' one a b "c, moved" d e >docs/a.md
step git add -A
step git commit -q -m "main moved"
step git push -q origin main
step cd "$tmp/topic"
here
echo two >>docs/a.md
step git commit -q -am "docs: two"
was="$(git rev-parse HEAD)"
# A tag on the commit, in a checkout set to push tags with their commits: a landing pushes the
# branch alone.
step git tag -a -m "a version" v9.9.9
step git config push.followTags true
# And a second branch at the commit, in a checkout set to move such branches along with a
# rebase: catching this one up leaves that one where it is.
step git branch keep
step git config rebase.updateRefs true
# Main has moved, and the branch lands as it is: the queue writes it on top. Its own run of CI
# takes a moment, which is waited for before it's queued.
echo 2 >"$tmp/slow"
echo "landing: $(scripts/land.sh 2>&1 | landed)"
rm "$tmp/slow"
echo "main: $(onmain)"
echo "the main checkout: $(git -C "$tmp/main" log --format=%s -1)"
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] && [ "$(git rev-parse HEAD)" != "$was" ] && echo "this branch is where main is, as the queue wrote it"
[ "$(git rev-parse keep)" = "$was" ] && echo "and a second branch at the commit it was at is still there"
echo "its pull request: $(cat "$tmp/title"), $(grep -c 'no review' "$tmp/body") word on its review, $(grep -c 'scripts/land.sh queues this' "$tmp/body") line for CI"
echo "queued as the commit it pushed, and no other: $(grep -c "head=$(cat "$tmp/at") " "$tmp/asked")"
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
# Fixed, main having moved meanwhile, and rebased onto it by hand, as one that didn't apply
# would have to be.
step git -C "$tmp/main" pull -q origin main
echo y >"$tmp/main/docs/c.md"
step git -C "$tmp/main" add -A
step git -C "$tmp/main" commit -q -m "main moved again"
step git -C "$tmp/main" push -q origin main
rm "$tmp/fail"
echo 'echo hello' >scripts/new.sh
step git commit -q -am "a script, fixed"
step git fetch -q origin main
step git rebase -q origin/main
# Someone has its pull request set to merge by itself: nothing is pushed to one that would land
# whatever it was given.
touch "$tmp/standing"
scripts/land.sh --reviewed-by "an account, a model" >"$tmp/out" 2>&1
echo "set to merge by itself: $? $(grep -c 'disable-auto' "$tmp/out"), origin's still $(git -C "$tmp/origin.git" log --format=%s -1 topic)"
rm "$tmp/standing"
# Landed again: the same pull request, and the branch pushed over this script's own earlier
# push of it. GitHub's yes to queueing it is lost on its way back, and it takes GitHub a look
# or two to show it queued: waited for, not taken for a no.
touch "$tmp/lost"
echo 2 >"$tmp/late"
echo "landing again: $(scripts/land.sh --reviewed-by "an account, a model" 2>&1 | landed)"
rm "$tmp/lost" "$tmp/late"
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
# With a version's tag on the commit: neither the push that makes the branch nor the one that
# removes it takes the tag along.
step git tag -a -m "another version" v9.9.8
echo "checked: $(scripts/land.sh --check 2>&1 | tail -1 | commits)"
echo "origin's branches after: $(onorigin), with $(git -C "$tmp/origin.git" tag | wc -l | tr -d ' ') tags"
# Queued and left: GitHub lands it with nobody waiting, and asking again says so and catches up.
echo "queued and left: $(scripts/land.sh --no-wait 2>&1 | head -1 | commits)"
echo "asked again, still queued: $(scripts/land.sh --no-wait 2>&1 | head -1 | commits)"
gh api graphql mergeQueueEntry number=9 >/dev/null
gh api graphql mergeQueueEntry number=9 >/dev/null
echo "asked again: $(scripts/land.sh 2>&1 | landed)"
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] && echo "and this branch is where main is"
# Stopped while the pull request's own run of CI was still going, before there was a place in
# the queue to ask for: asked again, it's queued then, not waited on as if it had been.
echo four >>docs/a.md
step git commit -q -am "docs: four"
echo 99 >"$tmp/slow"
CODEALONG_LAND_WAIT=-1 scripts/land.sh >"$tmp/out" 2>&1
echo "stopped before it was queued: $? ($(ls "$tmp" | grep -c '^queued$') queued)"
rm "$tmp/slow"
# Asked again while GitHub won't say whether it was put out of the queue meanwhile: no answer
# isn't "it wasn't", and nothing is asked of it.
touch "$tmp/mute"
scripts/land.sh >"$tmp/out" 2>&1
echo "with no word on whether it was put out: $? ($(ls "$tmp" | grep -c '^queued$') queued)"
rm "$tmp/mute"
# Taken up again, and queued with an error in GitHub's answering, which it takes a look to show
# as queued: an error isn't a no unless it says so.
touch "$tmp/muddled"
echo 1 >"$tmp/late"
echo "asked again: $(scripts/land.sh 2>&1 | landed)"
rm "$tmp/muddled" "$tmp/late"
# Put out before anyone saw it in the queue: still said, not waited on for ever.
echo five >>docs/a.md
step git commit -q -am "docs: five"
touch "$tmp/gone"
scripts/land.sh >"$tmp/out" 2>&1
echo "put out unseen: $? $(grep -c 'put pull request #11 out: FAILED_CHECKS' "$tmp/out")"
rm "$tmp/gone"
# Its branch pushed to by someone else while it's in the queue: no longer this landing's. Said,
# and left in the queue, which whoever pushed may have asked for. (Here they take it out.)
touch "$tmp/moved"
scripts/land.sh >"$tmp/out" 2>&1
echo "its branch moved under it: $? $(grep -c "no longer this landing's" "$tmp/out") $(grep -c 'take it out on its page' "$tmp/out")"
rm "$tmp/moved" "$tmp/queued"
# Its own run of CI fails: it's said with that run, and nothing is queued.
touch "$tmp/red"
scripts/land.sh >"$tmp/out" 2>&1
echo "its own run of CI failing: $? $(grep -c "can't join the queue" "$tmp/out") $(grep -c 'gh run view 77 --log-failed' "$tmp/out") ($(ls "$tmp" | grep -c '^queued$') queued)"
rm "$tmp/red"
# Main moved under its pull request, which no longer applies: said at once, where its own run
# of CI would never come.
touch "$tmp/conflicting"
scripts/land.sh >"$tmp/out" 2>&1
echo "main moved under its pull request: $? $(grep -c 'no longer applies on main' "$tmp/out")"
rm "$tmp/conflicting"
noted() { ls "$tmp/main/.git/codealong-landing" | grep -c 'sent$'; }
# GitHub says no to queueing it, with why: said, and that landing is over.
touch "$tmp/refused"
scripts/land.sh >"$tmp/out" 2>&1
echo "refused a place in the queue: $? $(grep -c "wouldn't queue" "$tmp/out") ($(noted) noted)"
rm "$tmp/refused"
# No answer at all to the asking, and no sign of it in the queue after a few looks: not called
# a refusal, and still noted, to be taken up again (as the next step does).
touch "$tmp/dropped"
scripts/land.sh >"$tmp/out" 2>&1
echo "no answer to the asking: $? $(grep -c "wouldn't queue" "$tmp/out") ($(noted) noted)"
rm "$tmp/dropped"
# GitHub's yes lost, and the pull request merged by then as another commit than this one: not
# said to be queued, though it isn't waited for.
touch "$tmp/lost" "$tmp/swapped"
scripts/land.sh --no-wait >"$tmp/out" 2>&1
echo "merged as another commit: $? $(grep -c '^Queued' "$tmp/out") $(grep -c 'not the commit this landing pushed' "$tmp/out")"
rm "$tmp/lost" "$tmp/swapped"
# Another landing of this branch under way here: its turn is held, as the script holds it.
python3 -c 'import fcntl, os, sys, time
held = os.open(sys.argv[1], os.O_RDWR | os.O_CREAT, 0o644)
fcntl.flock(held, fcntl.LOCK_EX)
os.ftruncate(held, 0)
os.write(held, b"pid 4242")
open(sys.argv[1] + ".taken", "w").close()
time.sleep(300)' "$tmp/main/.git/codealong-landing/$(printf %s mine | git hash-object --stdin).turn" &
other=$!
until ls "$tmp/main/.git/codealong-landing"/*.turn.taken >/dev/null 2>&1; do sleep 0.1; done
scripts/land.sh >"$tmp/out" 2>&1
echo "while another landing of it runs: $? $(grep -c 'Another landing of mine is under way here (pid 4242)' "$tmp/out")"
kill "$other"
wait "$other" 2>/dev/null
# Without main's merge queue, asking GitHub to merge would merge unchecked.
touch "$tmp/no-queue"
# Started as the script from before would start it, holding that one's turn: let go.
sleep 300 &
old=$!
bash -c 'export CODEALONG_LANDING_HELD=$$ CODEALONG_LANDING_HOLDER="$1"; exec scripts/land.sh' land "$old" >"$tmp/out" 2>&1
echo "with no merge queue: $? $(grep -c 'has no merge queue' "$tmp/out")"
wait "$old" 2>/dev/null
[ $? -gt 128 ] && echo "and the turn of the script before this one is let go"
rm "$tmp/no-queue"
# Main has changed the same lines meanwhile: it's said, and the checkout is left as it was,
# with nothing pushed.
step git -C "$tmp/main" pull -q origin main
echo other >>"$tmp/main/docs/a.md"
step git -C "$tmp/main" commit -q -am "main, on the same lines"
step git -C "$tmp/main" push -q origin main
there="$(git -C "$tmp/origin.git" rev-parse mine)"
echo six >>docs/a.md
step git commit -q -am "docs: six"
was="$(git rev-parse HEAD)"
scripts/land.sh >"$tmp/out" 2>&1
echo "not applying on main: $? $(grep -c "doesn't apply on origin/main" "$tmp/out") $(grep -c '^CONFLICT' "$tmp/out")"
[ "$(git rev-parse HEAD)" = "$was" ] && [ -z "$(git status --porcelain)" ] && [ "$(git -C "$tmp/origin.git" rev-parse mine)" = "$there" ] && echo "and it's left as it was, here and on origin"
# A branch whose first commit has landed, as the queue's copy of it, with one on top that
# hasn't: rebased first, which drops the copy.
step git worktree add -q "$tmp/half" -b half origin/main
step cd "$tmp/half"
here
echo h >docs/h.md
step git add -A
step git commit -q -m "half: one"
step env GIT_COMMITTER_NAME=queue git -C "$tmp/main" cherry-pick "$(git rev-parse HEAD)" >/dev/null
step git -C "$tmp/main" push -q origin main
echo i >docs/i.md
step git add -A
step git commit -q -m "half: two"
scripts/land.sh >"$tmp/out" 2>&1
echo "half of it landed already: $? $(grep -c 'come to nothing and drop out' "$tmp/out")"
# A change that differs from one main has only by its whitespace is still a change: git takes
# the two patches for one, and a landing that went by that would call it landed and drop it.
step git worktree add -q "$tmp/spaced" -b spaced origin/main
step cd "$tmp/spaced"
here
echo 'say("a b")' >docs/s.md
step git add -A
step git commit -q -m "spaced: a b"
echo 'say("ab")' >"$tmp/main/docs/s.md"
step git -C "$tmp/main" add -A
step git -C "$tmp/main" commit -q -m "main: ab"
step git -C "$tmp/main" push -q origin main
was="$(git rev-parse HEAD)"
scripts/land.sh >"$tmp/out" 2>&1
echo "differing from main's only by a space: $? $(grep -c 'Nothing to land' "$tmp/out") $(grep -c "doesn't apply on origin/main" "$tmp/out") $([ "$(git rev-parse HEAD)" = "$was" ] && echo kept)"
# A branch that adds a file and takes it away again, where main has since been given the same
# file from elsewhere: merging it would change nothing, so there's nothing to land, and the
# branch goes to main as it is. A rebase of its commits would have skipped the adding for
# main's and written the taking away on top of main, one landing from taking main's file away.
step git worktree add -q "$tmp/undone" -b undone origin/main
step cd "$tmp/undone"
here
echo u >docs/u.md
step git add -A
step git commit -q -m "undone: add"
step git rm -q docs/u.md
step git commit -q -m "undone: take away"
echo u >"$tmp/main/docs/u.md"
step git -C "$tmp/main" add -A
step git -C "$tmp/main" commit -q -m "main: the same file"
step git -C "$tmp/main" push -q origin main
scripts/land.sh >"$tmp/out" 2>&1
echo "adding and taking away what main has since: $? $(grep -c '^Nothing to land' "$tmp/out") $([ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] && [ -f docs/u.md ] && echo at main, with its file)"
# Another session commits in this checkout just as the branch is caught up: slipped in here
# before git's rebase starts, by a git that does that and then what it was asked. Its commit
# is one git would take for one of main's by its patch (the same file, a space apart) and skip.
# It's carried onto main instead, where it stops on main's file, and that's said; nothing is
# skipped, put back or given up, and the commit is still there.
step git worktree add -q "$tmp/shared" -b shared origin/main
step cd "$tmp/shared"
here
echo s >docs/shared.md
step git add -A
step git commit -q -m "shared: one"
echo 'say("ab")' >"$tmp/main/docs/late.md"
step git -C "$tmp/main" add -A
step git -C "$tmp/main" commit -q -m "main: late"
step git -C "$tmp/main" push -q origin main
step mkdir "$tmp/shim"
real="$(command -v git)"
cat >"$tmp/shim/git" <<GIT
#!/bin/bash
for word in "\\$@"; do
  if [ "\\$word" = rebase ] && [ -f "$tmp/meanwhile" ]; then
    rm "$tmp/meanwhile"
    echo 'say("a b")' >docs/late.md
    "$real" add docs/late.md
    "$real" commit -q -m "another session's, meanwhile"
  fi
done
exec "$real" "\\$@"
GIT
step chmod +x "$tmp/shim/git"
touch "$tmp/meanwhile"
PATH="$tmp/shim:$PATH" scripts/land.sh >"$tmp/out" 2>&1
echo "a commit made here as it's caught up: $? $(grep -c '^Landed' "$tmp/out") $(grep -c 'A rebase is under way' "$tmp/out")"
git rebase --abort 2>/dev/null
echo "and is still here: $(git log --format=%s -1)"
# GitHub says it merged before the main fetched here has the commit, main having moved since
# the branch was made: nothing here is touched (moved onto that main, the branch would be
# other commits than the ones noted), it's still noted, and asked again once main has it, it
# lands as any other.
step git worktree add -q "$tmp/early" -b early origin/main
step cd "$tmp/early"
here
echo e >docs/early.md
step git add -A
step git commit -q -m "early: one"
echo m >"$tmp/main/docs/m.md"
step git -C "$tmp/main" add -A
step git -C "$tmp/main" commit -q -m "main: meanwhile"
step git -C "$tmp/main" push -q origin main
was="$(git rev-parse HEAD)"
touch "$tmp/unseen"
scripts/land.sh >"$tmp/out" 2>&1
echo "merged, by GitHub's word, before main here has it: $? $(grep -c 'still noted' "$tmp/out") $([ "$(git rev-parse HEAD)" = "$was" ] && echo untouched)"
rm "$tmp/unseen"
step git -C "$tmp/queue" push -q origin main :early
echo "asked again: $(scripts/land.sh 2>&1 | landed)"
step cd "$tmp/main"
scripts/land.sh >"$tmp/out" 2>&1
echo "from main: $(grep -c 'is on main' "$tmp/out")"
echo "main, at the end: $(onmain)"
echo "tags on origin: $(git -C "$tmp/origin.git" tag | wc -l | tr -d ' ')"
`;

test('a landing queues its pull request as the commit that was asked for, on a main that moved, and brings this branch and the main checkout up', { timeout: LANDING_WAIT + 20_000 }, async () => {
  const went = await run('bash', ['-c', LANDING, 'landing', script, catchUp], LANDING_WAIT);
  expect(went.status, went.out + went.err).toBe(0);
  expect(went.out.trim().split('\n')).toEqual([
    'landing: Landed: <commit> is in origin/main. Fast-forwarded the main checkout to origin/main (<commit>).',
    'main: docs: two,main moved,first,',
    'the main checkout: docs: two',
    'this branch is where main is, as the queue wrote it',
    'and a second branch at the commit it was at is still there',
    'its pull request: docs: two, 1 word on its review, 1 line for CI',
    'queued as the commit it pushed, and no other: 1',
    "origin's branches after: main ",
    // Neither of these gets as far as a push or a pull request.
    'code, with no word on its review: 2 (7 pull requests so far)',
    "with a file git doesn't know of: 1",
    'put out of the queue: 1 1 1',
    'main, after: docs: two,main moved,first,',
    "set to merge by itself: 1 1, origin's still a script",
    'landing again: Landed: <commit> is in origin/main. Fast-forwarded the main checkout to origin/main (<commit>). Reviewed by: an account, a model.',
    'main: a script, fixed,a script,main moved again,docs: two,main moved,first, (8 pull requests)',
    "over someone else's branch: 1 1, theirs still theirs",
    'checked: Checked, not landed: <commit> passed CI (https://github.com/acme/northwind/actions/runs/55).',
    "origin's branches after: main theirs , with 0 tags",
    "queued and left: Queued: pull request #9 (https://github.com/acme/northwind/pull/9) lands <commit> when main's merge queue has checked it.",
    "asked again, still queued: Queued still: pull request #9 (https://github.com/acme/northwind/pull/9) lands <commit> when main's merge queue has checked it.",
    'asked again: Landed: <commit> is in origin/main. Fast-forwarded the main checkout to origin/main (<commit>).',
    'and this branch is where main is',
    'stopped before it was queued: 3 (0 queued)',
    'with no word on whether it was put out: 3 (0 queued)',
    'asked again: Landed: <commit> is in origin/main. Fast-forwarded the main checkout to origin/main (<commit>).',
    'put out unseen: 1 1',
    'its branch moved under it: 1 1 1',
    'its own run of CI failing: 1 1 1 (0 queued)',
    'main moved under its pull request: 1 1',
    'refused a place in the queue: 1 1 (0 noted)',
    'no answer to the asking: 3 0 (1 noted)',
    'merged as another commit: 1 0 1',
    'while another landing of it runs: 1 1',
    'with no merge queue: 1 1',
    'and the turn of the script before this one is let go',
    'not applying on main: 1 1 1',
    "and it's left as it was, here and on origin",
    'half of it landed already: 1 1',
    "differing from main's only by a space: 1 0 1 kept",
    'adding and taking away what main has since: 0 1 at main, with its file',
    "a commit made here as it's caught up: 0 1 1",
    "and is still here: another session's, meanwhile",
    "merged, by GitHub's word, before main here has it: 3 1 untouched",
    'asked again: Landed: <commit> is in origin/main. Fast-forwarded the main checkout to origin/main (<commit>).',
    'from main: 1',
    'main, at the end: early: one,main: meanwhile,shared: one,main: late,main: the same file,main: ab,half: one,main, on the same lines,docs: four,docs: three,a script, fixed,a script,main moved again,docs: two,main moved,first,',
    'tags on origin: 0',
  ]);
});

test('the landing test stops at a setup step that fails, where it began', { timeout: SEVERAL }, async () => {
  // With no script to copy in, the copy fails: nothing after it runs, least of all a commit.
  const went = await run('bash', ['-c', LANDING, 'landing', '/nowhere/land.sh', '/nowhere/catch-up-main.sh']);
  expect(went.status).toBe(9);
  expect(went.out).toContain('setup failed: cp /nowhere/land.sh');
  expect(went.out).not.toContain('landing:');
});
