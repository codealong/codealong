#!/usr/bin/env python3
# chntpw, as UUP dump's convert.sh asks it of a registry hive, for uup-iso.sh, which puts this
# first on the PATH as `chntpw` while convert.sh runs: `chntpw -e <hive>`, with its commands on
# stdin, which make the two values Setup needs in boot.wim (SystemRoot and WinPE's InstRoot).
# Homebrew has no chntpw (a tap builds it against OpenSSL 1.0), but has hivex, whose library sets
# one value and leaves a key's others as they are (hivex_node_set_value; hivexsh's setval replaces
# them all). It takes only the commands convert.sh sends: `cd <key>` from the key it's at, `nv 1
# <name>` (a string value), `ed <name>` with the value on the next line, `q`, and `y` to save. Any
# other fails, so a convert.sh that asks for more says so rather than building a broken ISO; since
# convert.sh goes on after a command fails, a failure also makes the file CHNTPW_FAILED names,
# which uup-iso.sh looks for. HIVEX_LIB names libhivex.dylib; uup-iso.sh sets both.
import ctypes
import ctypes.util
import os
import sys


class SetValue(ctypes.Structure):
    _fields_ = [("key", ctypes.c_char_p), ("t", ctypes.c_int), ("len", ctypes.c_size_t), ("value", ctypes.c_char_p)]


REG_SZ = 1
# hivex.h: HIVEX_OPEN_WRITE (2 is HIVEX_OPEN_DEBUG).
OPEN_WRITE = 4


def fail(why):
    if os.environ.get("CHNTPW_FAILED"):
        with open(os.environ["CHNTPW_FAILED"], "a") as failed:
            failed.write(f"{why}\n")
    sys.exit(f"chntpw (hivex's stand-in): {why}")


if len(sys.argv) != 3 or sys.argv[1] != "-e" or not os.path.isfile(sys.argv[2]):
    fail("only `-e <hive>` is done")
lib = ctypes.CDLL(os.environ.get("HIVEX_LIB") or ctypes.util.find_library("hivex") or fail("no libhivex: brew install hivex"), use_errno=True)
lib.hivex_open.restype = ctypes.c_void_p
lib.hivex_open.argtypes = [ctypes.c_char_p, ctypes.c_int]
lib.hivex_root.restype = ctypes.c_size_t
lib.hivex_root.argtypes = [ctypes.c_void_p]
lib.hivex_node_get_child.restype = ctypes.c_size_t
lib.hivex_node_get_child.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_char_p]
lib.hivex_node_set_value.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.POINTER(SetValue), ctypes.c_int]
lib.hivex_commit.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
lib.hivex_close.argtypes = [ctypes.c_void_p]

hive = lib.hivex_open(sys.argv[2].encode(), OPEN_WRITE)
if not hive:
    fail(f"couldn't open {sys.argv[2]}: {os.strerror(ctypes.get_errno())}")
node = lib.hivex_root(hive)
made = {}
lines = iter(sys.stdin.read().splitlines())
for line in lines:
    command, _, rest = line.strip().partition(" ")
    if command == "cd":
        for part in rest.split("\\"):
            if part:
                node = lib.hivex_node_get_child(hive, node, part.encode()) or fail(f"no key {part!r} under the one it's at")
    elif command == "nv":
        kind, _, name = rest.partition(" ")
        if kind != str(REG_SZ):
            fail(f"a value of type {kind} isn't done, only strings")
        made[name] = node
    elif command == "ed":
        if made.get(rest) != node:
            fail(f"{rest!r} wasn't made with nv at this key")
        text = (next(lines, "") + "\0").encode("utf-16-le")
        value = SetValue(rest.encode(), REG_SZ, len(text), text)
        if lib.hivex_node_set_value(hive, node, ctypes.byref(value), 0) != 0:
            fail(f"couldn't set {rest!r}: {os.strerror(ctypes.get_errno())}")
    elif command == "y":
        if lib.hivex_commit(hive, None, 0) != 0:
            fail(f"couldn't save {sys.argv[2]}: {os.strerror(ctypes.get_errno())}")
    elif command in ("q", ""):
        pass
    else:
        fail(f"doesn't know {line!r}")
lib.hivex_close(hive)
