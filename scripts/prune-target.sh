#!/usr/bin/env bash
# Deletes what the shared build directory (scripts/target-dir.sh) holds that no build has used for
# a while, and prints how much that freed. With many worktrees building it grows by gigabytes a
# day: dependencies and test binaries of versions no checkout builds any more, and incremental
# caches for them. relaunch.sh runs it, in the background, at most once a day.
#
#   scripts/prune-target.sh [days]   what nothing has read for that many days (default 2)
#
# Cargo reads every library it links and this volume records when a file was last read, so what a
# build still uses doesn't go unread for days. A build that had already found a file fresh could
# still be about to read it, so files are deleted only under cargo's own lock on their profile's
# folder (debug, release, <triple>/release), as no build there runs, in batches that let waiting
# builds take their turn between them. A build that finds one gone later compiles it again. Object
# files left beside a binary (debug builds with line tables left hundreds each) serve only a
# debugger, and go once they're an hour old.
set -euo pipefail
cd "$(dirname "$0")/.."
target="$(scripts/target-dir.sh)"
exec python3 -I - "$target" "${1:-2}" <<'EOF'
import fcntl, os, shutil, sys, time

target, days = sys.argv[1], float(sys.argv[2])
now = time.time()
unread = now - days * 86400
BATCH = 2000


def stale(path, st):
    if path.endswith(".rcgu.o") and st.st_mtime < now - 3600:
        return True
    # Cargo lets go of its lock before it runs a cargo test's binaries, one after another, so one
    # it found fresh can be waiting its turn unread. Executables (test binaries, the engine) wait
    # a week for each day the rest wait: only a checkout no one built in for that long could trip
    # over one gone, and its test run failing once builds it again.
    if st.st_mode & 0o111:
        return st.st_atime < now - days * 7 * 86400
    return st.st_atime < unread


def size(path):
    total = 0
    for root, _, files in os.walk(path):
        for f in files:
            try:
                total += os.lstat(os.path.join(root, f)).st_blocks * 512
            except OSError:
                pass
    return total


# A profile's folder has deps and cargo's lock: debug and release at the top, and a target's
# release a level down.
profiles, level = [], [target]
for _ in range(2):
    below = []
    for d in level:
        try:
            names = os.listdir(d)
        except OSError:
            continue
        for n in names:
            p = os.path.join(d, n)
            if not os.path.isdir(p) or os.path.islink(p):
                continue
            if os.path.isdir(os.path.join(p, "deps")) and os.path.exists(os.path.join(p, ".cargo-lock")):
                profiles.append(p)
            else:
                below.append(p)
    level = below

freed = 0
for profile in profiles:
    # Found without the lock, which is taken only to check each again and delete it.
    found = []
    with os.scandir(os.path.join(profile, "deps")) as it:
        for e in it:
            try:
                if e.is_file(follow_symlinks=False) and stale(e.path, e.stat(follow_symlinks=False)):
                    found.append(e.path)
            except OSError:
                pass
    incremental = os.path.join(profile, "incremental")
    if os.path.isdir(incremental):
        with os.scandir(incremental) as it:
            for e in it:
                try:
                    if e.is_dir(follow_symlinks=False) and e.stat(follow_symlinks=False).st_mtime < unread:
                        found.append(e.path)
                except OSError:
                    pass
    lock = os.open(os.path.join(profile, ".cargo-lock"), os.O_RDWR)
    try:
        for i in range(0, len(found), BATCH):
            fcntl.flock(lock, fcntl.LOCK_EX)
            try:
                for path in found[i : i + BATCH]:
                    try:
                        st = os.lstat(path)
                        if os.path.isdir(path):
                            if st.st_mtime < unread:
                                freed += size(path)
                                shutil.rmtree(path, ignore_errors=True)
                        elif stale(path, st):
                            os.unlink(path)
                            freed += st.st_blocks * 512
                    except OSError:
                        pass
            finally:
                fcntl.flock(lock, fcntl.LOCK_UN)
            time.sleep(0.5)
    finally:
        os.close(lock)

print(f"Pruned {target}: {freed / 2**30:.1f} GB freed.")
EOF
