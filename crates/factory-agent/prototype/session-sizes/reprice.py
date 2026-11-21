"""Reprice every ticket with its claude-fable-5-2 tokens at claude-opus-5-5 list prices.

usage: reprice.py <json output of measure.py>
"""
import json, sys
from collections import defaultdict
OPUS = dict(inp=4.0, cw5=5.0, cw1=8.0, cr=0.20, out=20.0)
FABLE = dict(inp=8.0, cw5=10.0, cw1=16.0, cr=0.20, out=40.0)
def usd(t, p): return sum(t.get(k, 0) * p[k] for k in p) / 1e6
def pct(v, q):
    v = sorted(v); i = (len(v) - 1) * q; lo = int(i); hi = min(lo + 1, len(v) - 1)
    return v[lo] + (v[hi] - v[lo]) * (i - lo)
d = json.load(open(sys.argv[1]))
by = defaultdict(list)
for s in d['segments']:
    if s['idx'] >= 0: by[(s['session'], s['ctx'])].append(s)
# a ticket with a prompt still in progress is left out whole, as measure.py leaves it out
by = {k: g for k, g in by.items() if not any(s['in_progress'] for s in g)}
def opus_usd(s):
    f = s['tok'].get('claude-fable-5-2', {})
    return s['usd'] - usd(f, FABLE) + usd(f, OPUS)
tickets, firsts = [], []
for segs in by.values():
    segs.sort(key=lambda s: s['idx'])
    if 'implement' not in str(segs[0]['kind']): continue
    tickets.append((sum(s['usd'] for s in segs), sum(opus_usd(s) for s in segs)))
    if segs[0]['calls'] >= 10: firsts.append(opus_usd(segs[0]))
big = [opus_usd(s) for s in d['segments'] if s['idx'] >= 0 and not s['in_progress'] and s['calls'] >= 10]
def row(name, v, k=1.0):
    print(f"{name:42s} n={len(v):3d} " + ' '.join(f"{lab}={pct(v,q)*k:6.2f}" for lab, q in [('p50',.5),('p75',.75),('p90',.9),('p95',.95),('max',1)]))
o = [t[1] for t in tickets]
row('ticket, opus prices', o); row('ticket, opus prices, +6%', o, 1.06)
row('first prompt >=10 calls, opus', firsts); row('one prompt >=10 calls, opus', big)
print('ratio opus/fable per ticket:', ' '.join(f"{b/a:.2f}" for a, b in sorted(tickets)))
print('pooled ratio:', round(sum(o) / sum(t[0] for t in tickets), 3))
for lim in (13.5, 15, 18, 20, 22.5, 25, 27):
    print(f"raised dollars over {lim}: {sum(x*1.06 > lim for x in o)} of {len(o)}")
print('sorted raised:', ' '.join(f"{x*1.06:.2f}" for x in sorted(o)))
