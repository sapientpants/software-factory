#!/usr/bin/env python3
"""mk.py lines <path> <n> [width] [tag] | longline <path> <bytes> [nlines]  -- deterministic text files."""
import os, sys
WORDS = "alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima".split()

def text_lines(n, tag="line", width=40, start=0):
    out = []
    for i in range(start, start + n):
        s = f"{tag} {i:06d} " + " ".join(WORDS[(i + k) % len(WORDS)] for k in range(40))
        out.append(s[:width].ljust(width, "."))
    return "\n".join(out) + "\n"

cmd, path = sys.argv[1], sys.argv[2]
os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
if cmd == "lines":
    n = int(sys.argv[3]); width = int(sys.argv[4]) if len(sys.argv) > 4 else 40
    tag = sys.argv[5] if len(sys.argv) > 5 else "line"
    data = text_lines(n, tag, width)
elif cmd == "longline":
    size = int(sys.argv[3]); nl = int(sys.argv[4]) if len(sys.argv) > 4 else 1
    per = size // nl
    data = ""
    for j in range(nl):
        s = " ".join(f"{WORDS[(i + j) % len(WORDS)]}{i}" for i in range(per // 6 + 2))
        data += s[:per - 1] + "\n"
open(path, "w").write(data)
