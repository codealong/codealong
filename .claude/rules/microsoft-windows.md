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
  So a command's pipes are threads on Windows (`os/procs/windows.rs` `Pipes`), stopped once the
  command is done, as a process it left behind may hold them. std reads and writes a child's
  pipes as overlapped I/O (Rust 1.98), which `CancelIoEx` on the pipe cancels and
  `CancelSynchronousIo` on the thread doesn't. And
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
- A window's page measures itself in its own monitor's points, and `os/windows/windows.rs`
  counts every rectangle in the primary monitor's, so place and size add up: on a monitor scaled
  otherwise than the primary, a panel comes out too big or too small by their ratio. Not settled;
  it wants each monitor's points mapped onto the desktop's.
- A command line on Windows is kept as it was started, quoted by Windows' rules: util.rs `args`
  splits it so, where `ps`'s on macOS and Linux are split at spaces.
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
- Ghostty's static library bundles Zig's compiler runtime on every OS (`GhosttyLibVt.zig`
  `bundle_compiler_rt`), whose weak definition of `__udivti3` Microsoft's link.exe takes for a
  conflict with Rust's compiler_builtins on Arm64 (LNK1227); lld-link, from the same clang, links
  it (`.cargo/config.toml`). x86_64 hasn't been linked.
- Tauri embeds its manifest, the one asking for Common Controls 6, in the app's binary alone, and
  the engine imports `TaskDialogIndirect`, which only that comctl32.dll has: a test binary without
  it isn't started (STATUS_ENTRYPOINT_NOT_FOUND), so build.rs has the linker write it into every
  binary (`/MANIFESTDEPENDENCY`, which link.exe and lld-link both take without merging files).
- Windows PowerShell 5.1, SSH's shell in the VM: `Invoke-RestMethod` hands a JSON array on as one
  object (in parentheses it goes one item at a time); `~` is expanded for its own commands only,
  never for a program's arguments (tar.exe gets `$HOME`); and it runs no local script, npm's
  `.ps1` shims included, until the machine's policy is RemoteSigned (tools.ps1).
- npm 11 on a fresh Windows: `npm ls -g` fails until `%APPDATA%\npm` exists, and Claude Code's
  postinstall isn't run (install scripts need allowing); `claude` runs without it (2.1.325).

## The VM

- Microsoft's download page hands a script the ISO only after the ov-df.microsoft.com session
  handshake its page does (`scripts/windows/iso-url.sh`), and refuses VPN and datacenter
  addresses. It refused this Mac on 2026-11-13, after a morning of tries ("Sentinel marked this
  request as rejected"), and a browser here too. Then the ISO is made from Windows Update's files
  (`scripts/windows/uup-iso.sh`): UUP dump lists them on Microsoft's update servers (68 files, 9 GB
  for 26H2's Pro in English, its cumulative update among them) and hands out its convert.sh,
  pinned by hash, which runs on macOS with Homebrew's aria2, cabextract, wimlib and cdrtools. It
  asks chntpw, which Homebrew hasn't, to set two values in boot.wim's registry; hivex's library
  sets them instead (`scripts/windows/chntpw.py`; hivexsh's `setval` would replace the key's
  other values).
- The ISO convert.sh makes is the release's base build (26100.1 for 24H2 to 26H2: they share
  24H2's base), without updates, its install image named "Windows 11 Professional" where
  Microsoft's is "Windows 11 Pro"; both describe it as "Windows 11 Pro", which the answer file
  picks it by. Its boot catalog has a BIOS image first and the EFI one after, both said to be for
  x86 (mkisofs's `-b` alone), which UEFI boots all the same; noprompt.py finds efisys.bin among
  them. Its Windows PE, made from the recovery image, hangs as anything starts its network
  (`wpeutil InitializeNetwork`, Setup's first step, a `drvload` of NetKVM behind them), whatever
  the NIC: the answer file turns Setup's network off (`EnableNetwork`), which Setup doesn't need.
- The firmware reads Windows PE (boot.wim, 450 MB) over emulated USB, which took from 3 to 15
  minutes before Setup's window showed (2026-11-13), the spinner under the firmware's logo all
  the while. Setup's own copying is quick.
- The virtio-win ISO's Windows 11 driver files are hard links to its Server 2025 ones, so the
  whole driver's folder is extracted, and what's extracted is read-only.
- HVF has no GICv2 (QEMU refuses `gic-version=2`); `its=off`, `vectors=0` and
  `disable-legacy=on` on virtio-net changed nothing of the hang above. Windows PE has no driver
  for `usb-net` or `e1000e` either.
- QEMU's `-nic` takes no `romfile=`: the network is `-netdev` and `-device virtio-net-pci`.
- Windows installed from the UUP ISO on 2026-11-13 (QEMU 11.1.3, HVF, an M1 Max whose memory
  other work had full, which made everything slow): Setup's copying took 42 minutes, its second
  phase and OOBE 25 more. OOBE stopped once at "Something went wrong" (OOBEZDP, its own check for
  updates) until Skip was clicked (`windows-vm.sh click`). Its log (Panther\UnattendGC\setupact.log)
  has the page giving up (`AbortZDP`) before the check, which went on in the background, had
  searched, so a VM that isn't starved may not stop there. Microsoft documents no way to leave the
  check out; `DisableCloudOptimizedContent`, which forums offer, is a policy Pro ignores. In the
  installed Windows NetKVM works (10.0.2.15 from QEMU's NAT), and OpenSSH came from Windows Update.
- Windows' OpenSSH makes its firewall rule (OpenSSH-Server-In-TCP) for private networks alone,
  and Windows takes QEMU's network for a public one: sshd listened and SSH timed out at the
  banner until firstlogon.ps1 opened the rule to every profile.
- A program started over SSH runs in session 0, no one's desktop: the app is started from a
  scheduled task in the logged-on user's session (`scripts/windows/run.ps1`).

## Unchecked

- 2026-11-13: everything at run time. The app, the server, the socket, the process table, the
  windows on the desktop, Windows Terminal starts, sounds, Open at login, Credential Manager and
  every harness's sessions there have only been compiled. The engine's tests ran in the VM on
  2026-11-13: 1989 passed and 128 failed, most of them tests that start `/bin/sh` or
  `/bin/sleep` as a stand-in, fixtures with Unix paths, and canonical paths in Windows' `\\?\`
  form; the binary's and codealong-peer's tests haven't run.
