#!/usr/bin/env bash
# uup-iso.sh <iso>: makes the Windows 11 Arm64 ISO from Windows Update's own files, for
# windows-vm.sh when Microsoft's download page won't hand out its link (it refuses scripts now and
# then, and VPN and datacenter addresses always). UUP dump (uupdump.net) lists a release's files on
# Microsoft's update servers and hands out the converter that builds an ISO of them, pinned by its
# SHA-256 (convert.sh); this does what UUP dump's own macOS script does: the converter and the
# files fetched with aria2, which checks each file's hash, then convert.sh run on them. Windows
# Pro alone, in English (US): about 9 GB to download, and as much again while it builds, in a
# folder beside <iso> that goes once the ISO is made. Run again after a stop, it picks the
# download up where it was.
#
# WINDOWS_UUP_RELEASE names the release, the newest build of which is made: "Windows 11, version
# 26H2" by default, the one Microsoft's page hands out in 2026. It needs aria2, cabextract, wimlib,
# cdrtools and hivex (`brew install aria2 cabextract wimlib cdrtools hivex`); hivex stands in for
# chntpw, which Homebrew hasn't (chntpw.py).
set -euo pipefail
# Made absolute, as what's made is moved there from the folder it's made in.
out="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
release="${WINDOWS_UUP_RELEASE:-Windows 11, version 26H2}"
assets="$(cd "$(dirname "$0")" && pwd)"
work="$out.uup"

for tool in aria2c cabextract wimlib-imagex mkisofs; do
  command -v "$tool" >/dev/null || { echo "$tool isn't installed: brew install aria2 cabextract wimlib cdrtools hivex" >&2; exit 1; }
done
libhivex="$(brew --prefix hivex)/lib/libhivex.dylib"
[ -f "$libhivex" ] || { echo "hivex isn't installed: brew install hivex" >&2; exit 1; }

# The newest build of the release for Arm64, leaving out Insider and preview builds; or, picking
# up where a run stopped, the build it was making, whose files are partly here.
api=https://api.uupdump.net
mkdir -p "$work"
if [ -s "$work/build" ]; then
  id="$(cat "$work/build")"
else
  id="$(curl -fsS -m 60 -G --data-urlencode "search=$release" "$api/listid.php" | RELEASE="$release" python3 -c '
import json, os, sys
builds = json.load(sys.stdin)["response"]["builds"]
builds = builds.values() if isinstance(builds, dict) else builds
release = os.environ["RELEASE"]
found = [b for b in builds if b.get("arch") == "arm64" and b["title"].startswith(release) and "Insider" not in b["title"] and "Preview" not in b["title"]]
if not found:
    sys.exit("UUP dump lists no Arm64 build of %r" % release)
newest = max(found, key=lambda b: [int(p) for p in b["build"].split(".")])
print(newest["uuid"])
sys.stderr.write("making %s\n" % newest["title"])')"
  echo "$id" > "$work/build"
fi
cd "$work"
query="id=$id&pack=en-us&edition=professional"
# What UUP dump's macOS package holds: the converter's address with its hash, and its settings.
curl -fsS -m 120 -o package.zip -X POST -d "autodl=2&cleanup=1" "https://uupdump.net/get.php?$query"
unzip -o -q package.zip
fetch() { aria2c --no-conf --console-log-level=warn --log-level=info --log=aria2.log --allow-overwrite=true --auto-file-renaming=false "$@"; }
fetch -x16 -s16 -j2 -d files -i files/converter_multi
fetch -o files.txt "https://uupdump.net/get.php?$query&aria2=2"
if grep -q '#UUPDUMP_ERROR:' files.txt; then
  echo "Windows Update didn't list the files: $(grep '#UUPDUMP_ERROR:' files.txt | sed 's/#UUPDUMP_ERROR://')" >&2
  exit 1
fi
# The updates (the cumulative one alone is 4 GB) are left out: convert.sh can't put them in, and
# says so, only UUP dump's Windows converter can. So is a file an earlier run fetched whole: aria2
# checked its hash then, and takes away its control file (.aria2) only once it's whole, where
# fetching it again would check the hash again, which it does slowly.
python3 - files.txt <<'PY'
import os, re, sys
entries, entry = [], []
for line in open(sys.argv[1]).read().splitlines():
    if line and not line[0].isspace() and entry:
        entries.append(entry)
        entry = []
    entry.append(line)
entries.append(entry)
update = re.compile(r"\s*out=(windows1[0-9.]*-kb[0-9].*|ssu-.*)$", re.I)
def fetched(entry):
    out = next((line.strip()[4:] for line in entry if line.strip().startswith("out=")), None)
    return out and os.path.isfile(f"UUPs/{out}") and not os.path.exists(f"UUPs/{out}.aria2")
kept = [e for e in entries if not any(update.match(line) for line in e) and not fetched(e)]
open(sys.argv[1], "w").write("\n".join(line for e in kept for line in e) + "\n")
PY
if grep -q . files.txt; then
  fetch -x16 -s16 -j5 -c -R -d UUPs -i files.txt
fi

# convert.sh makes two registry values in boot.wim with chntpw, which hivex does here. It goes on
# after a command fails, so the stand-in notes a failure in a file of its own.
mkdir -p bin
ln -sf "$assets/chntpw.py" bin/chntpw
rm -f ./*.iso ./*.ISO chntpw-failed
PATH="$PWD/bin:$PATH" HIVEX_LIB="$libhivex" CHNTPW_FAILED="$PWD/chntpw-failed" bash files/convert.sh wim UUPs 0
if [ -s chntpw-failed ]; then
  echo "convert.sh couldn't set boot.wim's registry values, so its ISO would be broken: $(cat chntpw-failed)" >&2
  exit 1
fi
made="$(find . -maxdepth 1 -iname '*.iso' | head -1)"
[ -n "$made" ] || { echo "convert.sh made no ISO; $work has what it left" >&2; exit 1; }
mv "$made" "$out"
cd ..
rm -rf "$work"
