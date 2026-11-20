// scripts/checks.sh and the made-up Mac's side of scripts/preview.sh: what a run counts as,
// which runs get another go, what --base makes of a failure, and that nothing of this Mac's
// ends up in the made-up Mac's page. Each is run for real, in a folder made for the test, over
// stand-ins for what it starts (an engine, vite's build, and for checks.sh a preview.sh that
// says what each run is told to), so no browser, no build and no board is needed. It's here
// rather than beside the scripts so that typecheck and lint read it, as land.test.ts is.
// The project has no types for node (the tests are the only code that runs there).
// @ts-expect-error no @types/node
import { spawn } from 'node:child_process';
// @ts-expect-error no @types/node
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

// Decoded, for a checkout whose path has a space in it: a URL's pathname keeps it as %20.
const checks: string = fileURLToPath(new URL('../scripts/checks.sh', import.meta.url));
const preview: string = fileURLToPath(new URL('../scripts/preview.sh', import.meta.url));

type Run = { status: number | null; out: string; err: string };

// How long a scenario may take before it's taken for hung: for a script that never ends, not
// for one that's slow (land.test.ts, on what a busy Mac does to these).
const WAIT = 600_000;

// Under a deadline of its own, and its process group killed after, as land.test.ts runs its
// script: the test's timer can't reach a script that hangs, and a set of checks killed part way
// leaves a watcher behind.
function bash(script: string, args: string[]): Promise<Run> {
  return new Promise((done, fail) => {
    const child = spawn('bash', ['-c', script, 'scenario', ...args], { detached: true });
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
      fail(new Error(`the scenario was still running after ${WAIT} ms: ${out}${err}`));
    }, WAIT);
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

/** The lines a scenario printed, which is what it found. */
async function said(script: string, ...args: string[]): Promise<string[]> {
  const went = await bash(script, args);
  expect(went.status, went.out + went.err).toBe(0);
  return went.out.trim().split('\n');
}

// A folder of the test's own, with a repository in it, and nothing of whoever runs the test:
// not its git settings or hooks (land.test.ts says why git's own variables are cleared first),
// not a PREVIEW_* variable, and its temporary files inside it, so a set of checks killed part
// way leaves nothing behind. Every step of the setup stops the test if it fails.
const FOLDER = `
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
for var in $(compgen -v PREVIEW_) CHECKS_SET SEEN BUILT INSTALLED FAIL_AT; do unset "$var"; done
step mkdir -p "$tmp/bin" "$tmp/t" "$tmp/repo/scripts/preview"
export TMPDIR="$tmp/t" PATH="$tmp/bin:$PATH"
step cd "$tmp/repo"
case "$(pwd -P)" in "$tmp"/*) ;; *) echo "not in the test's own folder: $(pwd -P)"; exit 9 ;; esac
# vite's build, without vite: a page in dist/, and a word on whose packages it was built with.
cat >"$tmp/bin/npx" <<'EOF'
#!/usr/bin/env bash
mkdir -p dist && echo page >dist/index.html
[ -z "\${BUILT:-}" ] || echo "built with $([ -L node_modules ] && echo "this checkout's packages" || echo "its own")" >>"$BUILT"
EOF
cat >"$tmp/bin/npm" <<'EOF'
#!/usr/bin/env bash
mkdir -p node_modules
[ -z "\${INSTALLED:-}" ] || echo "npm $1" >>"$INSTALLED"
EOF
step chmod +x "$tmp/bin/npx" "$tmp/bin/npm"
`;

// preview.sh itself, with two engines to run: one that has a made-up Mac and says of everything
// it prints which Mac it's of (FAIL_AT names a snapshot it fails at), and one from before there
// was one, which prints this Mac's whatever it's asked.
const PREVIEW = `${FOLDER}
step cp "$1" scripts/preview.sh
cat >"$tmp/bin/engine" <<'EOF'
#!/usr/bin/env bash
if [ "\${2:-}" != --fixture ]; then
  echo '{"lanes": [{"path": "/Users/real/app"}], "of": "REAL"}'
  exit 0
fi
case "\${3:-board}" in
  name) echo "the made-up Mac" ;;
  "\${FAIL_AT:-}") echo "couldn't" >&2; exit 1 ;;
  *) echo '{"lanes": [{"path": "/Users/me/projects/northwind-app"}], "of": "made up"}' ;;
esac
EOF
cat >"$tmp/bin/old-engine" <<'EOF'
#!/usr/bin/env bash
echo '{"lanes": [{"path": "/Users/real/app"}], "of": "REAL"}'
EOF
step chmod +x "$tmp/bin/engine" "$tmp/bin/old-engine"
# How many files of a folder hold this Mac's, and which Mac the folder says it's of.
real() { grep -rl REAL "$1" 2>/dev/null | wc -l | tr -d ' '; }
of() { cat "$1/snapshot-of" 2>/dev/null || echo nothing; }
refused() { grep -c "$1" "$tmp/err"; }

PREVIEW_FIXTURE=1 PREVIEW_PREPARE=only PREVIEW_BIN="$tmp/bin/old-engine" scripts/preview.sh >/dev/null 2>"$tmp/err"
echo "an engine from before it had a made-up Mac: $?, refused $(refused 'has no made-up Mac'), $(real .preview) of this Mac's kept"

PREVIEW_PREPARE=only PREVIEW_BIN="$tmp/bin/engine" scripts/preview.sh >/dev/null 2>&1
echo "this Mac: $?, a folder of $(of .preview)"
PREVIEW_FIXTURE=1 PREVIEW_PREPARE=only PREVIEW_BIN="$tmp/bin/engine" scripts/preview.sh >/dev/null 2>&1
echo "the made-up Mac: $?, a folder of $(of .preview/fixture) with $(real .preview/fixture) of this Mac's, beside $(of .preview)'s"

PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip PREVIEW_DIR=.preview scripts/preview.sh >/dev/null 2>"$tmp/err"
echo "the made-up Mac, from this Mac's folder: $?, refused $(refused "holds the snapshots of this Mac, not of the made-up Mac")"
PREVIEW_PREPARE=skip PREVIEW_DIR=.preview/fixture scripts/preview.sh >/dev/null 2>"$tmp/err"
echo "this Mac, from the made-up Mac's folder: $?, refused $(refused "holds the snapshots of the made-up Mac, not of this Mac")"
PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip PREVIEW_TRANSCRIPT=claude:claude:checkout-reply PREVIEW_BIN="$tmp/bin/old-engine" scripts/preview.sh >/dev/null 2>"$tmp/err"
echo "a conversation, asked of the engine from before: $?, refused $(refused 'has no made-up Mac'), $(real .preview/fixture) of this Mac's kept"

# One folder for both, which PREVIEW_DIR can ask for, and a made-up Mac that stops part way.
export PREVIEW_DIR="$tmp/one"
PREVIEW_PREPARE=only PREVIEW_BIN="$tmp/bin/engine" scripts/preview.sh >/dev/null 2>&1
echo "one folder, this Mac first: $(real "$tmp/one") of this Mac's"
FAIL_AT=config PREVIEW_FIXTURE=1 PREVIEW_PREPARE=only PREVIEW_BIN="$tmp/bin/engine" scripts/preview.sh >/dev/null 2>&1
echo "then the made-up Mac, stopped part way: $?, a folder of $(of "$tmp/one") with $(real "$tmp/one") of this Mac's"
PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip scripts/preview.sh >/dev/null 2>"$tmp/err"
echo "rendered as the made-up Mac: $?, refused $(refused 'holds the snapshots of a run that stopped')"
PREVIEW_PREPARE=skip scripts/preview.sh >/dev/null 2>"$tmp/err"
echo "rendered as this Mac: $?, refused $(refused 'holds the snapshots of a run that stopped')"
`;

test('nothing of this Mac’s is shown as the made-up Mac’s', { timeout: WAIT + 20_000 }, async () => {
  expect(await said(PREVIEW, preview)).toEqual([
    // An engine that takes --fixture for nothing prints this Mac's board, and none of it is kept.
    "an engine from before it had a made-up Mac: 1, refused 1, 0 of this Mac's kept",
    'this Mac: 0, a folder of this Mac',
    // In a folder of its own, beside this Mac's and not over it.
    "the made-up Mac: 0, a folder of the made-up Mac with 0 of this Mac's, beside this Mac's",
    "the made-up Mac, from this Mac's folder: 1, refused 1",
    "this Mac, from the made-up Mac's folder: 1, refused 1",
    "a conversation, asked of the engine from before: 1, refused 1, 0 of this Mac's kept",
    // The snapshots a run didn't get to are this Mac's from the run before, unless they're
    // taken away first; and the folder is the made-up Mac's only once it has every one.
    "one folder, this Mac first: 10 of this Mac's",
    "then the made-up Mac, stopped part way: 1, a folder of a run that stopped before it had them all with 0 of this Mac's",
    'rendered as the made-up Mac: 1, refused 1',
    'rendered as this Mac: 1, refused 1',
  ]);
});

// checks.sh itself, over a preview.sh that says of each run what scripts/preview/outcomes has
// for it: "<check> <view>[-<scheme>]: <its first go> | <its second> | …", the last for every go
// after, and a pass for a run it doesn't name. A go is PASS, FAIL <what failed>, NONE (the check
// never said how it went) or DIE (the set of checks it's in is killed, before it writes what
// it found). Four checks, each with a run in light and one in dark, and the lot committed: the
// commit is --base's.
const CHECKS = `${FOLDER}
step cp "$1" scripts/checks.sh
cat >scripts/preview.sh <<'EOF'
#!/usr/bin/env bash
cd "$(dirname "$0")/.." || exit 1
dir="\${PREVIEW_DIR:-.preview}"
got="$(env | grep '^PREVIEW_' | sort | tr '\\n' ' ' | sed 's/ $//')"
if [ "\${PREVIEW_PREPARE:-}" = only ]; then
  mkdir -p "$dir" && echo '{}' >"$dir/board.json"
  [ -z "\${SEEN:-}" ] || echo "the build and the snapshots: $got" >>"$SEEN"
  exit 0
fi
if [ -n "\${PREVIEW_BIN:-}" ] && [ ! -x "$PREVIEW_BIN" ]; then
  echo "No engine at $PREVIEW_BIN (PREVIEW_BIN)." >&2
  exit 1
fi
run="$(basename "$3" -check.js) $1\${PREVIEW_SCHEME:+-$PREVIEW_SCHEME}"
[ -z "\${SEEN:-}" ] || echo "$run: $got" >>"$SEEN"
mkdir -p .goes
echo x >>".goes/$run"
go="$(($(wc -l <".goes/$run")))"
says="$(sed -n "s/^$run: //p" scripts/preview/outcomes 2>/dev/null)"
says="$(echo "\${says:-PASS}" | awk -F' [|] ' -v go="$go" '{ print $(go < NF ? go : NF) }')"
case "$says" in
  PASS) echo "PASS 2/2" ;;
  NONE)
    echo "FAIL 0/0 no result: the check never said done"
    exit 1
    ;;
  DIE)
    pid=$PPID
    while [ "\${pid:-1}" -gt 1 ]; do
      case "$(ps -o command= -p "$pid")" in *scripts/checks.sh*) kill -9 "$pid"; break ;; esac
      pid="$(ps -o ppid= -p "$pid" | tr -d ' ')"
    done
    exit 1
    ;;
  *)
    echo "FAIL 1/2 \${says#FAIL }"
    exit 1
    ;;
esac
EOF
step chmod +x scripts/preview.sh
light() { echo "scripts/preview.sh sessions '' scripts/preview/$1-check.js"; }
dark() { echo "PREVIEW_SCHEME=dark scripts/preview.sh sessions '' scripts/preview/$1-check.js"; }
# A check's file: its header, with the run lines given.
check() {
  local name="$1"
  shift
  { echo "// $name-check.js"; printf '//   %s\\n' "$@"; echo "done();"; } >"scripts/preview/$name-check.js"
}
for name in alpha beta gamma delta; do
  check "$name" "$(light "$name")" "$(dark "$name")"
done
# What the runs say from here on, each from its first go.
says() {
  printf '%s\\n' "$@" >scripts/preview/outcomes
  rm -rf .goes
}
goes() { echo "$(($(wc -l <".goes/$1")))"; }
count() { grep -E '^[0-9]+ passed' "$tmp/out"; }
echo one >package-lock.json
printf '.preview\\n.goes\\ndist\\nnode_modules\\nmine\\n' >.gitignore
`;

const COMMITTED = `
step git init -q
step git add -A
step git commit -q -m base
`;

test('a run that gave no result goes again by itself, and its check’s other runs keep what they said', { timeout: WAIT + 20_000 }, async () => {
  const lines = await said(
    `${CHECKS}
says "alpha sessions: FAIL saves: nothing saved | PASS" "alpha sessions-dark: NONE | PASS"
scripts/checks.sh alpha >"$tmp/out" 2>&1
echo "$?: $(count) Light went $(goes 'alpha sessions'), dark $(goes 'alpha sessions-dark')."
`,
    checks,
  );
  // Had light gone again with dark, its second go's pass would have taken its failure away.
  expect(lines).toEqual(['1: 1 passed, 1 failed. Light went 1, dark 2.']);
});

test('the made-up Mac’s runs get nothing from whoever starts them, and the gate’s list holds only what it can run', { timeout: WAIT + 20_000 }, async () => {
  const lines = await said(
    `${CHECKS}
says
{ light alpha; dark alpha; } >scripts/preview/fixture-passes.txt
SEEN="$tmp/seen" PREVIEW_PLATFORM=linux PREVIEW_SCHEME=dark PREVIEW_SCHEDULES=s.json PREVIEW_SETUP=u.json PREVIEW_ANALYTICS=1 PREVIEW_IMAGES=/pictures scripts/checks.sh --fixture >"$tmp/out" 2>&1
echo "the gate: $?: $(count)"
cat "$tmp/seen"
rm "$tmp/seen"
SEEN="$tmp/seen" PREVIEW_SCHEME=dark scripts/checks.sh beta >"$tmp/out" 2>&1
echo "this Mac's data: $?: $(count)"
cat "$tmp/seen"

check zeta "PREVIEW_TRANSCRIPT=<key> scripts/preview.sh sessions '' scripts/preview/zeta-check.js"
echo "PREVIEW_TRANSCRIPT=<key> scripts/preview.sh sessions '' scripts/preview/zeta-check.js" >>scripts/preview/fixture-passes.txt
scripts/checks.sh --fixture >"$tmp/out" 2>&1
echo "a run made by hand, listed: $?: $(count) $(grep -c "lists a run that's made by hand" "$tmp/out") said so."

{ light alpha; echo "PREVIEW_SCHEME=sepia scripts/preview.sh sessions '' scripts/preview/alpha-check.js"; light beta; } >scripts/preview/fixture-passes.txt
scripts/checks.sh --fixture --listed alpha >"$tmp/out" 2>&1
echo "a listed run its check has lost, the check named: $?: $(count) $(grep -c 'lists a run no check has' "$tmp/out") said so."
scripts/checks.sh --fixture --listed beta >"$tmp/out" 2>&1
echo "another check named: $?: $(count)"

rm -rf .goes
scripts/checks.sh --fixture --base alpha >"$tmp/out" 2>&1
echo "--base on the made-up Mac: $?, $(grep -c 'not with --fixture' "$tmp/out") said so, $(ls .goes 2>/dev/null | wc -l | tr -d ' ') runs."
`,
    checks,
  );
  expect(lines).toEqual([
    'the gate: 0: 2 passed, 0 failed.',
    // In the made-up Mac's own folder, and with nothing but what a run's own line sets: the
    // platform would have had every run check Linux's keys, and the scheme run light in dark.
    'the build and the snapshots: PREVIEW_DIR=.preview/fixture PREVIEW_FIXTURE=1 PREVIEW_PREPARE=only',
    'alpha sessions: PREVIEW_DIR=.preview/fixture PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip',
    'alpha sessions-dark: PREVIEW_DIR=.preview/fixture PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip PREVIEW_SCHEME=dark',
    // On real data a scheme asked for still reaches every run, as the script's header says.
    "this Mac's data: 0: 2 passed, 0 failed.",
    'the build and the snapshots: PREVIEW_DIR=.preview PREVIEW_PREPARE=only PREVIEW_SCHEME=dark',
    'beta sessions-dark: PREVIEW_DIR=.preview PREVIEW_PREPARE=skip PREVIEW_SCHEME=dark',
    'beta sessions-dark: PREVIEW_DIR=.preview PREVIEW_PREPARE=skip PREVIEW_SCHEME=dark',
    // Not run, it would pass the gate every time.
    'a run made by hand, listed: 1: 2 passed, 1 failed. 1 said so.',
    'a listed run its check has lost, the check named: 1: 1 passed, 1 failed. 1 said so.',
    'another check named: 0: 1 passed, 0 failed.',
    '--base on the made-up Mac: 2, 1 said so, 0 runs.',
  ]);
});

test('--base clears only a failure the base’s check words the same', { timeout: WAIT + 20_000 }, async () => {
  const lines = await said(
    `${CHECKS}
says "alpha sessions: FAIL saves: nothing saved" "beta sessions: FAIL saves: nothing saved; opens: no row" "gamma sessions: FAIL saves: nothing saved"
# An engine this checkout has and the base's folder doesn't, named from here as PREVIEW_BIN may.
step mkdir mine
step cp "$tmp/bin/npx" mine/engine
${COMMITTED}
base="$(git rev-parse --short HEAD)"
# What --base says of each failure, without the runs' own lines and with one space for many.
verdicts() { sed -n '/^Trying the .* that failed on/,$p' "$tmp/out" | grep -v -e '^    [A-Z]' -e '^$' -e '^Trying' | sed "s/$base/<base>/g" | tr -s ' '; }

says "alpha sessions: NONE" "beta sessions: FAIL saves: nothing saved" "gamma sessions: FAIL saves: nothing saved; closes: still open" "delta sessions: FAIL opens: no row"
PREVIEW_BIN=mine/engine BUILT="$tmp/built" INSTALLED="$tmp/installed" scripts/checks.sh --base=HEAD >"$tmp/out" 2>&1
echo "$?: $(count)"
verdicts
cat "$tmp/built"
echo "installed: $(cat "$tmp/installed" 2>/dev/null)"

says "beta sessions: FAIL saves: nothing saved"
echo two >package-lock.json
: >"$tmp/built"
BUILT="$tmp/built" INSTALLED="$tmp/installed" scripts/checks.sh --base=HEAD >"$tmp/out" 2>&1
echo "$?: $(count)"
verdicts
cat "$tmp/built"
echo "installed: $(cat "$tmp/installed" 2>/dev/null)"
`,
    checks,
  );
  expect(lines).toEqual([
    '1: 4 passed, 4 failed.',
    // The base's failure covers this one only when everything that fails here fails so there.
    'fails on <base> too beta sessions',
    'FAIL here only delta sessions',
    'FAIL, not as on <base> gamma sessions',
    ' here only: closes: still open',
    // No result twice says nothing of the check, whatever the base's run of it said.
    'FAIL, no result here alpha sessions',
    '4 failed: 1 on <base> too, 3 not.',
    // The same packages as the base asks for: this checkout's, and the engine by its whole path
    // (the base's runs would have had no engine, and so no result, by the path it was given).
    "built with this checkout's packages",
    'installed: ',
    '0: 7 passed, 1 failed.',
    // Other packages than the base asks for: its UI is built with its own, or a failure an
    // update to one made would fail on both and pass for the data's.
    ' <base> asks for other packages than this checkout has: installing its own…',
    'fails on <base> too beta sessions',
    '1 failed: 1 on <base> too, 0 not.',
    'built with its own',
    'installed: npm ci',
  ]);
});

test('a set of checks that stops short is a failure --base can see', { timeout: WAIT + 20_000 }, async () => {
  const lines = await said(
    `${CHECKS}
says
${COMMITTED}
says "beta sessions: DIE | NONE"
scripts/checks.sh --jobs=2 --base=HEAD >"$tmp/out" 2>&1
echo "$?: $(count)"
echo "$(grep -c 'a set of checks stopped short (beta gamma' "$tmp/out") said so; beta went $(goes 'beta sessions') and $(goes 'beta sessions-dark'), gamma $(goes 'gamma sessions') and $(goes 'gamma sessions-dark')."
grep -e 'no result here' -e 'failed: ' "$tmp/out" | sed "s/$(git rev-parse --short HEAD)/<base>/g" | tr -s ' '
`,
    checks,
  );
  expect(lines).toEqual([
    // The set (beta and gamma, every other check) left no results: its checks go again, and
    // the run that gives none then stands as a failure, where it was once counted with no run
    // to its name and so cleared by --base.
    '1: 7 passed, 1 failed.',
    '1 said so; beta went 2 and 1, gamma 1 and 1.',
    'FAIL, no result here beta sessions',
    '1 failed: 0 on <base> too, 1 not.',
  ]);
});
