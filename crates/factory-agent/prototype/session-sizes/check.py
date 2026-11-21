"""Cross-check: Claude Code's own cost count against the transcripts, and how long calls take.

usage: check.py <transcript directory>
"""
import json, os, glob, collections, datetime, sys
D = os.path.expanduser(sys.argv[1])
def ts(s): return datetime.datetime.fromisoformat(s.replace('Z', '+00:00')).timestamp()
tot_t = collections.Counter(); tot_c = collections.Counter()
pos = []
calldur = []; tooldur = []; qsrc = collections.Counter()
for f in sorted(glob.glob(D + '/*.jsonl')):
    sid = os.path.basename(f)[:-6]
    calls = {}; cs = []; n = 0; cs_pos = []; last_ts = None; cs_after = []
    files = [f] + [x for x in glob.glob(D + '/' + sid + '/**/agent-*.jsonl', recursive=True)]
    for ff in files:
        req = {}; first = {}; lastl = {}; tu = {}
        for line in open(ff):
            e = json.loads(line); t = e.get('type')
            if ff == f:
                n += 1
                if t == 'cost-state': cs.append(e); cs_pos.append(n)
            if t == 'api-request':
                req[e.get('id')] = ts(e['timestamp']); qsrc[e.get('querySource')] += 1
            if t == 'assistant' and e['message'].get('model') != '<synthetic>':
                m = e['message']; calls[m['id']] = (m['model'], m['usage'])
                r = e.get('requestRef'); r = r.get('id') if isinstance(r, dict) else r
                if r in req: lastl[m['id']] = (req[r], ts(e['timestamp']))
                for b in m.get('content') or []:
                    if b.get('type') == 'tool_use': tu[b['id']] = ts(e['timestamp'])
            if t == 'user' and not isinstance(e['message'].get('content'), str):
                for b in e['message']['content']:
                    if b.get('type') == 'tool_result' and b.get('tool_use_id') in tu: tooldur.append(ts(e['timestamp']) - tu[b['tool_use_id']])
        calldur.extend(b - a for a, b in lastl.values())
    if not cs: continue
    last = cs[-1]
    mine = collections.defaultdict(collections.Counter)
    for model, u in calls.values():
        cc = u.get('cache_creation') or {}
        mine[model].update(dict(inputTokens=u.get('input_tokens', 0), outputTokens=u.get('output_tokens', 0), cacheReadInputTokens=u.get('cache_read_input_tokens', 0), cacheCreationEphemeral5mInputTokens=cc.get('ephemeral_5m_input_tokens', 0), cacheCreationEphemeral1hInputTokens=cc.get('ephemeral_1h_input_tokens', 0)))
    pos.append((sid[:8], cs_pos, n))
    for model, mu in (last.get('modelUsage') or {}).items():
        for k in ('inputTokens', 'outputTokens', 'cacheReadInputTokens', 'cacheCreationEphemeral5mInputTokens', 'cacheCreationEphemeral1hInputTokens'):
            tot_c[(model, k)] += mu.get(k, 0); 
        tot_c[(model, 'costUSD')] += mu.get('costUSD', 0); tot_c[(model, 'webSearchRequests')] += mu.get('webSearchRequests', 0)
    for model in mine:
        for k, v in mine[model].items(): tot_t[(model, k)] += v
print('cost-state line position / total lines per session:', pos)
print('| model | category | cost-state (last line) | transcript | transcript/cost-state |')
for k in sorted(set(tot_c) | set(tot_t)):
    a, b = tot_c.get(k, 0), tot_t.get(k, 0)
    print('|', k[0], '|', k[1], '|', f'{a:,.2f}' if isinstance(a, float) else f'{a:,}', '|', f'{b:,}', '|', f'{b / a:.3f}' if a else '-', '|')
def pct(v, q):
    v = sorted(v); k = (len(v) - 1) * q; lo = int(k); hi = min(lo + 1, len(v) - 1); return v[lo] + (v[hi] - v[lo]) * (k - lo)
for name, v in (('model call duration s (api-request -> last block)', calldur), ('tool duration s (tool_use line -> tool_result line)', tooldur)):
    print(name, 'n', len(v), {q: round(pct(v, q), 1) for q in (.5, .75, .9, .95, .99, 1)}, '>300s:', sum(1 for x in v if x > 300), '>600s:', sum(1 for x in v if x > 600), '>1800s:', sum(1 for x in v if x > 1800))
print('long model calls (s):', sorted(round(x) for x in calldur if x > 300))
print('long tool durations (s):', sorted(round(x) for x in tooldur if x > 300))
print('querySource', dict(qsrc))
