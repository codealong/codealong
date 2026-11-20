// scripts/checks.sh and the made-up Mac's side of scripts/preview.sh: what a run counts as,
// which runs get another go, what --base makes of a failure, and that nothing of this Mac's
// ends up in the made-up Mac's page. Each is run for real, in a folder made for the test, over
// stand-ins for what it starts (an engine, vite's build, and for checks.sh a preview.sh that
// says what each run is told to), so no browser, no build, no board and no socket is needed:
// every landing runs this, some of them in a sandbox that lets a program listen on nothing. It's
// here rather than beside the scripts so that typecheck and lint read it, as land.test.ts is.
// The project has no types for node (the tests are the only code that runs there).
// @ts-expect-error no @types/node
import { spawn } from 'node:child_process';
// @ts-expect-error no @types/node
import { readFileSync } from 'node:fs';
// @ts-expect-error no @types/node
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

// Decoded, for a checkout whose path has a space in it: a URL's pathname keeps it as %20.
const checks: string = fileURLToPath(new URL('../scripts/checks.sh', import.meta.url));
const preview: string = fileURLToPath(new URL('../scripts/preview.sh', import.meta.url));
const clock: string = fileURLToPath(new URL('../scripts/preview/clock.js', import.meta.url));
const prelude: string = fileURLToPath(new URL('../scripts/preview/prelude.js', import.meta.url));
const text = (file: string): string => readFileSync(file, 'utf8');

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
// was one, which prints this Mac's whatever it's asked. And a python3 that opens no socket, as
// in a sandbox: nothing here gets as far as a page, so nothing here may ask for a port.
const PREVIEW = `${FOLDER}
step cp "$1" scripts/preview.sh
REAL_PYTHON="$(command -v python3)" || exit 9
export REAL_PYTHON
cat >"$tmp/bin/python3" <<'EOF'
#!/usr/bin/env bash
case "$*" in *socket*) echo "PermissionError: [Errno 1] Operation not permitted" >&2; exit 1 ;; esac
exec "$REAL_PYTHON" "$@"
EOF
step chmod +x "$tmp/bin/python3"
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
echo "this Mac, from the made-up Mac's folder: $?, refused $(refused "holds the snapshots of the made-up Mac, as its engine named it, not of this Mac")"
# A folder marked as the made-up Mac's before an engine was asked for its name, which an engine
# from before could have filled with this Mac's.
step mkdir "$tmp/before"
echo '{"of": "REAL"}' >"$tmp/before/board.json"
echo "the made-up Mac" >"$tmp/before/snapshot-of"
PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip PREVIEW_DIR="$tmp/before" scripts/preview.sh >/dev/null 2>"$tmp/err"
echo "the made-up Mac, from a folder marked before engines were asked: $?, refused $(refused "holds the snapshots of the made-up Mac, not of the made-up Mac, as its engine named it")"
PREVIEW_DATE=soon PREVIEW_FIXTURE=1 PREVIEW_PREPARE=only PREVIEW_BIN="$tmp/bin/engine" scripts/preview.sh >/dev/null 2>"$tmp/err"
echo "a day that isn't one: $?, refused $(refused 'PREVIEW_DATE is a day')"
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
    "the made-up Mac: 0, a folder of the made-up Mac, as its engine named it with 0 of this Mac's, beside this Mac's",
    "the made-up Mac, from this Mac's folder: 1, refused 1",
    "this Mac, from the made-up Mac's folder: 1, refused 1",
    // The bare name was once written over whatever an engine printed, so it says nothing now.
    'the made-up Mac, from a folder marked before engines were asked: 1, refused 1',
    "a day that isn't one: 1, refused 1",
    "a conversation, asked of the engine from before: 1, refused 1, 0 of this Mac's kept",
    // The snapshots a run didn't get to are this Mac's from the run before, unless they're
    // taken away first; and the folder is the made-up Mac's only once it has every one.
    "one folder, this Mac first: 10 of this Mac's",
    "then the made-up Mac, stopped part way: 1, a folder of a run that stopped before it had them all with 0 of this Mac's",
    'rendered as the made-up Mac: 1, refused 1',
    'rendered as this Mac: 1, refused 1',
  ]);
});

// preview.sh as far as its server: with a python3 whose server can't have the port (another
// has it, as when PREVIEW_PORT names one in use, or a free one was taken before this run bound
// it) and a curl that answers as that other server would. No socket is opened for it either.
const SERVED = `${FOLDER}
step cp "$1" scripts/preview.sh
REAL_PYTHON="$(command -v python3)" || exit 9
export REAL_PYTHON
cat >"$tmp/bin/python3" <<'EOF'
#!/usr/bin/env bash
case "$*" in
  *socket*) echo 45678 ;;
  *http.server*) echo "OSError: [Errno 48] Address already in use" >&2; exit 1 ;;
  *) exec "$REAL_PYTHON" "$@" ;;
