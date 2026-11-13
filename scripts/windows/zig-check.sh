#!/usr/bin/env bash
# Zig, as scripts/windows-check.sh runs it for Ghostty's terminal library (libghostty-vt-sys's
# build runs $ZIG): Zig can't find MSVC's headers from a Mac, so the library is built for Windows'
# GNU ABI instead, whose headers Zig carries. A check links nothing, so that's enough to check the
# Rust. The build script also reads the install's bin folder, where an MSVC build puts its DLL.
set -euo pipefail
args=() prefix='' take=0
for a in "$@"; do
  [ "$take" = 1 ] && prefix="$a"
  take=0
  [ "$a" = --prefix ] && take=1
  case "$a" in -Dtarget=*-windows-msvc) a="${a%-msvc}-gnu" ;; esac
  args+=("$a")
done
zig "${args[@]}"
if [ -n "$prefix" ]; then mkdir -p "$prefix/bin"; fi
