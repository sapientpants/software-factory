#!/usr/bin/env python3
"""Aggregate sizes of Claude Code work from transcripts. Read-only; prints aggregates only.

usage: measure.py <transcript directory> [<id of a session to leave out as in progress>] [<json output>]
"""
import json, os, glob, re, sys, collections, datetime, math

D = os.path.expanduser(sys.argv[1])                 # e.g. ~/.claude/projects/<project>
CURRENT = sys.argv[2] if len(sys.argv) > 2 else ''  # the session this measurement runs in, if any
OUT = sys.argv[3] if len(sys.argv) > 3 else None    # per-prompt and per-session figures; holds session ids
import time
NOW = time.time()
GAP = 300.0  # seconds; larger gaps are not "active"

# USD per million tokens. Source: platform.claude.com/docs/en/about-claude/pricing (fetched 2026-11-21),
# agrees with the claude-api skill (shared/models.md, shared/prompt-caching.md).
PRICES = {
    'claude-fable-5-2': dict(inp=8.0, cw5=10.0, cw1=16.0, cr=0.20, out=40.0),
    'claude-opus-5-5':  dict(inp=4.0, cw5=5.0,  cw1=8.0,  cr=0.20, out=20.0),
    'claude-haiku-5-5': dict(inp=0.10, cw5=0.125, cw1=0.20, cr=0.01, out=0.50),          # prompt <= 100K
    'claude-haiku-5-5:long': dict(inp=0.50, cw5=0.625, cw1=1.0, cr=0.05, out=2.50),      # prompt > 100K
}
WEB_SEARCH = 10.0 / 1000

def ts(s):
    return datetime.datetime.fromisoformat(s.replace('Z', '+00:00')).timestamp()

def okind(e):
    o = e.get('origin')
    return o.get('kind') if isinstance(o, dict) else o

def user_text(e):
    c = e['message'].get('content')
    if isinstance(c, str): return c
    if any(b.get('type') == 'tool_result' for b in c): return None
    return ''.join(b.get('text', '') for b in c if b.get('type') == 'text')

def is_human_prompt(e):
    return e.get('type') == 'user' and not e.get('isMeta') and okind(e) == 'human' and user_text(e) is not None

def command_name(text):
    m = re.search(r'<command-name>\s*(/?[A-Za-z0-9:_.-]+)\s*</command-name>', text or '')
    return m.group(1) if m else None

def call_cost(model, u):
    cc = u.get('cache_creation') or {}
    cw5 = cc.get('ephemeral_5m_input_tokens', 0) or 0
    cw1 = cc.get('ephemeral_1h_input_tokens', 0) or 0
    cw = u.get('cache_creation_input_tokens', 0) or 0
    if cw5 + cw1 != cw:  # fall back: unattributed writes priced as 5m
        cw5 = cw - cw1
    inp = u.get('input_tokens', 0) or 0; out = u.get('output_tokens', 0) or 0; cr = u.get('cache_read_input_tokens', 0) or 0
    key = model
    if model == 'claude-haiku-5-5' and inp + cr + cw > 100_000: key = model + ':long'
    p = PRICES.get(key)
    ws = ((u.get('server_tool_use') or {}).get('web_search_requests', 0) or 0)
    tok = dict(inp=inp, out=out, cr=cr, cw5=cw5, cw1=cw1)
    if p is None: return None, tok, ws
    usd = (inp * p['inp'] + out * p['out'] + cr * p['cr'] + cw5 * p['cw5'] + cw1 * p['cw1']) / 1e6 + ws * WEB_SEARCH
    return usd, tok, ws

class Seg:
    def __init__(self, session, idx, t0, kind):
        self.session, self.idx, self.t0, self.kind = session, idx, t0, kind
        self.events = []          # (timestamp, category)
        self.calls = {}           # message id -> (chain, model, usage)
        self.tools = {}           # tool_use id -> chain
        self.turn_ms = 0
        self.in_progress = False
        self.peer_msgs = 0; self.queued_human = 0; self.ctx = 0

def new_stats():
    return dict()