esac
EOF
cat >"$tmp/bin/curl" <<'EOF'
#!/usr/bin/env bash
echo "<title>done: PASS 9/9</title> another run's page"
EOF
cat >"$tmp/bin/engine" <<'EOF'
#!/usr/bin/env bash
case "\${3:-board}" in
  name) echo "the made-up Mac" ;;
  *) echo '{"lanes": []}' ;;
esac
EOF
step chmod +x "$tmp/bin/python3" "$tmp/bin/curl" "$tmp/bin/engine"
for file in fake-tauri.js prelude.js some-check.js; do
  echo "// $file" >"scripts/preview/$file"
done
step env PREVIEW_FIXTURE=1 PREVIEW_PREPARE=only PREVIEW_BIN="$tmp/bin/engine" scripts/preview.sh >/dev/null 2>&1
refused() { grep -c "$1" "$tmp/err"; }
PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip scripts/preview.sh sessions '' scripts/preview/some-check.js >"$tmp/out" 2>"$tmp/err"
echo "a free port another server had first: $?, refused $(refused 'No port for this run to serve its page on'), said $(wc -c <"$tmp/out" | tr -d ' ') bytes of a result"
PREVIEW_PORT=8123 PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip scripts/preview.sh sessions '' scripts/preview/some-check.js >"$tmp/out" 2>"$tmp/err"
echo "a port asked for that another server has: $?, refused $(refused "Port 8123 (PREVIEW_PORT) isn't this run's"), said $(wc -c <"$tmp/out" | tr -d ' ') bytes of a result"
echo "left in the folder: $(ls .preview/fixture | grep -c '^served-')"
`;

test('a page is opened only from this run’s own server', { timeout: WAIT + 20_000 }, async () => {
  expect(await said(SERVED, preview)).toEqual([
    // The other server's page says PASS: read for this run's, it would be this check's result,
    // and on the made-up Mac's run it could be a page of this Mac's.
    'a free port another server had first: 1, refused 1, said 0 bytes of a result',
    'a port asked for that another server has: 1, refused 1, said 0 bytes of a result',
    'left in the folder: 0',
  ]);
});

// preview.sh as far as Chrome, which here is a stand-in that does what Chrome does to a busy Mac:
// it starts two helpers that are still there when the browser has gone, one of which doesn't go
// when asked. The process table is a stand-in too: it lists the helpers as Chrome's are, started
// from where Chrome is with the run's profile in their arguments, and bystanders that name the
// profile otherwise (a longer path, another command naming the folder, the profile inside
// another argument, someone looking for the very argument), which have to be left alone. It
// lists only what's still there, as the real one does. curl stands in for this run's server and
// for Chrome's debugging endpoint, which says the check passed once Chrome is up. No socket is
// opened and the real process table isn't asked. Each of these processes holds a lock for as
// long as it lives, which is how one that's there is told from one that's gone (a process that
// has ended holds none, reaped or not), and ends by itself once the test's folder is gone.
const LEFT = `${FOLDER}
step cp "$1" scripts/preview.sh
REAL_PYTHON="$(command -v python3)" || exit 9
export REAL_PYTHON FAKE="$tmp"
cat >"$tmp/bin/python3" <<'EOF'
#!/usr/bin/env bash
case "$*" in
  *socket*) echo 45678 ;;
  *http.server*) exec sleep 120 ;;
  *) exec "$REAL_PYTHON" "$@" ;;
