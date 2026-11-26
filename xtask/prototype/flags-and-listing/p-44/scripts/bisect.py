#!/usr/bin/env python3
"""bisect.py <spec.json> : run size experiments on the chain branch probe/p-44-bisect.

spec: [{"label": "...", "files": [["lines", "p44/bis/a.txt", 400, 40], ["longline", path, bytes, nlines], ["edit", path, ...]]}]
Each experiment is one commit that replaces p44/bis/ entirely; the observation is the logged-out compare view
probe/p-44-base...<sha>. Results are appended to evidence/bisect-results.tsv.
"""
import json, os, subprocess, sys, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CLONE = os.path.join(ROOT, "clone")
SCR = os.path.join(ROOT, "scripts")
EV = os.path.join(ROOT, "evidence", "compare")
os.makedirs(EV, exist_ok=True)


def git(*a, check=True):
    return subprocess.run(["git", "-C", CLONE, *a], check=check, capture_output=True, text=True).stdout.strip()


spec = json.load(open(sys.argv[1]))
have = git("branch", "--list", "probe/p-44-bisect")
if have:
    git("checkout", "-q", "probe/p-44-bisect")
else:
    git("checkout", "-q", "-b", "probe/p-44-bisect", "probe/p-44-base")
done = []
for exp in spec:
    git("rm", "-rq", "--ignore-unmatch", "p44/bis")
    for f in exp["files"]:
        subprocess.run(["python3", os.path.join(SCR, "mk.py"), f[0], os.path.join(CLONE, f[1]), *[str(x) for x in f[2:]]], check=True)
    git("add", "-A")
    git("commit", "-q", "--allow-empty", "-m", "Probe P-44 bisect: " + exp["label"])
    done.append((exp["label"], git("rev-parse", "HEAD")))
subprocess.run(["git", "-C", CLONE, "push", "-q", "origin", "probe/p-44-bisect"], check=True, capture_output=True)
time.sleep(3)
res = open(os.path.join(ROOT, "evidence", "bisect-results.tsv"), "a")
for label, sha in done:
    stem = os.path.join(EV, "bis-" + label)
    subprocess.run(["python3", "-I", os.path.join(SCR, "fetch.py"), f"/sapientpants/sf-scratch/compare/probe/p-44-base...{sha}", stem], check=True, capture_output=True)
    frags = sorted(x for x in os.listdir(EV) if x.startswith("bis-" + label + ".") and x.endswith(".html"))
    out = subprocess.run(["python3", "-I", os.path.join(SCR, "parse_files.py"), *[os.path.join(EV, x) for x in frags]], capture_output=True, text=True).stdout
    for line in out.splitlines()[1:]:
        c = line.split("\t")
        if c[0].startswith("#files"):
            continue
        if c[0].startswith("#"):
            row = [label, sha[:10], *c]
        else:
            msg = c[6].split(" Oops, something")[0][:160]
            row = [label, sha[:10], c[0], c[2], c[5], msg]
        print("\t".join(row))
        res.write("\t".join(row) + "\n")
    time.sleep(1)
