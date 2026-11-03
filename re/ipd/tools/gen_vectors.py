"""Regenerate crates/amps/tests/fixtures/ipd_vectors.json by running the vendor's own packet code.

    IPD_SRC=<decompiled sources> IPD_PY27=<python2.7> python3 re/ipd/tools/gen_vectors.py

Commands are built and replies parsed by orig_harness_py27.py (the original d_protocol.py under
Python 2.7). The reply bytes it parses are made here, with the same layout the amp uses.
"""

import json
import os
import random
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE.parents[2] / "crates" / "amps" / "tests" / "fixtures" / "ipd_vectors.json"
PY27 = os.environ["IPD_PY27"]
CODE_TABLE = (233, 45, 98, 215, 123, 42, 99, 23, 178, 132, 200, 150, 100, 50, 77, 43, 54, 90, 31, 43,
              123, 234, 211, 177, 65, 133, 3, 64, 23, 83, 251, 1, 54, 34)


def orig(obj):
    r = subprocess.run([PY27, str(HERE / "orig_harness_py27.py")], input=json.dumps(obj).encode(), capture_output=True)
    if r.returncode:
        raise SystemExit(r.stderr.decode())
    return r.stdout.decode().strip()


def xor(data, period):
    return bytes(b ^ CODE_TABLE[i % period] for i, b in enumerate(data))


def build_reply(frags, mk2, pc_id, hw_id):
    body = b"".join(bytes((op, len(d) + 2)) + d for op, d in frags)
    cs = 0xD5
    for b in body:
        cs ^= b
    body += bytes((9, cs))
    if mk2:
        return b"DDW" + bytes(4) + bytes((pc_id, len(body) >> 8, len(body) & 0xFF)) + xor(body, 34)
    return b"DDA" + xor(body.ljust(36, b"\0"), 32) + bytes((pc_id, hw_id)) + b"Q"


random.seed(1)
cases = []
for mk2, items in [
    (False, [[[13, 33, 0, 0], None]]),
    (False, [[[0, 0, 128, 0], -250]]),
    (False, [[[7, 14, 129, 0], 1]]),
    (True, [[[0, 0, 128, 0], -250], [[7, 14, 128, 0], 1], [[5, 1, 128, 3], 1000], [[13, 16, 0, 0], "Amp  1          "]]),
    (True, [[[5, 2, 0, 9], 7], [[2, 8, 0x81, 0], 2500000], [[11, 26, 0, 1], 1], [[13, 29, 0, 0], "Password"]]),
    (True, []),
    (False, []),
    (True, [[[5, 0, 128, b], None] for b in range(10)]),
]:
    hw, my = random.randint(0, 254), random.randint(1, 254)
    cases.append({"type": "command", "mk2": mk2, "items": items, "hw_id": hw, "my_id": my,
                  "hex": orig({"op": "build", "mk2": mk2, "items": items, "hw_id": hw, "my_id": my})})

for mk2, frags in [
    (True, [(1, bytes([0xec, 0xe9])), (2, bytes([0x9c, 0x9c])), (7, b"\0\0"), (8, b"\x03\0"),
            (5, bytes([0, 0, 128, 0]) + (-250).to_bytes(2, "big", signed=True)),
            (5, bytes([13, 16, 0, 0]) + b"Amp 1".ljust(16))]),
    (True, [(5, bytes([13, 37, 0, 0]) + bytes([80, 82, 255, 41])), (12, bytes([100, 60, 90, 50]))]),
    (False, [(5, bytes([13, 33, 0, 0]) + (0x01020B).to_bytes(4, "big"))]),
    (False, [(1, bytes([0xf0, 0xf1])), (2, bytes([0x9c, 0x9c]))]),
]:
    pkt = build_reply(frags, mk2, pc_id=5, hw_id=9)
    cases.append({"type": "reply", "mk2": mk2, "hex": pkt.hex(),
                  "fragments": json.loads(orig({"op": "parse", "hex": pkt.hex()}))})

OUT.write_text(json.dumps(cases, indent=1))
print(f"{len(cases)} vectors -> {OUT}")