esac
EOF
cat >"$tmp/bin/curl" <<'EOF'
#!/usr/bin/env bash
for last; do :; done
case "$last" in
  */json/list)
    title=loading
    [ ! -s "$FAKE/ready" ] || title="\${SAYS:-done: PASS 1/1}"
    echo "[{\\"type\\": \\"page\\", \\"title\\": \\"$title\\"}]"
    ;;
  *) cat ".preview/fixture/\${last##*/}" 2>/dev/null ;;
esac
EOF
# Whether the lock $1 (a name in the test's folder) is still held: by a process that's there.
cat >"$tmp/bin/held" <<'EOF'
#!/usr/bin/env bash
! "$REAL_PYTHON" -c 'import fcntl, sys; fcntl.flock(open(sys.argv[1], "a"), fcntl.LOCK_EX | fcntl.LOCK_NB)' "$FAKE/$1" 2>/dev/null
EOF
# The process table: "pid lock command…" for each process started here, of which it lists
# the ones still there, as "pid command…".
cat >"$tmp/bin/ps" <<'EOF'
#!/usr/bin/env bash
while read -r pid lock command; do
  ! held "$lock" || echo "$pid $command"
done <"$FAKE/ps" 2>/dev/null
EOF
# A process that holds the lock $1 for its life, and says so once it has it. Asked to go, one
# that "goes" notes that it was and does; one that "stays" doesn't.
cat >"$tmp/bin/holds" <<'EOF'
#!/usr/bin/env bash
exec "$REAL_PYTHON" -c '
import fcntl, os, signal, sys, time
lock = os.path.join(os.environ["FAKE"], sys.argv[1])
def asked(*_):
    open(lock + ".asked", "w").close()
    os._exit(0)
signal.signal(signal.SIGTERM, signal.SIG_IGN if sys.argv[2] == "stays" else asked)
held = open(lock, "w")
fcntl.flock(held, fcntl.LOCK_EX)
open(lock + ".held", "w").close()
while os.path.isdir(os.environ["FAKE"]):
    time.sleep(1)
' "$@" >/dev/null 2>&1
EOF
cat >"$tmp/bin/chrome" <<'EOF'
#!/usr/bin/env bash
profile=""
for arg; do
  case "$arg" in --user-data-dir=*) profile="\${arg#--user-data-dir=}" ;; esac
done
holds lock.goes goes &
echo "$! lock.goes" >>"$FAKE/helpers"
echo "$! lock.goes $0 --type=renderer --user-data-dir=$profile --lang=en" >>"$FAKE/ps"
holds lock.stays stays &
echo "$! lock.stays" >>"$FAKE/helpers"
echo "$! lock.stays $0 --type=gpu-process --user-data-dir=$profile" >>"$FAKE/ps"
while read -r pid lock what; do
  case "$what" in
    longer) echo "$pid $lock $0 --type=renderer --user-data-dir=$profile-kept --lang=en" ;;
    folder) echo "$pid $lock tail -f \${profile%/profile}/dom.html" ;;
    within) echo "$pid $lock logger info=--user-data-dir=$profile" ;;
    looking) echo "$pid $lock grep -F -- --user-data-dir=$profile" ;;
  esac
done <"$FAKE/bystanders" >>"$FAKE/ps"
until [ -e "$FAKE/lock.goes.held" ] && [ -e "$FAKE/lock.stays.held" ]; do sleep 0.1; done
echo up >"$FAKE/ready"
case "$*" in
  *--dump-dom*) echo '<html><head><title>done: PASS 1/1</title></head></html>' ;;
  *--screenshot=*) exit 7 ;;
  *) exec sleep 120 ;;
esac
EOF
cat >"$tmp/bin/engine" <<'EOF'
#!/usr/bin/env bash
case "\${3:-board}" in
  name) echo "the made-up Mac" ;;
  *) echo '{"lanes": []}' ;;
