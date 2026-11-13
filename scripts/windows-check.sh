#!/usr/bin/env bash
# `cargo check` of the engine for Windows, from this Mac: whether it compiles there, in a minute,
# without the Windows VM (scripts/windows-vm.sh, which builds and tests it for real). Args go to
# cargo check (`--tests` checks the tests too). It checks x86_64 Windows: ring's build won't
# cross-compile for Arm64 from here (it calls plain clang with MSVC's flags), and the Rust is the
# same. Warnings are errors, as CI's Linux build has them.
#
# What it needs, once: `cargo install --locked cargo-xwin` (which downloads MSVC's headers and
# libraries the first time, accepting Microsoft's license for them), `rustup target add
# x86_64-pc-windows-msvc`, and LLVM's clang-cl and lld-link (`brew install llvm`). Its builds go
# beside the shared build folder (scripts/target-dir.sh), not in it, so they neither queue behind
# nor unsettle the Mac's.
set -euo pipefail
cd "$(dirname "$0")/.."
# Homebrew's llvm, or the newest llvm@<version> where only a versioned one is installed.
llvm="$(brew --prefix)/opt/llvm"
[ -x "$llvm/bin/clang-cl" ] || llvm="$(find "$(brew --prefix)/opt" -maxdepth 1 -name 'llvm@*' | sort -V | tail -1)"
[ -x "$llvm/bin/clang-cl" ] || { echo "LLVM's clang-cl isn't installed: brew install llvm" >&2; exit 1; }
PATH="$HOME/.cargo/bin:$llvm/bin:$PATH"
export PATH
command -v cargo-xwin >/dev/null || { echo "cargo-xwin isn't installed: cargo install --locked cargo-xwin" >&2; exit 1; }
CARGO_TARGET_DIR="$(scripts/target-dir.sh)-windows"
export CARGO_TARGET_DIR
# Ghostty's terminal library builds with Zig, which finds no MSVC headers here (zig-check.sh).
export ZIG="$PWD/scripts/windows/zig-check.sh"
export RUSTFLAGS="-D warnings"
cd src-tauri
exec cargo xwin check --target x86_64-pc-windows-msvc "$@"
