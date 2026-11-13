#!/usr/bin/env bash
# A Windows VM to build, test and run Codealong in: Windows 11 Arm64 under QEMU with Apple's
# hypervisor (`brew install qemu`), installed with no clicks from Microsoft's own ISO by the answer
# file and scripts in scripts/windows/. The VM mounts nothing of the Mac's: it works on a copy of
# the checkout this runs from, one for each checkout (~\checkouts\<folder>-<hash of its path>),
# which every command but start, shell, screenshot and stop first brings up to date, and every
# copy builds into one folder (~\codealong-target), as linux-vm.sh's do. Its files are in
# ~/.codealong-vms/<name> (the disk grows to some 60 GB), outside every checkout, since one VM
# serves them all; what's downloaded is kept in ~/.codealong-vms/downloads for the next VM.
#
#   scripts/windows-vm.sh                creates the VM the first time (an hour or two: an 8 GB
#                                        download, Windows' install, Visual Studio's build tools),
#                                        else starts it
#   scripts/windows-vm.sh test [args…]   cargo test in it (args go to cargo test)
#   scripts/windows-vm.sh build          builds the app there (`tauri build --no-bundle`)
#   scripts/windows-vm.sh run [secs]     runs that build on the VM's desktop, with its own home, and
#                                        saves a screenshot to .preview/windows.png
#   scripts/windows-vm.sh shell [cmd…]   PowerShell in the VM (or one command), in its copy of this
#                                        checkout
#   scripts/windows-vm.sh screenshot     the VM's screen, to .preview/windows.png
#   scripts/windows-vm.sh click <x> <y>  clicks the VM's screen there, in that screenshot's pixels
#   scripts/windows-vm.sh stop | delete
#
# Microsoft's download page refuses a script now and then ("Sentinel marked this request as
# rejected", and always from VPN and datacenter addresses): then the ISO is made from Windows
# Update's own files through UUP dump (scripts/windows/uup-iso.sh, `brew install aria2 cabextract
# wimlib cdrtools hivex` first), or WINDOWS_ISO=<file> takes one downloaded from
# https://www.microsoft.com/software-download/windows11arm64 in a browser. Its install runs
# detached and survives this script ending; run the script again to wait for it and go on.
# Setup's screen shows only in `screenshot`, and RDP reaches the desktop at localhost:3390 (dev,
# with the password in the VM's folder). A screen of Setup's that waits on a click takes one from
# `click`: OOBE once stopped at "Something went wrong" (OOBEZDP, its check for updates) until Skip
# was clicked.
#
# The app in the VM has the VM's home, nothing of the Mac's. Claude Code and Codex are installed
# there, signed in to nothing; sign one in from `scripts/windows-vm.sh shell` to give the board
# real sessions.
set -euo pipefail
cd "$(dirname "$0")/.."
here="$PWD"
assets="$here/scripts/windows"
vm="${CODEALONG_WINVM:-codealong-windows}"
root="$HOME/.codealong-vms"
dir="$root/$vm"
downloads="$root/downloads"
ssh_port="${CODEALONG_WINVM_SSH:-2223}"
rdp_port="${CODEALONG_WINVM_RDP:-3390}"
# Its folder's name, made safe to use unquoted in the commands sent to the VM.
copy="checkouts/$(basename "$here" | tr -c 'A-Za-z0-9._\n-' '_')-$(printf %s "$here" | shasum | cut -c1-8)"

command -v qemu-system-aarch64 >/dev/null || { echo "QEMU isn't installed: brew install qemu" >&2; exit 1; }
qemu_share="$(brew --prefix qemu)/share/qemu"

say() { echo "windows-vm: $*" >&2; }
qmp() { python3 "$assets/qmp.py" "$dir/qmp.sock" "$@"; }
running() { [ -f "$dir/qemu.pid" ] && kill -0 "$(cat "$dir/qemu.pid")" 2>/dev/null; }

ssh_vm() {
  ssh -i "$dir/id_ed25519" -p "$ssh_port" -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile="$dir/known_hosts" \
    -o LogLevel=ERROR -o ConnectTimeout=10 -o ServerAliveInterval=30 dev@127.0.0.1 "$@"
}
scp_vm() {
  scp -q -i "$dir/id_ed25519" -P "$ssh_port" -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile="$dir/known_hosts" -o LogLevel=ERROR "$@"
}
# A command in the VM's PowerShell (its SSH shell), from its copy of the checkout, ending with the
# last native command's status.
in_vm() { ssh_vm "\$ErrorActionPreference = 'Stop'; Set-Location ~/$copy; $1; exit \$LASTEXITCODE"; }