esac
EOF
step chmod +x "$tmp/bin/python3" "$tmp/bin/curl" "$tmp/bin/held" "$tmp/bin/ps" "$tmp/bin/holds" "$tmp/bin/chrome" "$tmp/bin/engine"
for file in fake-tauri.js prelude.js some-check.js; do
  echo "// $file" >"scripts/preview/$file"
done
step env PREVIEW_FIXTURE=1 PREVIEW_PREPARE=only PREVIEW_BIN="$tmp/bin/engine" scripts/preview.sh >/dev/null 2>&1
export PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip PREVIEW_CHROME="$tmp/bin/chrome"
for what in longer folder within looking; do
  holds "by.$what" goes &
  echo "$! by.$what $what" >>"$tmp/bystanders"
done
until [ -e "$tmp/by.longer.held" ] && [ -e "$tmp/by.folder.held" ] && [ -e "$tmp/by.within.held" ] && [ -e "$tmp/by.looking.held" ]; do sleep 0.1; done
# How many of the helpers started since the last count are still there (and none, after), and
# how many of them went when asked. One made to go lets go of its lock in a moment, so it's
# looked at a few times before it's taken for there.
left() {
  local pid lock n=0 all=0 tries
  while read -r pid lock; do
    all=$((all + 1))
    tries=0
    while held "$lock" && [ $tries -lt 30 ]; do
      sleep 0.1
      tries=$((tries + 1))
    done
    if held "$lock"; then
      n=$((n + 1))
      kill -9 "$pid" 2>/dev/null
    fi
  done <"$tmp/helpers"
  echo "$n of $all left, $(ls "$tmp" | grep -c '^lock\\..*\\.asked$') asked first"
  rm -f "$tmp"/lock.* "$tmp/helpers" "$tmp/ready"
  grep ' by\\.' "$tmp/ps" >"$tmp/ps.kept" || true
  mv "$tmp/ps.kept" "$tmp/ps"
}
echo "a check: $(scripts/preview.sh sessions '' scripts/preview/some-check.js 2>/dev/null), with $(left)"
echo "a check on the real clock: $(PREVIEW_WAIT=100000 scripts/preview.sh sessions '' scripts/preview/some-check.js 2>/dev/null), with $(left)"
# A screenshot Chrome fails at fails as Chrome did.
scripts/preview.sh sessions >/dev/null 2>&1
echo "a screenshot: $?, with $(left)"
# One stopped part way, as checks.sh stops a run that takes too long: its page never says done.
SAYS=loading PREVIEW_WAIT=100000 scripts/preview.sh sessions '' scripts/preview/some-check.js >/dev/null 2>&1 &
run=$!
until [ -s "$tmp/ready" ]; do sleep 0.2; done
kill "$run"
wait "$run"
echo "a check stopped part way: $?, with $(left)"
there=0
while read -r pid lock what; do
  if held "$lock"; then there=$((there + 1)); fi
  kill -9 "$pid" 2>/dev/null