segments = []; sessions = []
anomalies = collections.Counter()
top = sorted(glob.glob(D + '/*.jsonl'))
for f in top:
    sid = os.path.basename(f)[:-6]
    segs = []; cur = None
    pre = Seg(sid, -1, None, '(before first human prompt)')
    pid2seg = {}; mid_owner = {}; tool_owner = {}; ctx = 0
    cost_states = []
    def add_assistant(seg, e, chain):
        m = e['message']; model = m.get('model')
        if model == '<synthetic>' or e.get('isApiErrorMessage'):
            anomalies['synthetic/error assistant lines skipped'] += 1; return
        mid = m.get('id')
        if not mid: anomalies['assistant line without message.id'] += 1; return
        owner = mid_owner.setdefault(mid, seg)
        if owner is not seg: anomalies['message.id lines straddling a segment boundary (kept in first segment)'] += 1
        owner.calls[mid] = (chain, model, m.get('usage') or {})   # last line wins
        for b in m.get('content') or []:
            if b.get('type') == 'tool_use': tool_owner.setdefault(b.get('id'), owner).tools[b.get('id')] = chain
    for line in open(f):
        e = json.loads(line); t = e.get('type')
        if t == 'cost-state': cost_states.append(e); continue
        if is_human_prompt(e):
            cur = Seg(sid, len(segs), ts(e['timestamp']), command_name(user_text(e)) or '(text)')
            cur.events.append((cur.t0, 'prompt')); cur.ctx = ctx
            segs.append(cur)
            if e.get('promptId'): pid2seg[e['promptId']] = cur
            continue
        seg = cur or pre
        if t == 'user':
            if e.get('promptId') and e['promptId'] not in pid2seg: pid2seg[e['promptId']] = seg
            txt = user_text(e)
            if txt is not None and command_name(txt) == '/clear': ctx += 1; anomalies['/clear commands (context boundaries inside a file)'] += 1
            if txt is None: seg.events.append((ts(e['timestamp']), 'tool_result'))
            else:
                if okind(e) == 'peer': seg.peer_msgs += 1
                if okind(e) in ('peer', 'task-notification'): seg.events.append((ts(e['timestamp']), 'injected'))
                # local commands, meta lines, interrupts: not timing events
        elif t == 'assistant':
            m = e['message']
            has_tool = any(b.get('type') == 'tool_use' for b in m.get('content') or [])
            seg.events.append((ts(e['timestamp']), 'assistant_tool' if has_tool else 'assistant'))
            add_assistant(seg, e, 'main')
        elif t == 'api-request':
            seg.events.append((ts(e['timestamp']), 'api_request'))
        elif t == 'system' and e.get('subtype') == 'turn_duration':
            seg.turn_ms += e.get('durationMs') or 0
        elif t == 'attachment':
            a = e.get('attachment') or {}
            if a.get('type') == 'queued_command':
                o = a.get('origin'); o = o.get('kind') if isinstance(o, dict) else o
                if o == 'human': seg.queued_human += 1
    # sidechains
    n_agent_files = 0
    for sf in sorted(glob.glob(D + '/' + sid + '/**/*.jsonl', recursive=True)):
        if not os.path.basename(sf).startswith('agent-'): continue
        n_agent_files += 1
        seg = None; now_seg = None; held = []
        for line in open(sf):
            e = json.loads(line); t = e.get('type')
            if t == 'user' and e.get('promptId'):
                s2 = pid2seg.get(e['promptId'])
                if s2 is None: anomalies['sidechain promptId not found in parent'] += 1
                else:
                    now_seg = s2
                    # an agent invocation (launch or continuation) starts at a non-meta text user line;
                    # everything until the next one belongs to the segment current at that moment
                    if seg is None or (user_text(e) is not None and not e.get('isMeta')):
                        if seg is not None and s2 is not seg: anomalies['agent continued from a later segment'] += 1
                        seg = s2
                for h in held: seg.events.append(h) if seg else None
                held = []
            if 'timestamp' not in e: continue
            if t == 'user':
                ev = (ts(e['timestamp']), 'tool_result' if user_text(e) is None else 'injected')
            elif t == 'assistant':
                has_tool = any(b.get('type') == 'tool_use' for b in e['message'].get('content') or [])
                ev = (ts(e['timestamp']), 'assistant_tool' if has_tool else 'assistant')
            elif t == 'api-request': ev = (ts(e['timestamp']), 'api_request')
            else: continue
            if seg is None: held.append(ev); continue
            seg.events.append(ev)
            if t == 'assistant':
                if now_seg is not seg and e['message'].get('id') not in mid_owner: anomalies['sidechain calls made after a later human prompt, kept with the launching segment'] += 1
                add_assistant(seg, e, 'side')
    if segs and sid == CURRENT:
        segs[-1].in_progress = True
        for x in segs:
            if x.events and max(t for t, _ in x.events) > NOW - 900: x.in_progress = True
    allsegs = ([pre] if (pre.calls or pre.events) else []) + segs
    segments.extend(allsegs)
    sessions.append(dict(sid=sid, segs=allsegs, cost_states=cost_states, n_agent_files=n_agent_files))