# Waits for `check` to pass, up to `minutes`, saying what it waits for every five.
wait_for() {
  local what="$1" minutes="$2" check="$3" waited=0
  until eval "$check"; do
    if [ $((waited % 300)) -eq 0 ]; then say "waiting for $what ($((waited / 60)) of $minutes minutes)"; fi
    if [ "$waited" -ge $((minutes * 60)) ]; then say "$what didn't come; scripts/windows-vm.sh screenshot shows the screen"; return 1; fi
    sleep 10
    waited=$((waited + 10))
  done
}

# ---- Making it ----

download() {
  local url="$1" file="$2"
  [ -f "$file" ] && return
  say "downloading $(basename "$file")"
  curl -fL --retry 5 -C - -o "$file.part" "$url"
  mv "$file.part" "$file"
}

# Microsoft's Windows 11 Arm64 ISO, checked against the SHA-256 its page lists: the one given as
# WINDOWS_ISO, else one this downloads (scripts/windows/iso-url.sh). A partial download goes on
# where it stopped, with a fresh link when the last one has expired (they last 24 hours). When
# the page won't give a link, the ISO is made from Windows Update's own files instead
# (scripts/windows/uup-iso.sh), which takes longer and needs a few tools from Homebrew.
windows_iso() {
  if [ -n "${WINDOWS_ISO:-}" ]; then
    [ -f "$WINDOWS_ISO" ] || { say "WINDOWS_ISO=$WINDOWS_ISO isn't a file"; exit 1; }
    iso="$WINDOWS_ISO"
    return
  fi
  iso="$downloads/win11-arm64.iso"
  [ -f "$iso" ] && return
  local link sha url
  if ! link="$(bash "$assets/iso-url.sh")"; then
    say "Microsoft's page refused the download; making the ISO from Windows Update's files (an hour or so)"
    bash "$assets/uup-iso.sh" "$iso"
    return
  fi
  sha="${link%% *}" url="${link#* }"
  say "downloading Windows 11 Arm64 (about 8 GB)"
  curl -fL --retry 5 -C - -o "$iso.part" "$url"
  say "checking its SHA-256"
  if [ "$(shasum -a 256 "$iso.part" | cut -d' ' -f1 | tr '[:lower:]' '[:upper:]')" != "$sha" ]; then
    mv "$iso.part" "$iso.bad"
    say "the ISO's SHA-256 isn't the one Microsoft lists; it's kept as $iso.bad"
    exit 1
  fi
  mv "$iso.part" "$iso"
}

# The medium Setup reads its answers from (Windows looks for autounattend.xml at the root of
# every drive), with the network's driver where Setup loads drivers from ($WinPEDriver$), and
# what the first logon runs: OpenSSH's setup, the VM's own key, and Microsoft's OpenSSH MSI for
# when Windows Update can't be reached.
unattend_medium() {
  [ -f "$dir/unattend.iso" ] && return
  download "https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/stable-virtio/virtio-win.iso" "$downloads/virtio-win.iso"
  download "https://github.com/PowerShell/Win32-OpenSSH/releases/download/10.4.0.0p1-Preview/OpenSSH-ARM64-v10.4.0.0.msi" "$downloads/OpenSSH-ARM64.msi"
  local stage="$dir/unattend"
  # What's taken from an ISO comes read-only, folders too.
  [ -d "$dir/virtio" ] && chmod -R u+w "$dir/virtio"
  rm -rf "$stage" "$dir/virtio"
  mkdir -p "$stage/setup" "$stage/\$WinPEDriver\$" "$dir/virtio"
  # The whole driver's folder: its Windows 11 files are hard links to others of its folders'.
  tar -xf "$downloads/virtio-win.iso" -C "$dir/virtio" NetKVM
  cp -R "$dir/virtio/NetKVM/w11/ARM64" "$stage/\$WinPEDriver\$/NetKVM"
  sed "s/@PASSWORD@/$(cat "$dir/password")/g" "$assets/autounattend.xml" > "$stage/autounattend.xml"
  cp "$assets/firstlogon.ps1" "$downloads/OpenSSH-ARM64.msi" "$stage/setup/"
  cp "$dir/id_ed25519.pub" "$stage/setup/authorized_keys"
  hdiutil makehybrid -quiet -ov -iso -joliet -default-volume-name UNATTEND -o "$dir/unattend.iso" "$stage"
  chmod -R u+w "$stage" "$dir/virtio"
  rm -rf "$stage" "$dir/virtio"
}