done <"$tmp/bystanders"
echo "what else names the run's profile is left alone: $there of 4"
`;

test('Chrome’s helpers go once a run is over, however it ends, and nothing else does', { timeout: WAIT + 20_000 }, async () => {
  expect(await said(LEFT, preview)).toEqual([
    'a check: PASS 1/1, with 0 of 2 left, 1 asked first',
    'a check on the real clock: PASS 1/1, with 0 of 2 left, 1 asked first',
    'a screenshot: 7, with 0 of 2 left, 1 asked first',
    'a check stopped part way: 130, with 0 of 2 left, 1 asked first',
    "what else names the run's profile is left alone: 4 of 4",
  ]);
});

// checks.sh itself, over a preview.sh that says of each run what scripts/preview/outcomes has
// for it: "<check> <view>[-<scheme>]: <its first go> | <its second> | …", the last for every go
// after, and a pass for a run it doesn't name. A go is PASS, FAIL <what failed> (one of two
// failing, or FAIL 0/10 <what failed> for a count of its own), NONE (the check never said how it
// went) or DIE (the set of checks it's in is killed where it stands). Four checks, each with a
// run in light and one in dark, and the lot committed: the commit is --base's.
//
// The set to kill is the stand-in's parent: checks.sh runs it under gtimeout where that's
// installed, so here gtimeout hands straight on to what it's given, and the run is checks.sh's
// own child either way. Nothing asks the process table, which a sandbox doesn't show (ps here
// refuses, as it does there).
const CHECKS = `${FOLDER}
step cp "$1" scripts/checks.sh
cat >"$tmp/bin/gtimeout" <<'EOF'
#!/usr/bin/env bash
shift 3
exec "$@"
EOF
cat >"$tmp/bin/ps" <<'EOF'
#!/usr/bin/env bash
echo "ps: Operation not permitted" >&2
exit 1
EOF
step chmod +x "$tmp/bin/gtimeout" "$tmp/bin/ps"
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
    kill -9 "$PPID"
    exit 1
    ;;
  FAIL\\ [0-9]*/[0-9]*\\ *)
    echo "$says"
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
SEEN="$tmp/seen" PREVIEW_PREPARE=skip PREVIEW_PLATFORM=linux PREVIEW_SCHEME=dark PREVIEW_SCHEDULES=s.json PREVIEW_SETUP=u.json PREVIEW_ANALYTICS=1 PREVIEW_IMAGES=/pictures PREVIEW_DATE=2026-12-28 scripts/checks.sh --fixture >"$tmp/out" 2>&1
echo "the gate: $?: $(tail -n 1 "$tmp/out")"
cat "$tmp/seen"
rm "$tmp/seen"
SEEN="$tmp/seen" PREVIEW_PREPARE=skip PREVIEW_SCHEME=dark scripts/checks.sh beta >"$tmp/out" 2>&1
echo "this Mac's data: $?: $(tail -n 1 "$tmp/out")"
cat "$tmp/seen"
rm "$tmp/seen"
SEEN="$tmp/seen" scripts/checks.sh --fixture --prepared >"$tmp/out" 2>&1
echo "the gate, asked not to build: $?: $(count)"
tail -n 1 "$tmp/out"
cat "$tmp/seen"

check zeta "PREVIEW_TRANSCRIPT=<key> scripts/preview.sh sessions '' scripts/preview/zeta-check.js"
echo "PREVIEW_TRANSCRIPT=<key> scripts/preview.sh sessions '' scripts/preview/zeta-check.js" >>scripts/preview/fixture-passes.txt
scripts/checks.sh --fixture >"$tmp/out" 2>&1
echo "a run made by hand, listed: $?: $(count) $(grep -c "lists a run that's made by hand" "$tmp/out") said so."

# The run its check has lost is the list's last line, with no newline after it.
printf '%s\n%s\n%s' "$(light alpha)" "$(light beta)" "PREVIEW_SCHEME=sepia scripts/preview.sh sessions '' scripts/preview/alpha-check.js" >scripts/preview/fixture-passes.txt
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
    // And built, whatever was left exported: a gate that skips the build passes on the UI of
    // whenever the folder was made.
    'the build and the snapshots: PREVIEW_DIR=.preview/fixture PREVIEW_FIXTURE=1 PREVIEW_PREPARE=only',
    'alpha sessions: PREVIEW_DIR=.preview/fixture PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip',
    'alpha sessions-dark: PREVIEW_DIR=.preview/fixture PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip PREVIEW_SCHEME=dark',
    // On real data a scheme asked for still reaches every run, as the script's header says.
    "this Mac's data: 0: 2 passed, 0 failed.",
    'the build and the snapshots: PREVIEW_DIR=.preview PREVIEW_PREPARE=only PREVIEW_SCHEME=dark',
    'beta sessions-dark: PREVIEW_DIR=.preview PREVIEW_PREPARE=skip PREVIEW_SCHEME=dark',
    'beta sessions-dark: PREVIEW_DIR=.preview PREVIEW_PREPARE=skip PREVIEW_SCHEME=dark',
    // Only the command line asks for the folder as it stands, and the run's last word is that.
    'the gate, asked not to build: 0: 2 passed, 0 failed.',
    "Not built (--prepared): these ran against the UI and the snapshots .preview/fixture already held, which may not be this checkout's code.",
    'alpha sessions: PREVIEW_DIR=.preview/fixture PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip',
    'alpha sessions-dark: PREVIEW_DIR=.preview/fixture PREVIEW_FIXTURE=1 PREVIEW_PREPARE=skip PREVIEW_SCHEME=dark',
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
# Three more checks: for a report that's cut short (prelude.js keeps the first 2,400 characters
# of a list of failures, the last an ellipsis, and so names only the ones that fit), for one
# whose last "why" alone is cut, at its own shorter length, and for the same words with more
# failing.
for name in kappa lambda mu; do
  check "$name" "$(light "$name")"