GAPLOG = []
def timing(events):
    """wall seconds, active seconds, dropped seconds by category of the event before the gap"""
    ev = sorted(events)
    if len(ev) < 2: return 0.0, 0.0, {}
    active = 0.0; dropped = collections.Counter()
    for (a, ca), (b, _) in zip(ev, ev[1:]):
        g = b - a
        if g <= GAP: active += g
        else: dropped[ca] += g; GAPLOG.append((round(g / 60, 1), ca))
    a15 = sum(b - a for (a, _), (b, _) in zip(ev, ev[1:]) if b - a <= 900)
    dropped['__a15'] = a15
    return ev[-1][0] - ev[0][0], active, dropped

def summarize(seg):
    wall, active, dropped = timing(seg.events)
    tok = collections.defaultdict(lambda: collections.Counter()); usd = 0.0; unpriced = 0; ws = 0
    nm = ns = 0; bymodel = collections.Counter(); usd_side = 0.0
    for mid, (chain, model, u) in seg.calls.items():
        c, t, w = call_cost(model, u)
        tok[model].update(t); ws += w; bymodel[model] += 1
        if chain == 'main': nm += 1
        else: ns += 1
        if c is None: unpriced += 1
        else:
            usd += c
            if chain == 'side': usd_side += c
    tm = sum(1 for c in seg.tools.values() if c == 'main'); tsd = len(seg.tools) - tm
    tot = collections.Counter()
    for m in tok: tot.update(tok[m])
    return dict(session=seg.session, idx=seg.idx, ctx=seg.ctx, kind=seg.kind, in_progress=seg.in_progress,
                calls=nm + ns, calls_main=nm, calls_side=ns, tools=tm + tsd, tools_main=tm, tools_side=tsd,
                wall_min=wall / 60, active_min=active / 60,
                drop_inflight_model_min=dropped.get('api_request', 0) / 60,
                drop_tool_pending_min=dropped.get('assistant_tool', 0) / 60,
                drop_other_min=sum(v for k, v in dropped.items() if k not in ('api_request', 'assistant_tool', '__a15')) / 60, active15_min=dropped.get('__a15', 0) / 60,
                turn_min=seg.turn_ms / 60000, out=tot['out'], inp_side=tot['inp'] + tot['cr'] + tot['cw5'] + tot['cw1'],
                tok={m: dict(v) for m, v in tok.items()}, usd=usd, usd_side=usd_side, unpriced=unpriced, web_search=ws,
                bymodel=dict(bymodel), peer_msgs=seg.peer_msgs, queued_human=seg.queued_human,
                side_share=(ns / (nm + ns)) if (nm + ns) else None)

def pct(vals, q):
    v = sorted(vals); n = len(v)
    if n == 0: return float('nan')
    k = (n - 1) * q; lo = math.floor(k); hi = math.ceil(k)
    return v[lo] + (v[hi] - v[lo]) * (k - lo)

QS = [('min', 0), ('p50', .5), ('p75', .75), ('p90', .9), ('p95', .95), ('max', 1)]
def fmt(x, d=0):
    return f'{x:,.{d}f}'