# QEMU's machine: Apple's hypervisor, UEFI firmware with its variables kept for this VM, a
# framebuffer Setup can draw on without drivers, an NVMe disk (Windows has its driver), the
# media as USB drives (Windows has that driver too), and virtio's network, forwarding SSH and
# RDP on localhost only.
qemu_args() {
  args=(-name "$vm" -machine "virt,highmem=on,gic-version=3" -accel hvf -cpu host -smp 8 -m 12G
    -drive "if=pflash,format=raw,unit=0,readonly=on,file=$qemu_share/edk2-aarch64-code.fd"
    -drive "if=pflash,format=raw,unit=1,file=$dir/vars.fd"
    -device ramfb
    -device "qemu-xhci,id=xhci" -device "usb-kbd,bus=xhci.0" -device "usb-tablet,bus=xhci.0"
    -device "nvme,serial=codealong,drive=sys" -drive "if=none,id=sys,file=$dir/disk.qcow2,format=qcow2,discard=unmap,cache=writeback"
    -device "usb-storage,bus=xhci.0,drive=unattend,removable=on" -drive "if=none,id=unattend,media=cdrom,readonly=on,file=$dir/unattend.iso"
    -netdev "user,id=net,hostfwd=tcp:127.0.0.1:$ssh_port-:22,hostfwd=tcp:127.0.0.1:$rdp_port-:3389" -device "virtio-net-pci,netdev=net,romfile="
    -rtc base=localtime -display none -monitor none -serial "file:$dir/serial.log"
    -qmp "unix:$dir/qmp.sock,server=on,wait=off" -pidfile "$dir/qemu.pid" -daemonize)
}

boot() {
  qemu_args
  rm -f "$dir/qemu.pid"
  qemu-system-aarch64 "${args[@]}" "$@"
}

# Windows' Setup, the first time: from a copy of the ISO that boots without asking for a key
# (scripts/windows/noprompt.py; an APFS clone, so it takes no room), with QEMU ending at Setup's
# first restart (`-no-reboot`), since that copy would boot Setup again. Started detached, so it
# goes on if this script ends; a run of this script after it finds it running and waits.
install() {
  if [ ! -f "$dir/noprompt.iso" ]; then
    cp -c "$iso" "$dir/noprompt.iso.part"
    python3 "$assets/noprompt.py" "$dir/noprompt.iso.part"
    mv "$dir/noprompt.iso.part" "$dir/noprompt.iso"
  fi
  if ! running; then
    say "installing Windows (half an hour or so)"
    boot -no-reboot -device "usb-storage,bus=xhci.0,drive=win,removable=on" -drive "if=none,id=win,media=cdrom,readonly=on,file=$dir/noprompt.iso"
    touch "$dir/installing"
  fi
  wait_for "Windows' Setup to finish copying" 90 '! running'
  rm -f "$dir/installing" "$dir/noprompt.iso"
  touch "$dir/installed"
}

create() {
  mkdir -p "$dir" "$downloads"
  [ -f "$dir/id_ed25519" ] || ssh-keygen -q -t ed25519 -N '' -C "$vm" -f "$dir/id_ed25519"
  if [ ! -f "$dir/password" ]; then
    (umask 077 && python3 -c 'import secrets, string; print("".join(secrets.choice(string.ascii_letters + string.digits) for _ in range(20)), end="")' > "$dir/password")
  fi
  [ -f "$dir/vars.fd" ] || cp "$qemu_share/edk2-arm-vars.fd" "$dir/vars.fd"
  [ -f "$dir/disk.qcow2" ] || qemu-img create -q -f qcow2 "$dir/disk.qcow2" 128G
  if [ ! -f "$dir/installed" ]; then
    unattend_medium
    windows_iso
    install
  fi
}

# The build tools (scripts/windows/tools.ps1), the first time, once SSH answers: Setup's second
# half and the first logon, which turns SSH on, take a while after the install's restart.
provision() {
  [ -f "$dir/provisioned" ] && return
  say "installing the build tools (Visual Studio's take most of an hour)"
  scp_vm "$assets/tools.ps1" "dev@127.0.0.1:tools.ps1"
  ssh_vm "powershell -NoProfile -ExecutionPolicy Bypass -File tools.ps1; exit \$LASTEXITCODE" | tee -a "$dir/provision.log"
  touch "$dir/provisioned"
}

start() {
  create
  if ! running; then
    boot
  fi
  wait_for "the VM's SSH" 60 'ssh_vm exit 2>/dev/null'
  provision
}