done
cut_short="nine: that both have; $(head -c 2377 /dev/zero | tr '\\0' 'x')…"
says "alpha sessions: FAIL saves: nothing saved" "beta sessions: FAIL saves: nothing saved; opens: no row" "gamma sessions: FAIL saves: nothing saved" \\
  "kappa sessions: FAIL 1/11 $cut_short" \\
  "lambda sessions: FAIL 1/3 saves: nothing saved; lists: Show in Ghostty, Open folder in, New terminal, and more than a why has room for…" \\
  "mu sessions: FAIL 1/3 saves: nothing saved"
# An engine this checkout has and the base's folder doesn't, named from here as PREVIEW_BIN may.
step mkdir mine
step cp "$tmp/bin/npx" mine/engine
${COMMITTED}
base="$(git rev-parse --short HEAD)"
# What --base says of each failure, without the runs' own lines and with one space for many.
verdicts() { sed -n '/^Trying the .* that failed on/,$p' "$tmp/out" | grep -v -e '^    [A-Z]' -e '^$' -e '^Trying' | sed "s/$base/<base>/g" | tr -s ' '; }

says "alpha sessions: NONE" "beta sessions: FAIL saves: nothing saved" "gamma sessions: FAIL saves: nothing saved; closes: still open" "delta sessions: FAIL opens: no row" \\
  "kappa sessions: FAIL 1/11 $cut_short" \\
  "lambda sessions: FAIL 1/3 saves: nothing saved; lists: Show in Ghostty, Open folder in, New terminal, and more than a why has room for…" \\
  "mu sessions: FAIL 0/3 saves: nothing saved"
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
    '1: 4 passed, 7 failed.',
    // The base's failure covers this one only when everything that fails here fails so there.
    'fails on <base> too beta sessions',
    'FAIL here only delta sessions',
    'FAIL, not as on <base> gamma sessions',
    ' here only: closes: still open',
    // A report cut short names only its first failures, so it's never the base's, whatever
    // the two have in common: here the same words and the same count, as when nine failures
    // are both's and a tenth, beyond the cut, is `opens` there and `saves` here. A report
    // whose last "why" is cut names every failure, and is read like any other.
    'FAIL, its report cut short kappa sessions',
    " 1/11 here, 1/11 there: only the first failures are named, so it can't be told from <base>'s: look at it",
    'fails on <base> too lambda sessions',
    'FAIL, more than on <base> mu sessions',
    ' 0/3 here, 1/3 there',
    // No result twice says nothing of the check, whatever the base's run of it said.
    'FAIL, no result here alpha sessions',
    '7 failed: 2 on <base> too, 5 not.',
    // The same packages as the base asks for: this checkout's, and the engine by its whole path
    // (the base's runs would have had no engine, and so no result, by the path it was given).
    "built with this checkout's packages",
    'installed: ',
    '0: 10 passed, 1 failed.',
    // Other packages than the base asks for: its UI is built with its own, or a failure an
    // update to one made would fail on both and pass for the data's.
    ' <base> asks for other packages than this checkout has: installing its own…',
    'fails on <base> too beta sessions',
    '1 failed: 1 on <base> too, 0 not.',
    'built with its own',
    'installed: npm ci',
  ]);
});

