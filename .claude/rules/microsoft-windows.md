---
paths:
  - "src-tauri/src/**/windows.rs"
  - "src-tauri/src/os/**"
  - "scripts/windows-vm.sh"
  - "scripts/windows-check.sh"
  - "scripts/windows/**"
---

# Windows (the OS)

What porting Codealong to Windows turned out to need, found on 2026-11-13 while it compiled from a
Mac (`scripts/windows-check.sh`) and from Windows' and the crates' own code. `windows.md` is about
the app's windows, not this. Paths are under `src-tauri/src/`.

- std has no Unix sockets on Windows, but Windows has had AF_UNIX since Windows 10 (1803):
  uds_windows wraps it with std's calls (`connect`, `bind`, `accept`, `pair`, timeouts), so the
  server's socket is the same file there (`os/sock.rs`). A socket handle stays the listener's,
  so `os::sock::id` is its number there, and `server.rs`'s check for a replaced listener (a macOS
  matter) always passes.
- An anonymous pipe can't be read or written without waiting the way POSIX's can: one set to
  `PIPE_NOWAIT` answers a read with nothing by `ERROR_NO_DATA`, which std reads as a broken pipe.
  So a command's pipes are threads on Windows (`os/procs/windows.rs` `Pipes`), stopped with
  `CancelSynchronousIo` once the command is done, as a process it left behind may hold them. And
  rpc.rs's writes to a child wait until it reads, so a write's deadline isn't kept there: a child
  that stops reading holds the line until it ends (not settled; it wants a write of its own
  thread, tried on Windows).
- Rust's `Command` finds `name.exe` alone on the PATH, never a `.cmd`, which is what npm installs
  for a command (`claude.cmd`). `os::procs::names_to_find` gives a program's names with PATHEXT's
  extensions, which `procs::command`, harness detection (detect.rs) and the editors use.
- Windows keeps a process's folder, environment and command line only in its own memory: the
  PEB's process parameters, read with `NtQueryInformationProcess` and `ReadProcessMemory`
  (`os/procs/windows.rs`, at the offsets every 64-bit Windows since 8 has, x64 and Arm64). Not
  for a 32-bit process, nor one Windows won't open for you (elevated, another user's). A process's
  parent pid outlives the parent and can be the next process's, so a parent that started after
  its child isn't taken for it.
- Windows has no `ps`: `os::procs::table` reads every process for `procs::list`, `find_child` and
  hygiene, and is None on macOS and Linux, which keep running `ps`.
- Apps started from the Start menu get the user's whole environment, so there's no login shell to
  ask (`procs::login_var`). `$HOME` isn't set there; util.rs `home` reads it first anyway, so a
  copy run with a home of its own keeps everything in it, as on macOS.
- Claude Code (2.1.323) names `C:\Program Files\ClaudeCode` for its managed settings on Windows,
  and keeps its sign-in in the config folder's `.credentials.json`, as on Linux. On Windows it
  needs Git Bash, whose `bash.exe` Git for Windows puts in `Git\bin`; System32's `bash.exe` is
  WSL's, which runs in Linux (`launch/windows.rs` `bash`). Claude Code's native installer puts
  `claude.exe` in `~\.local\bin`, which `procs::user_path` adds.
- Windows Terminal (`wt.exe`) takes a `;` on its command line for one of its own commands, takes
  no scripts and no keys from outside, and says nothing of its tabs: a new terminal runs its
  command from a script file (`launch/windows.rs`), and nothing can type into one or find it.
- A read-only file can't be deleted on Windows, and a folder's read-only flag stops nothing:
  `os::files::set_mode` sets the flag from the owner's write bit, and `mode_of` reads it back.
  Making a link takes Developer Mode or an administrator (`os::files::symlink`).
- A key combination with Ctrl and Alt is AltGr on many layouts (German, French…), which types
  `@`, `[` or `€`: the windows' Ctrl+Alt chords and the default shortcuts (Ctrl+Alt+/…) may take
  a character someone types. Not settled.

## Building

- ring compiles its C for `aarch64-pc-windows-msvc` with clang only, so Arm64 Windows needs
  Visual Studio's clang (`Microsoft.VisualStudio.Component.VC.Llvm.Clang`) on PATH; from a Mac it
  won't cross-compile for Arm64 at all (it calls plain clang with MSVC's `/imsvc` flags), so
  `windows-check.sh` checks x86_64.
- libghostty-vt-sys builds Ghostty with Zig, which finds no MSVC headers from a Mac, and
  `ZIG_LIBC` or `--libc` reach the host tools Ghostty's build compiles too, which then fail.
  `scripts/windows/zig-check.sh` builds it for the GNU ABI instead, enough for a check, which links
  nothing.

## The VM

- Microsoft's download page hands a script the ISO only after the ov-df.microsoft.com session
  handshake its page does (`scripts/windows/iso-url.sh`), and refuses VPN and datacenter
  addresses. It refused this Mac too on 2026-11-13, after a morning of tries ("Sentinel marked
  this request as rejected"): then `WINDOWS_ISO=<file>`, downloaded in a browser.
- The virtio-win ISO's Windows 11 driver files are hard links to its Server 2025 ones, so the
  whole driver's folder is extracted, and what's extracted is read-only.
- QEMU's `-nic` takes no `romfile=`: the network is `-netdev` and `-device virtio-net-pci`.
- The VM's machine boots to its firmware with both drives on 2026-11-13 (QEMU 11.1.3, HVF, an
  M1 Max); Windows' install itself hasn't run yet.
- A program started over SSH runs in session 0, no one's desktop: the app is started from a
  scheduled task in the logged-on user's session (`scripts/windows/run.ps1`).

## Unchecked

- 2026-11-13: everything at run time. The app, the server, the socket, the process table, the
  windows on the desktop, Windows Terminal starts, sounds, Open at login, Credential Manager and
  every harness's sessions there have only been compiled, and the tests have only compiled for
  Windows, never run there.
