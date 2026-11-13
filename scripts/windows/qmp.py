#!/usr/bin/env python3
# qmp.py <socket> '<json command>'…: runs each command on a QEMU's monitor socket (QMP) and prints
# each reply, for windows-vm.sh (screendump, send-key, quit). QMP takes commands only once
# qmp_capabilities has been asked, and sends events between replies, which are skipped. Exits 1
# when QEMU answers a command with an error.
import json
import socket
import sys

s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
f = s.makefile("rw")
json.loads(f.readline())  # the greeting
failed = False
for command in [{"execute": "qmp_capabilities"}] + [json.loads(a) for a in sys.argv[2:]]:
    f.write(json.dumps(command) + "\n")
    f.flush()
    while True:
        line = f.readline()
        if not line:  # QEMU closed the socket, as it does once asked to quit
            sys.exit(0)
        reply = json.loads(line)
        if "event" not in reply:
            break
    failed = failed or "error" in reply
    if command["execute"] != "qmp_capabilities":
        print(json.dumps(reply))
sys.exit(1 if failed else 0)