# The VM's copy of the checkout: what's in this one, committed or not, without what it built,
# sent as a tarball (Windows has tar but no rsync). What was there is taken away first but for
# node_modules, so a file removed here is gone there too; the files keep their times, so cargo
# builds again only what changed.
sync() {
  start
  local tarball
  tarball="$(mktemp -t codealong-sync).tgz"
  COPYFILE_DISABLE=1 tar -czf "$tarball" --exclude ./.git --exclude ./node_modules --exclude ./dist --exclude ./.preview \
    --exclude ./src-tauri/target --exclude ./src-tauri/gen .
  ssh_vm "New-Item -ItemType Directory -Force ~/$copy | Out-Null"
  scp_vm "$tarball" "dev@127.0.0.1:$copy.tgz"
  rm -f "$tarball"
  ssh_vm "Get-ChildItem -Force ~/$copy | Where-Object Name -ne 'node_modules' | Remove-Item -Recurse -Force; tar -xzf ~/$copy.tgz -C ~/$copy; \$s = \$LASTEXITCODE; Remove-Item ~/$copy.tgz; exit \$s"
}

frontend() {
  in_vm "if (-not (Test-Path node_modules/.package-lock.json) -or (Get-Item node_modules/.package-lock.json).LastWriteTime -lt (Get-Item package-lock.json).LastWriteTime) { npm ci --no-audit --no-fund; if (\$LASTEXITCODE) { exit \$LASTEXITCODE } }; npx vite build --logLevel warn"
}

screenshot() {
  running || { say "the VM isn't running"; exit 1; }
  mkdir -p "$here/.preview"
  qmp "{\"execute\":\"screendump\",\"arguments\":{\"filename\":\"$here/.preview/windows.png\",\"format\":\"png\"}}" >/dev/null
  echo "$here/.preview/windows.png"
}

# A click at x,y in the last screenshot's pixels, with the USB tablet, whose coordinates run from 0
# to 32767 across the screen; the press and the release go a moment apart, after the move, as a
# hand's would.
click() {
  running || { say "the VM isn't running"; exit 1; }
  local png="$here/.preview/windows.png" size x y
  [ -f "$png" ] || screenshot >/dev/null
  size="$(python3 -c 'import struct, sys; print(*struct.unpack(">II", open(sys.argv[1], "rb").read(24)[16:24]))' "$png")"
  x=$(($1 * 32767 / ${size% *})) y=$(($2 * 32767 / ${size#* }))
  qmp "{\"execute\":\"input-send-event\",\"arguments\":{\"events\":[{\"type\":\"abs\",\"data\":{\"axis\":\"x\",\"value\":$x}},{\"type\":\"abs\",\"data\":{\"axis\":\"y\",\"value\":$y}}]}}" >/dev/null
  sleep 0.2
  qmp '{"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":true,"button":"left"}}]}}' >/dev/null
  sleep 0.1
  qmp '{"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":false,"button":"left"}}]}}' >/dev/null
}

case "${1:-start}" in
  start) start ;;
  test)
    shift
    sync
    frontend
    in_vm "Set-Location src-tauri; cargo test $*"
    ;;
  build)
    sync
    frontend
    in_vm "npx tauri build --no-bundle"
    ;;
  run)
    secs="${2:-20}"
    start
    scp_vm "$assets/run.ps1" "dev@127.0.0.1:run.ps1"
    ssh_vm "powershell -NoProfile -ExecutionPolicy Bypass -File run.ps1; exit \$LASTEXITCODE"
    sleep "$secs"
    screenshot
    ssh_vm "powershell -NoProfile -ExecutionPolicy Bypass -File run.ps1 -Stop"
    ;;
  shell)
    shift
    start
    if [ $# -eq 0 ]; then
      ssh -t -i "$dir/id_ed25519" -p "$ssh_port" -o UserKnownHostsFile="$dir/known_hosts" -o LogLevel=ERROR dev@127.0.0.1 "Set-Location ~/$copy -ErrorAction SilentlyContinue; powershell -NoLogo"
    else
      in_vm "$*"
    fi
    ;;
  screenshot) screenshot ;;
  click)
    [ $# -eq 3 ] || { say "click <x> <y>, in the pixels of scripts/windows-vm.sh screenshot's picture"; exit 2; }
    click "$2" "$3"
    ;;
  stop)
    if running; then
      qmp '{"execute":"system_powerdown"}' >/dev/null
      wait_for "Windows to shut down" 5 '! running' || { qmp '{"execute":"quit"}' >/dev/null || true; }
    fi
    ;;
  delete)
    if running; then qmp '{"execute":"quit"}' >/dev/null || true; fi
    rm -rf "$dir"
    ;;
  *)
    sed -n '2,37p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