def table(title, rows, metrics):
    print(f'\n### {title}  (n={len(rows)})')
    if not rows: return
    print('| metric | ' + ' | '.join(n for n, _ in QS) + ' | sum |'); print('|---|' + '---:|' * (len(QS) + 1))
    for label, key, d in metrics:
        vals = [r[key] for r in rows]
        print(f'| {label} | ' + ' | '.join(fmt(pct(vals, q), d) for _, q in QS) + f' | {fmt(sum(vals), d)} |')

METRICS = [('model calls (all)', 'calls', 0), ('model calls, main chain', 'calls_main', 0), ('model calls, sidechain', 'calls_side', 0),
           ('tool calls (all)', 'tools', 0), ('tool calls, main', 'tools_main', 0), ('tool calls, sidechain', 'tools_side', 0),
           ('wall clock min', 'wall_min', 1), ('active min (gaps >5m dropped)', 'active_min', 1),
           ('active min, 15-min gap rule (sensitivity)', 'active_plus', 1), ('turn_duration sum, min (main chain, harness-reported)', 'turn_min', 1),
           ('output tokens', 'out', 0), ('input-side tokens', 'inp_side', 0), ('USD at list price', 'usd', 2)]

S = [summarize(s) for s in segments]
SEG_GAPS = list(GAPLOG)
for r in S: r['active_plus'] = r['active15_min']
real = [r for r in S if r['idx'] >= 0 and not r['in_progress']]
pre = [r for r in S if r['idx'] < 0]; inprog = [r for r in S if r['in_progress']]
print('transcript files: top-level', len(top), '; agent files', sum(s['n_agent_files'] for s in sessions))
print('anomalies', dict(anomalies))
print('segments (human prompts):', len(real) + len(inprog), '; excluded as in progress:', len(inprog), [(r['calls'], round(r['usd'], 2)) for r in inprog])
print('pre-first-prompt buckets:', len(pre), 'calls', sum(r['calls'] for r in pre), 'usd', round(sum(r['usd'] for r in pre), 2))
print('segments with 0 model calls:', sum(1 for r in real if r['calls'] == 0), '; 1-9 calls:', sum(1 for r in real if 0 < r['calls'] < 10), '; >=10:', sum(1 for r in real if r['calls'] >= 10))
print('peer-origin messages inside segments:', sum(r['peer_msgs'] for r in S), 'in', sum(1 for r in S if r['peer_msgs']), 'segments; human prompts queued mid-turn (no new segment):', sum(r['queued_human'] for r in S))
kinds = collections.Counter(r['kind'] for r in real)
print('segment kinds by command name:', dict(kinds))

big = [r for r in real if r['calls'] >= 10]
table('Segments with >= 10 model calls', big, METRICS)
table('All segments (every human prompt)', real, METRICS)
table('All segments with >= 1 model call', [r for r in real if r['calls'] >= 1], METRICS)
impl = [r for r in real if r['kind'].endswith(':implement') or r['kind'] == '/implement']
table('Segments started by an implement command', impl, METRICS)
table('Implement segments with >= 10 model calls', [r for r in impl if r['calls'] >= 10], METRICS)
table('>=10-call segments NOT started by implement', [r for r in big if r not in impl], METRICS)

# contexts: stretch between /clear commands (or file start/end) = one conversation context
ctxs = collections.OrderedDict()
for r in S:
    if r['idx'] < 0: continue
    ctxs.setdefault((r['session'], r['ctx']), []).append(r)
CT = []
for key, rows in ctxs.items():
    c = dict(first_kind=rows[0]['kind'], in_progress=any(x['in_progress'] for x in rows), prompts=len(rows))
    for k in ('calls', 'calls_main', 'calls_side', 'tools', 'tools_main', 'tools_side', 'wall_min', 'active_min', 'active_plus', 'turn_min', 'out', 'inp_side', 'usd'):
        c[k] = sum(x[k] for x in rows)
    CT.append(c)
