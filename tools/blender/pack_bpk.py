#!/usr/bin/env python3
"""Pack rendered frames into the BPK2 movie format read by onboarding.rs.

Layout: b"BPK2", u32 frame count, N x (u32 absolute offset, u32 length), then
JPEG payloads. Identical (held) frames share one payload.
usage: pack_bpk.py FRAMES_DIR OUT.bpk
"""
import os, struct, sys

N = 480
HOLD_START_END, HOLD_END_START = 59, 405    # must match plugin_intro.py

def source_frame(f):
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

header = 8 + N * 8
offsets, payloads, cursor = {}, [], header
for f in uniq:
    data = open(os.path.join(frames_dir, f"f{f:04d}.jpg"), "rb").read()
    offsets[f] = (cursor, len(data))
    payloads.append(data)
    cursor += len(data)

with open(out, "wb") as fh:
    fh.write(b"BPK2" + struct.pack("<I", N))
    for f in range(N):
        fh.write(struct.pack("<II", *offsets[source_frame(f)]))
    for p in payloads:
        fh.write(p)
print(f"wrote {out}: {N} frames, {len(uniq)} unique, {cursor / 1e6:.1f} MB")