test('a set of checks that stops short is a failure --base can see, and keeps what it had found', { timeout: WAIT + 20_000 }, async () => {
  const lines = await said(
    `${CHECKS}
says
${COMMITTED}
says "beta sessions: DIE | NONE"
scripts/checks.sh --jobs=2 --base=HEAD >"$tmp/out" 2>&1
echo "$?: $(count)"
echo "$(grep -c 'a set of checks stopped short (beta gamma' "$tmp/out") said so; beta went $(goes 'beta sessions') and $(goes 'beta sessions-dark'), gamma $(goes 'gamma sessions') and $(goes 'gamma sessions-dark')."
grep -e 'no result here' -e 'failed: ' "$tmp/out" | sed "s/$(git rev-parse --short HEAD)/<base>/g" | tr -s ' '

# The set stops at beta's second run, having found its first failing: a failure that would pass
# on another go.
says "beta sessions: FAIL saves: nothing saved | PASS" "beta sessions-dark: DIE | PASS"
scripts/checks.sh --jobs=2 >"$tmp/out" 2>&1
echo "$?: $(count) Beta's light run went $(goes 'beta sessions'), its dark $(goes 'beta sessions-dark'), gamma's $(goes 'gamma sessions') and $(goes 'gamma sessions-dark')."
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
    // A set says each result as it has it, so what it found before it stopped stands: only the
    // runs it never got to go again. Its results were once written as it ended, a set that
    // stopped left none, and the failure went again with the rest, and passed.
    "1: 7 passed, 1 failed. Beta's light run went 1, its dark 2, gamma's 1 and 1.",
  ]);
});

test('a list of failures is taken for cut short at the length prelude.js cuts it to', () => {
  // checks.sh --base can't see the page, only the title it left: it takes a list for cut by its
  // length, which is prelude.js's to change.
  const kept = /\.join\('; '\), (\d+)\)/.exec(text(prelude))?.[1];
  const taken = /^cut_len=(\d+)$/m.exec(text(checks))?.[1];
  expect(kept).toMatch(/^\d+$/);
  expect(taken).toBe(kept);
});

test('a page asked for a day is on that day, at noon, whatever the hour and the day here', () => {
  // PREVIEW_DATE's clock (scripts/preview/clock.js), run as the page runs it, on the evening
  // of a day with 25 hours: 23:30 on November 1, 2026 in Los Angeles, where the clocks went
  // back that morning. Counted from midnight that's 24 and a half hours into the day, and a
  // page set that far into December 28 is on the 29th, where due-check.js would have taken
  // its expectation from the same clock and passed without the 28th being looked at.
  const env = (globalThis as unknown as { process: { env: Record<string, string | undefined> } }).process.env;
  const zone = env.TZ;
  env.TZ = 'America/Los_Angeles';
  try {
    const now = new Date(2026, 10, 1, 23, 30).getTime();
    expect(new Date(now).toISOString()).toBe('2026-11-02T07:30:00.000Z');
    class Then extends Date {
      constructor(...given: unknown[]) {
        super(...((given.length ? given : [now]) as [number]));
      }
      static now() {
        return now;
      }
    }
    const page = { __PREVIEW_DATE__: '2026-12-28', Date: Then as unknown as DateConstructor };
    new Function('window', 'Date', text(clock))(page, Then);
    const there = new page.Date();
    expect([there.getFullYear(), there.getMonth() + 1, there.getDate(), there.getHours(), there.getMinutes()]).toEqual([2026, 12, 28, 12, 0]);
    expect(new Date(page.Date.now()).getDate()).toBe(28);
    // The rest of Date is as it was: a day made from its parts, a time read back.
    const due = new page.Date(2027, 0, 9);
    expect([due instanceof Date, due.getFullYear(), due.getMonth(), due.getDate()]).toEqual([true, 2027, 0, 9]);
    expect(new page.Date(now).getTime()).toBe(now);
    expect(page.Date.UTC(2027, 0, 9)).toBe(Date.UTC(2027, 0, 9));
  } finally {
    if (zone === undefined) delete env.TZ;
    else env.TZ = zone;
  }
});
