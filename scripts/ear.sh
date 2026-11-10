#!/usr/bin/env bash
# Prints the path of this checkout's codealong-ear, the helper that hears for Codealong
# (src-tauri/ear/main.swift), building it first when its source has changed since the last one:
#
#   scripts/ear.sh                  # for this Mac's processor
#   scripts/ear.sh arm64 x86_64     # one binary for both, as a release has it
#
# It isn't cargo's to build: it's Swift, since Apple's speech recognition has no other interface,
# and only macOS has it. relaunch.sh and release.sh put it in the app beside the engine
# (Contents/MacOS), and engine.sh beside the engine it builds for the checks, which is where the
# engine looks for it (ear.rs `binary`). It needs macOS 26 to run and Xcode 26 to build; where it
# can't be built this says why and fails, and its callers carry on without it: Codealong then says
# that this build can't hear.
set -euo pipefail
cd "$(dirname "$0")/.."
[ "$(uname)" = Darwin ] || {
  echo "codealong-ear is only for macOS." >&2
  exit 1
}
archs=("$@")
[ ${#archs[@]} -gt 0 ] || archs=("$(uname -m)")
own=.preview/ear
mkdir -p "$own"
name="codealong-ear-$(IFS=-; echo "${archs[*]}")"
state="$(shasum src-tauri/ear/main.swift | cut -d' ' -f1)"
if [ -x "$own/$name" ] && [ "$(cat "$own/$name.state" 2>/dev/null)" = "$state" ]; then
  echo "$PWD/$own/$name"
  exit 0
fi
# Built beside where it goes and moved into place, so two builds at once in a checkout (two
# previews) each leave a whole binary.
work="$(mktemp -d "$own/build.XXXXXX")"
trap 'rm -rf "$work"' EXIT
parts=()
for arch in "${archs[@]}"; do
  # Swift 5's rules: under 6's, every buffer handed from an audio thread to the transcriber would
  # need a proof that it's safe, which Apple's own audio types don't give.
  xcrun swiftc -swift-version 5 -O -target "$arch-apple-macos26.0" -o "$work/$arch" src-tauri/ear/main.swift >&2
  parts+=("$work/$arch")
done
if [ ${#parts[@]} -gt 1 ]; then
  lipo -create "${parts[@]}" -output "$work/out"
else
  mv "${parts[0]}" "$work/out"
fi
mv "$work/out" "$own/$name"
echo "$state" >"$own/$name.state"
echo "$PWD/$own/$name"