CT = [c for c in CT if not c['in_progress']]
CM = [m for m in METRICS if m[1] not in ('wall_min', 'turn_min')] + [('human prompts in the context', 'prompts', 0)]
table('Contexts (between /clear or file boundaries), all; times are sums over their segments', CT, CM)
table('Contexts whose first prompt is an implement command (the command plus every follow-up prompt until /clear or end)', [c for c in CT if c['first_kind'].endswith(':implement')], CM)
table('Contexts with >= 10 model calls', [c for c in CT if c['calls'] >= 10], CM)
ci = [c for c in CT if c['first_kind'].endswith(':implement')]
print('implement contexts: >150 calls', sum(1 for c in ci if c['calls'] > 150), '; >100 calls', sum(1 for c in ci if c['calls'] > 100), '; main >100', sum(1 for c in ci if c['calls_main'] > 100), '; >60 active min', sum(1 for c in ci if c['active_min'] > 60), '; >$25', sum(1 for c in ci if c['usd'] > 25), '; detail (prompts, calls, main, active, usd):', sorted((c['prompts'], c['calls'], c['calls_main'], round(c['active_min']), round(c['usd'], 2)) for c in ci))
print('all contexts: >150 calls', sum(1 for c in CT if c['calls'] > 150), '; >60 active min', sum(1 for c in CT if c['active_min'] > 60), '; >$25', sum(1 for c in CT if c['usd'] > 25), '; >$50', sum(1 for c in CT if c['usd'] > 50))
print('\n### thresholds')
for name, rows in (('>=10 calls', big), ('all', real), ('implement', impl)):
    print(f'{name}: n={len(rows)}; >150 model calls: {sum(1 for r in rows if r["calls"] > 150)}; >100: {sum(1 for r in rows if r["calls"] > 100)}; >60 active min: {sum(1 for r in rows if r["active_min"] > 60)}; >60 active min (15-min rule): {sum(1 for r in rows if r["active_plus"] > 60)}; >30 active min: {sum(1 for r in rows if r["active_min"] > 30)}; >$10: {sum(1 for r in rows if r["usd"] > 10)}; >$25: {sum(1 for r in rows if r["usd"] > 25)}; >$50: {sum(1 for r in rows if r["usd"] > 50)}')
    print(f'   main-chain calls >150: {sum(1 for r in rows if r["calls_main"] > 150)}; main-chain calls >100: {sum(1 for r in rows if r["calls_main"] > 100)}; main >60: {sum(1 for r in rows if r["calls_main"] > 60)}')

print('\n### sidechain share of model calls per segment')
for name, rows in (('>=10 calls', big), ('>=1 call', [r for r in real if r['calls'] >= 1]), ('implement', [r for r in impl if r['calls'] >= 1])):
    v = [r['side_share'] for r in rows]
    print(f'{name}: n={len(v)} p50={pct(v, .5):.3f} p75={pct(v, .75):.3f} p90={pct(v, .9):.3f} max={pct(v, 1):.3f}; segments with any sidechain call: {sum(1 for x in v if x > 0)}; share of USD from sidechains (pooled): {sum(r["usd_side"] for r in rows) / max(1e-9, sum(r["usd"] for r in rows)):.3f}')

print('\n### dropped long gaps (>5 min), minutes, all segments, by what preceded the gap')
print('after api-request (model call in flight):', fmt(sum(r['drop_inflight_model_min'] for r in real), 1), '| after assistant tool_use (tool running or permission wait):', fmt(sum(r['drop_tool_pending_min'] for r in real), 1), '| other (idle / waiting on human or background agent):', fmt(sum(r['drop_other_min'] for r in real), 1))

print('dropped gaps (min, preceding event), segments only:', sorted(SEG_GAPS, reverse=True))
# models
print('\n### models (deduplicated calls, everything incl. in-progress and pre-prompt)')
mc = collections.Counter(); mt = collections.defaultdict(collections.Counter); musd = collections.Counter(); mchain = collections.Counter()
for seg in segments:
    for mid, (chain, model, u) in seg.calls.items():
        c, t, w = call_cost(model, u); mc[model] += 1; mt[model].update(t); mchain[(model, chain)] += 1
        if c is not None: musd[model] += c
