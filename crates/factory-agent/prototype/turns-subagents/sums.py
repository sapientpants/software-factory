#!/usr/bin/env python3
"""PROTOTYPE: per session, what the stream's lines add up to against the totals of `result`.

Reads the raw streams that turns_probe.py kept. Usage: python3 sums.py results/pass-1 [...]
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

SIDES = (
    ("input", "input_tokens", "inputTokens"),
    ("cache read", "cache_read_input_tokens", "cacheReadInputTokens"),
    ("cache write", "cache_creation_input_tokens", "cacheCreationInputTokens"),
)


def session(path: Path) -> str:
    events = [json.loads(line) for line in path.read_text("utf-8").splitlines()]
    summary = json.loads(path.with_name(path.name.replace(".events.jsonl", ".json")).read_text("utf-8"))
    calls, delta, current = {}, {}, None  # message id -> (parent, model, usage of the assistant line)
    tasks, results = [], []
    for event in events:
        kind = event.get("type")
        if kind == "stream_event":
            inner = event["event"]
            if inner["type"] == "message_start":
                current = inner["message"]["id"]
                calls.setdefault(current, (event.get("parent_tool_use_id"), inner["message"]["model"], {}))
            elif inner["type"] == "message_delta":
                delta[current] = inner["usage"]
        elif kind == "assistant":
            message = event["message"]
            calls[message["id"]] = (event.get("parent_tool_use_id"), message["model"], message["usage"])
        elif kind == "system" and event.get("subtype") == "task_started":
            tasks.append(f"started {'background' if event['is_backgrounded'] else 'foreground'}")
        elif kind == "system" and event.get("subtype") == "task_updated":
            tasks.append(f"updated {event['patch'].get('status')}")
        elif kind == "system" and event.get("subtype") == "task_notification":
            usage = event.get("usage")
            tasks.append(f"notification {event['status']}" + (f" tool_uses={usage['tool_uses']}" if usage else ""))
        elif kind == "result":
            results.append(event)

    main = [(model, usage) for parent, model, usage in calls.values() if parent is None]
    sub = [(model, usage) for parent, model, usage in calls.values() if parent is not None]
    out = [f"{path.name.replace('.events.jsonl', '')}: exit {summary['exit_code']}"]
    for result in results:
        stats = result.get("subagent_stats", {})
        out.append(
            f"  result {result.get('result_index')}: {result['subtype']}, num_turns {result['num_turns']}, "
            f"total_cost_usd {result['total_cost_usd']:.6f}, subagents completed {stats.get('completed')} "
            f"killed {stats.get('killed')}"
        )
    out.append(f"  model calls shown: main loop {len(main)}, subagents {len(sub)}; tasks: {', '.join(tasks) or 'none'}")
    if not results:
        return "\n".join(out)
    usage_by_model = results[-1]["modelUsage"]
    for label, line_key, total_key in SIDES:
        a = sum(usage.get(line_key, 0) for _, usage in main)
        b = sum(usage.get(line_key, 0) for _, usage in sub)
        total = sum(entry[total_key] for entry in usage_by_model.values())
        out.append(f"  {label:11}: main loop {a} + subagent lines {b} = {a + b}; modelUsage of the last result {total}")
    main_out = sum(d["output_tokens"] for message_id, d in delta.items() if calls[message_id][0] is None)
    sub_placeholder = sum(usage.get("output_tokens", 0) for _, usage in sub)
    total_out = sum(entry["outputTokens"] for entry in usage_by_model.values())
    out.append(
        f"  output     : main loop by message_delta {main_out}; subagent lines {sub_placeholder}; "
        f"modelUsage {total_out}; remainder {total_out - main_out}"
    )
    per_model = {model: (entry["outputTokens"], round(entry["costUSD"], 6)) for model, entry in usage_by_model.items()}
    out.append(f"  modelUsage (output tokens, dollars): {per_model}; subagent models {sorted({m for m, _ in sub})}")
    return "\n".join(out)


def main() -> int:
    for directory in sys.argv[1:]:
        print(f"## {directory}\n")
        for path in sorted(Path(directory).glob("*.events.jsonl")):
            print(session(path) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
