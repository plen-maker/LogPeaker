#!/usr/bin/env python3
"""Pack rendered frames into the BPK2 movie format read by onboarding.rs.

Layout: b"BPK2", u32 frame count, N x (u32 absolute offset, u32 length), then
JPEG payloads. Identical (held) frames share one payload.
usage: pack_bpk.py FRAMES_DIR OUT.bpk [--identity [--first N]]

--dedupe: frames with byte-identical JPEGs share one payload.
--identity: every output frame comes from its own file (no held-frame sharing);
--holds A,B: (with --identity) output frames <= A repeat frame 0 and frames >= B repeat frame B;
--first N: the number of the file that becomes frame 0 (Blender scenes start at 1).
"""
import os, struct, sys

N = 480
HOLD_START_END, HOLD_END_START = 59, 405    # must match plugin_intro.py

IDENTITY = "--identity" in sys.argv
HOLDS = tuple(int(v) for v in sys.argv[sys.argv.index("--holds") + 1].split(",")) if "--holds" in sys.argv else None
FIRST = int(sys.argv[sys.argv.index("--first") + 1]) if "--first" in sys.argv else 0

def source_frame(f):
    if IDENTITY:
        if HOLDS:
            f = 0 if f <= HOLDS[0] else (HOLDS[1] if f >= HOLDS[1] else f)
        return f + FIRST
    if f <= HOLD_START_END:
        return 0
    if f >= HOLD_END_START:
        return HOLD_END_START
    return f

frames_dir, out = sys.argv[1], sys.argv[2]
uniq = sorted({source_frame(f) for f in range(N)})
missing = [f for f in uniq if not os.path.exists(os.path.join(frames_dir, f"f{f:04d}.jpg"))]
if missing:
    sys.exit(f"missing {len(missing)} rendered frames, e.g. {missing[:5]}")

import hashlib
header = 8 + N * 8
offsets, payloads, cursor = {}, [], header
seen = {}
for f in uniq:
    data = open(os.path.join(frames_dir, f"f{f:04d}.jpg"), "rb").read()
    key = hashlib.sha1(data).hexdigest() if "--dedupe" in sys.argv else None
    if key in seen:
        offsets[f] = seen[key]
        continue
    offsets[f] = (cursor, len(data))
    if key:
        seen[key] = offsets[f]
    payloads.append(data)
    cursor += len(data)

with open(out, "wb") as fh:
    fh.write(b"BPK2" + struct.pack("<I", N))
    for f in range(N):
        fh.write(struct.pack("<II", *offsets[source_frame(f)]))
    for p in payloads:
        fh.write(p)
print(f"wrote {out}: {N} frames, {len(payloads)} payloads, {cursor / 1e6:.1f} MB")