tot = sum(mc.values())
print('| model | calls | share | main | side | input | output | cache read | cache write 5m | cache write 1h | USD |'); print('|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|')
for m, n in mc.most_common():
    t = mt[m]; print(f'| {m} | {n} | {n / tot:.1%} | {mchain[(m, "main")]} | {mchain[(m, "side")]} | {t["inp"]:,} | {t["out"]:,} | {t["cr"]:,} | {t["cw5"]:,} | {t["cw1"]:,} | {musd[m]:,.2f} |')
print('total calls', tot, 'grand total USD', round(sum(musd.values()), 2), 'web searches', sum(r['web_search'] for r in S), 'unpriced calls', sum(r['unpriced'] for r in S))
print('grand total USD in measured segments (excl. in-progress and pre-prompt):', round(sum(r['usd'] for r in real), 2))
geo = collections.Counter(); 
for seg in segments:
    for mid, (chain, model, u) in seg.calls.items(): geo[(u.get('inference_geo'), u.get('service_tier'), u.get('speed'))] += 1
print('inference_geo/service_tier/speed:', dict(geo))

# sessions
print('\n### sessions (one transcript file incl. its subagent files)')
SESS = []
for s in sessions:
    rows = [summarize(x) for x in s['segs']]
    ev = [e for x in s['segs'] for e in x.events]
    wall, active, dropped = timing(ev)
    r = dict(sid=s['sid'], calls=sum(x['calls'] for x in rows), calls_main=sum(x['calls_main'] for x in rows), calls_side=sum(x['calls_side'] for x in rows),
             tools=sum(x['tools'] for x in rows), tools_main=sum(x['tools_main'] for x in rows), tools_side=sum(x['tools_side'] for x in rows),
             wall_min=wall / 60, active_min=active / 60, active_plus=dropped.get('__a15', 0) / 60,
             turn_min=sum(x['turn_min'] for x in rows), out=sum(x['out'] for x in rows), inp_side=sum(x['inp_side'] for x in rows), usd=sum(x['usd'] for x in rows),
             prompts=sum(1 for x in rows if x['idx'] >= 0), tok=collections.Counter())
    # transcript cost field
    cs = s['cost_states']; r['cost_state_lines'] = len(cs)
    if cs:
        vals = [c.get('totalCostUSD') or 0 for c in cs]; total = 0.0; prev = 0.0
        for v in vals:
            if v < prev: total += prev
            prev = v
        r['cost_field'] = total + prev; r['cost_resets'] = sum(1 for a, b in zip(vals, vals[1:]) if b < a)
        r['cost_unknown'] = any(c.get('hasUnknownModelCost') for c in cs)
    SESS.append(r)
DONE = [r for r in SESS if r['sid'] != CURRENT]
table('Sessions, finished (current session excluded)', DONE, METRICS + [('human prompts per session', 'prompts', 0)])
print('finished sessions: >150 calls', sum(1 for r in DONE if r['calls'] > 150), '; >100 calls', sum(1 for r in DONE if r['calls'] > 100), '; >60 active min', sum(1 for r in DONE if r['active_min'] > 60), '; >$25', sum(1 for r in DONE if r['usd'] > 25), '; >$50', sum(1 for r in DONE if r['usd'] > 50))
print('current (in-progress) session included above:', CURRENT[:8])
print('\n### transcript cost field (cost-state.totalCostUSD) vs computed, sessions that carry it')
print('| session | cost-state lines | resets | cost field USD | computed USD | ratio computed/field |'); print('|---|---:|---:|---:|---:|---:|')
cf = [r for r in SESS if r.get('cost_state_lines')]
for r in cf: print(f'| {r["sid"][:8]} | {r["cost_state_lines"]} | {r["cost_resets"]} | {r["cost_field"]:.2f} | {r["usd"]:.2f} | {r["usd"] / r["cost_field"] if r["cost_field"] else float("nan"):.3f} |')
print('sessions with cost field:', len(cf), 'of', len(SESS), '; sum field', round(sum(r['cost_field'] for r in cf), 2), '; sum computed (same sessions)', round(sum(r['usd'] for r in cf), 2))

if OUT: json.dump(dict(segments=S, sessions=[{k: v for k, v in r.items() if k != 'tok'} for r in SESS]), open(OUT, 'w'))
