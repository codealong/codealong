#!/usr/bin/env bash
# A Linux VM to build, test and run Codealong in: Ubuntu 24.04 under Lima (`brew install lima`),
# set up by scripts/linux/lima.yaml. The VM mounts nothing of the Mac's: it works on a copy of the
# checkout this runs from, one for each checkout (~/checkouts/<folder>-<hash of its path>), which
# every command but start and shell first brings up to date, so sessions working in different
# checkouts don't overwrite each other's, and its builds and node_modules never mix with the
# Mac's. Every copy builds into one folder (~/codealong-target), where cargo's lock queues them;
# `run` runs whichever built last, on one display, so two runs at once take turns badly.
#
#   scripts/linux-vm.sh                 creates the VM the first time (about ten minutes) and starts it
#   scripts/linux-vm.sh test [args…]    cargo test in it (args go to cargo test)
#   scripts/linux-vm.sh build           builds the app with `tauri build` (a .deb and the binary)
#   scripts/linux-vm.sh run [secs]      runs that build on the VM's own headless display, with its
#                                       own home, and saves a screenshot to .preview/linux.png
#   scripts/linux-vm.sh shell [cmd…]    a shell (or one command) in the VM's copy of the checkout
#   scripts/linux-vm.sh stop | delete
#
# The app in the VM has the VM's home: its own config.toml, sessions and socket, nothing of the
# Mac's. Claude Code and Codex are installed there, signed in to nothing; sign one in from
# `scripts/linux-vm.sh shell` to give the board real sessions.
set -euo pipefail
cd "$(dirname "$0")/.."
here="$PWD"
vm="${CODEALONG_VM:-codealong-linux}"
# Its folder's name, made safe to use unquoted in the commands sent to the VM.
copy="checkouts/$(basename "$here" | tr -c 'A-Za-z0-9._\n-' '_')-$(printf %s "$here" | shasum | cut -c1-8)"

command -v limactl >/dev/null || { echo "Lima isn't installed: brew install lima" >&2; exit 1; }

status() { limactl list --format '{{.Status}}' "$vm" 2>/dev/null || true; }

start() {
  case "$(status)" in
    Running) ;;
    "") limactl start --name "$vm" --tty=false --timeout 40m scripts/linux/lima.yaml ;;
    *) limactl start --tty=false --timeout 10m "$vm" ;;
  esac
}

# A command in the VM, in a login shell (rustup's PATH, CARGO_TARGET_DIR), from its copy.
in_vm() { limactl shell --workdir / "$vm" bash -lc "cd ~/$copy && $1"; }

# The VM's copy of the checkout: what's in this one, committed or not, without what it built,
# sent over the VM's own ssh.
sync() {
  start
  local ssh_config
  ssh_config="$(limactl list --format '{{.Dir}}' "$vm")/ssh.config"
  limactl shell --workdir / "$vm" bash -c "mkdir -p ~/$copy"
  rsync -a --delete -e "ssh -F '$ssh_config'" \
    --exclude /.git --exclude /node_modules --exclude /dist --exclude /.preview \
    --exclude /src-tauri/target --exclude /src-tauri/gen \
    "$here/" "lima-$vm:$copy/"
}

frontend() {
  in_vm '[ node_modules/.package-lock.json -nt package-lock.json ] || npm ci --no-audit --no-fund'
  in_vm 'npx vite build --logLevel warn'
}

case "${1:-start}" in
  start) start ;;
  shell)
    shift
    start
    if [ $# -eq 0 ]; then limactl shell --workdir / "$vm" bash -lc "cd ~/$copy 2>/dev/null; exec bash -l"
    else sync; in_vm "$*"; fi
    ;;
  test)
    shift
    sync
    frontend
    in_vm "cd src-tauri && cargo test $*"
    ;;
  build)
    sync
    frontend
    in_vm "npx tauri build --bundles deb --config '{\"build\": {\"beforeBuildCommand\": \"\"}}'"
    in_vm 'ls -1 "$CARGO_TARGET_DIR"/release/bundle/deb/*.deb "$CARGO_TARGET_DIR"/release/codealong'
    ;;
  run)
    sync
    mkdir -p .preview
    in_vm "scripts/linux/run.sh ${2:-20}"
    limactl copy "$vm:~/linux.png" .preview/linux.png
    echo "$here/.preview/linux.png"
    ;;
  stop) limactl stop "$vm" ;;
  delete) limactl delete --force "$vm" ;;
  *) sed -n '2,/^set -e/p' "$0" | sed '$d' >&2; exit 1 ;;
esac
