#!/usr/bin/env python3
"""PROTOTYPE: what a session limit meets on the stream of a headless Claude Code session.

Throwaway code for issue #47 and probe P-26 (SPEC 14, BUD-6 and BUD-10). See README.md.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOKEN_FILE = Path.home() / ".config" / "sf-probe" / "oauth-token"
CHILD_PATH = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

# The chain: each file names the next, so the reads cannot be made in one response.
CHAIN = ["start", "amber", "birch", "cedar", "dune", "ember", "fjord", "glade", "heath", "inlet", "knoll", "ledge", "marsh", "notch"]
FINAL_WORD = "HERON"
# A second chain, for two subagents at once.
CHAIN_B = ["first", "gorse", "hazel", "iris", "juniper", "kelp", "larch", "moss", "nettle", "oak", "pine", "quince", "rowan", "sedge"]
FINAL_WORD_B = "OSPREY"

SCHEMA = {
    "type": "object",
    "properties": {"answer": {"type": "string"}},
    "required": ["answer"],
    "additionalProperties": False,
}

CHAIN_TASK = (
    "Read the file chain/start.txt with the Read tool. Each file of the chain names the next "
    "file. Follow the chain with one Read call per response, never two in one response, until "
    "a file gives the final word."
)
CHAIN_TASK_B = CHAIN_TASK.replace("chain/start.txt", "chain/first.txt")

PROMPTS = {
    "read": "Use the Read tool to read README.md in the current directory. Answer with its first line.",
    "chain": CHAIN_TASK + " Answer with the final word.",
    "sub-chain": (
        "Call the Agent tool exactly once, with subagent_type \"general-purpose\", and give the "
        "subagent this task word for word: '" + CHAIN_TASK + " Report the final word.' "
        "Do not read any file yourself. Answer with the word the subagent reports."
    ),
    "sub-chain-foreground": (
        "Call the Agent tool exactly once, with subagent_type \"general-purpose\" and "
        "run_in_background false, and give the subagent this task word for word: '" + CHAIN_TASK + " "
        "Report the final word.' Do not read any file yourself. Answer with the word the subagent reports."
    ),
    "sub-chain-background": (
        "Call the Agent tool exactly once, with subagent_type \"general-purpose\" and "
        "run_in_background true, and give the subagent this task word for word: '" + CHAIN_TASK + " "
        "Report the final word.' Do not read any file yourself. Answer with the word the subagent reports."
    ),
    "sub-chain-and-own-chain": (
        "First call the Agent tool exactly once, with subagent_type \"general-purpose\" and "
        "run_in_background true, and give the subagent this task word for word: '" + CHAIN_TASK_B + " "
        "Report the final word.' Then do this yourself, without waiting for the subagent: " + CHAIN_TASK + " "
        "Answer with the final word of your own chain."
    ),
    "bash-background": (
        "Call the Bash tool exactly once, with the command `sleep 20; echo finished` and "
        "run_in_background true. Do not wait for the command and do not call any other tool. "
        "Answer with the word STARTED."
    ),
    "sub-chain-sonnet": (
        "Call the Agent tool exactly once, with subagent_type \"general-purpose\" and model "
        "\"sonnet\", and give the subagent this task word for word: '" + CHAIN_TASK + " Report "
        "the final word.' Do not read any file yourself. Answer with the word the subagent reports."
    ),
    "two-sub-chains": (
        "Call the Agent tool twice in one response, so that the two subagents run at the same "
        "time, both with subagent_type \"general-purpose\". Give the first this task word for "
        "word: '" + CHAIN_TASK + " Report the final word.' Give the second this task word for "
        "word: '" + CHAIN_TASK_B + " Report the final word.' Do not read any file yourself. "
        "Answer with the two words the subagents report, separated by a space."
    ),
}

MAIN_TOOLS = "Read,Glob,Grep"
BASH_TOOLS = "Bash,Read"
SUB_TOOLS = "Read,Glob,Grep,Agent"


def case(name, about, messages, *, max_turns, budget="1", tools=MAIN_TOOLS, partial=True, extra=(), env=None, wait=None, timeout=300):
    return {
        "name": name,
        "about": about,
        "messages": messages,
        "max_turns": max_turns,
        "budget": budget,
        "tools": tools,
        "partial": partial,
        "extra": list(extra),
        "env": env or {},
        # Wait for the background tasks the stream lists before the next message or the end
        # of input. By default: wherever the session has the Agent tool.
        "wait_for_tasks": (tools in (SUB_TOOLS, BASH_TOOLS)) if wait is None else wait,
        "timeout": timeout,
    }


FORWARD = ["--forward-subagent-text"]

# The budgets were set after a first pass, from the costs that the cases before them
# reported. They are dollars as the CLI counts them, on the haiku alias.
CASES = [
    # Part 1: more than one message on a stream-JSON input.
    case(
        "turns-per-message",
        "--max-turns 5; three messages that fit, one that does not, one more after the cap",
        ["read", "read", "read", "chain", "read"],
        max_turns=5,
    ),
    case(
        "turns-cap-last",
        "--max-turns 3; one message that does not fit, then the end of input",
        ["chain"],
        max_turns=3,
    ),
    case(
        "budget-across-messages",
        "--max-budget-usd above the first message's cost and below the session's; four messages",
        ["read", "chain", "chain", "read"],
        max_turns=30,
        budget="0.0019",
    ),
    # Part 2: a subagent.
    case(
        "subagent",
        "one subagent that makes fifteen model calls; the whole line of AG-1",
        ["sub-chain"],
        max_turns=30,
        tools=SUB_TOOLS,
    ),
    case(
        "subagent-input-closed",
        "the same, with standard input closed 3 s after the first result",
        ["sub-chain"],
        max_turns=30,
        tools=SUB_TOOLS,
        wait=False,
    ),
    case(
        "subagent-forward-text",
        "the same as subagent with --forward-subagent-text",
        ["sub-chain"],
        max_turns=30,
        tools=SUB_TOOLS,
        extra=FORWARD,
    ),
    case(
        "subagent-no-partial",
        "the same as subagent without --include-partial-messages",
        ["sub-chain"],
        max_turns=30,
        tools=SUB_TOOLS,
        partial=False,
    ),
    case(
        "subagent-foreground",
        "the model asks for run_in_background false",
        ["sub-chain-foreground"],
        max_turns=30,
        tools=SUB_TOOLS,
    ),
    case(
        "subagent-foreground-forward-text",
        "the same with --forward-subagent-text",
        ["sub-chain-foreground"],
        max_turns=30,
        tools=SUB_TOOLS,
        extra=FORWARD,
    ),
    case(
        "subagent-background-off",
        "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1, and a model that does not ask for the foreground",
        ["sub-chain"],
        max_turns=30,
        tools=SUB_TOOLS,
        env={"CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1"},
    ),
    case(
        "subagent-background-off-asked",
        "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1, and a message that asks for run_in_background true",
        ["sub-chain-background"],
        max_turns=30,
        tools=SUB_TOOLS,
        env={"CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1"},
    ),
    case(
        "subagent-sonnet",
        "the subagent on another model than the main loop, so that modelUsage tells them apart",
        ["sub-chain-sonnet"],
        max_turns=30,
        tools=SUB_TOOLS,
    ),
    case(
        "two-subagents",
        "two subagents at once, no cap reached",
        ["two-sub-chains"],
        max_turns=30,
        tools=SUB_TOOLS,
    ),
    case(
        "subagent-main-loop-max-turns",
        "--max-turns 4, reached by the main loop while a background subagent runs",
        ["sub-chain-and-own-chain"],
        max_turns=4,
        tools=SUB_TOOLS,
    ),
    # Beside the question: the variable of the cases above and a background shell command.
    case(
        "bash-background",
        "a Bash command that the message asks to run in the background",
        ["bash-background"],
        max_turns=10,
        tools=BASH_TOOLS,
    ),
    case(
        "bash-background-off",
        "the same with CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1",
        ["bash-background"],
        max_turns=10,
        tools=BASH_TOOLS,
        env={"CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1"},
    ),
    # Part 2, the caps.
    case(
        "subagent-max-turns",
        "--max-turns 6, below the subagent's fifteen calls and above the main loop's",
        ["sub-chain"],
        max_turns=6,
        tools=SUB_TOOLS,
    ),
    case(
        "subagent-foreground-max-turns",
        "the same with the subagent in the foreground",
        ["sub-chain-foreground"],
        max_turns=6,
        tools=SUB_TOOLS,
    ),
    case(
        "subagent-max-budget",
        "--max-budget-usd reached while the subagent runs and the main loop has returned",
        ["sub-chain"],
        max_turns=30,
        budget="0.0025",
        tools=SUB_TOOLS,
    ),
    case(
        "subagent-max-budget-then-message",
        "the same, and a second message after the cap",
        ["sub-chain", "read"],
        max_turns=30,
        budget="0.0025",
        tools=SUB_TOOLS,
    ),
    case(
        "subagent-foreground-max-budget",
        "--max-budget-usd reached while a foreground subagent runs",
        ["sub-chain-foreground"],
        max_turns=30,
        budget="0.0020",
        tools=SUB_TOOLS,
    ),
    case(
        "subagent-background-off-max-budget",
        "the same with CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1 and no request for the foreground",
        ["sub-chain"],
        max_turns=30,
        budget="0.0030",
        tools=SUB_TOOLS,
        env={"CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1"},
    ),
    # Part 2, as SPEC AG-1 and AG-11 stand after this probe: both switches together.
    case(
        "subagent-as-specified",
        "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1 and --forward-subagent-text",
        ["sub-chain"],
        max_turns=30,
        tools=SUB_TOOLS,
        extra=FORWARD,
        env={"CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1"},
    ),
    case(
        "subagent-as-specified-max-budget",
        "the same, with --max-budget-usd reached while the subagent runs",
        ["sub-chain", "read"],
        max_turns=30,
        budget="0.0020",
        tools=SUB_TOOLS,
        extra=FORWARD,
        env={"CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1"},
    ),
    case(
        "two-subagents-max-budget",
        "two subagents at once, with the cap reached while both run",
        ["two-sub-chains"],
        max_turns=30,
        budget="0.0035",
        tools=SUB_TOOLS,
    ),
]


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def plant_repository(work: Path) -> Path:
    write(work / "README.md", "Probe fixture for P-26\n\nNothing else is here.\n")
    for names, word in ((CHAIN, FINAL_WORD), (CHAIN_B, FINAL_WORD_B)):
        for here, after in zip(names, names[1:]):
            write(work / "chain" / f"{here}.txt", f"This is not the last file. The next file is chain/{after}.txt\n")
        write(work / "chain" / f"{names[-1]}.txt", f"This is the last file. The final word is {word}.\n")
    env = {
        "PATH": CHILD_PATH,
        "HOME": str(work),
        "GIT_CONFIG_GLOBAL": "/dev/null",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_AUTHOR_NAME": "probe",
        "GIT_AUTHOR_EMAIL": "probe@example.invalid",
        "GIT_COMMITTER_NAME": "probe",
        "GIT_COMMITTER_EMAIL": "probe@example.invalid",
    }
    for args in (["init", "-q", "-b", "main"], ["add", "-A"], ["commit", "-q", "-m", "fixture"]):
        subprocess.run(["git", *args], cwd=work, env=env, check=True, capture_output=True)
    return work


def build_command(spec, model: str, session_id: str, host: Path) -> list[str]:
    """The line of AG-1 in `subscription` mode (AG-3a): every flag it names, and no --bare."""
    write(host / "settings.json", json.dumps({"sandbox": {"enabled": False}}, indent=2))
    write(host / "mcp.json", json.dumps({"mcpServers": {}}, indent=2))
    write(host / "append.md", "You are a worker of a probe. Do exactly what the message asks.\n")
    cmd = ["claude", "-p", "--setting-sources", "user", "--output-format", "stream-json", "--verbose"]
    cmd += ["--input-format", "stream-json", "--session-id", session_id, "--model", model]
    cmd += ["--permission-mode", "dontAsk", "--permission-prompts", "none"]
    cmd += ["--tools", spec["tools"], "--allowedTools", spec["tools"]]
    cmd += ["--settings", str(host / "settings.json")]
    # --strict-mcp-config directly after --mcp-config, which takes several values.
    cmd += ["--mcp-config", str(host / "mcp.json"), "--strict-mcp-config"]
    cmd += ["--json-schema", json.dumps(SCHEMA)]
    cmd += ["--max-turns", str(spec["max_turns"]), "--max-budget-usd", spec["budget"]]
    cmd += ["--append-system-prompt-file", str(host / "append.md")]
    if spec["partial"]:
        cmd += ["--include-partial-messages"]
    cmd += ["--debug-file", str(host / "debug.log")]  # the harness's own, not a flag of AG-1
    return cmd + spec["extra"]


def run_session(spec, model: str, token: str, out: Path) -> dict:
    run = out / spec["name"]
    if run.exists():
        shutil.rmtree(run)
    host, home = run / "host", run / "home"
    for d in (host, home / ".claude", run / "tmp"):
        d.mkdir(parents=True)
    cwd = plant_repository(run / "work").resolve()
    env = {
        "PATH": CHILD_PATH,
        "HOME": str(home),
        "CLAUDE_CONFIG_DIR": str(home / ".claude"),
        "CLAUDE_CODE_OAUTH_TOKEN": token,
        "TMPDIR": str(run / "tmp"),
        "LANG": "en_US.UTF-8",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
        **spec["env"],
    }
    cmd = build_command(spec, model, str(uuid.uuid4()), host)
    messages = [PROMPTS[key] for key in spec["messages"]]
    notes = []
    started = time.monotonic()

    def now() -> float:
        return round(time.monotonic() - started, 2)

    def send(proc, index: int) -> bool:
        line = {"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": messages[index]}]}}
        try:
            proc.stdin.write((json.dumps(line) + "\n").encode())
            proc.stdin.flush()
        except (BrokenPipeError, ValueError) as error:
            notes.append({"_t": now(), "note": f"message {index + 1} could not be written: {type(error).__name__}"})
            return False
        notes.append({"_t": now(), "note": f"message {index + 1} sent ({spec['messages'][index]})"})
        return True

    with open(run / "stderr.txt", "wb") as stderr:
        proc = subprocess.Popen(
            cmd, cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, start_new_session=True
        )
        timed_out = threading.Event()

        def kill():
            timed_out.set()
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

        timer = threading.Timer(spec["timeout"], kill)
        timer.start()
        send(proc, 0)
        state = {"results": 0, "tasks": []}

        def after_result(seen: int) -> None:
            """Runs beside the reader: what the harness does once a result has arrived."""
            # Does the process outlive this result? A cap ends a session without
            # stream-JSON input (P-22); with it, that is part of the question.
            time.sleep(3)
            alive = proc.poll() is None
            notes.append({"_t": now(), "note": f"3 s after result {seen}: process {'alive' if alive else 'gone'}"})
            if spec["wait_for_tasks"]:
                # A result is not the end while the stream lists a background task: wait
                # until none has been listed for 5 s, or for a later result to take over.
                deadline, quiet_since = time.monotonic() + 60, None
                while time.monotonic() < deadline and proc.poll() is None:
                    if state["results"] != seen:
                        return
                    if state["tasks"]:
                        quiet_since = None
                    else:
                        quiet_since = quiet_since or time.monotonic()
                        if time.monotonic() - quiet_since >= 5:
                            break
                    time.sleep(0.2)
                else:
                    if proc.poll() is None:
                        notes.append({"_t": now(), "note": f"background tasks still listed 60 s after result {seen}"})
            if state["results"] != seen:
                return
            next_index = state["sent"]
            if next_index < len(messages):
                state["sent"] += 1
                send(proc, next_index)
            else:
                try:
                    proc.stdin.close()
                except BrokenPipeError:
                    pass
                notes.append({"_t": now(), "note": "standard input closed"})

        state["sent"] = 1
        events = []
        with open(run / "events.jsonl", "wb") as raw:
            for line in proc.stdout:
                raw.write(line)  # the raw stream is kept before it is parsed (AG-6)
                raw.flush()
                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    event = {"type": "unparsed", "text": line.decode("utf-8", "replace")[:500]}
                event["_t"] = now()
                events.append(event)
                if event.get("type") == "system" and event.get("subtype") == "background_tasks_changed":
                    state["tasks"] = event.get("tasks", [])
                if event.get("type") == "result":
                    state["results"] += 1
                    threading.Thread(target=after_result, args=(state["results"],), daemon=True).start()
        exit_code = proc.wait()
        ended = now()
        timer.cancel()

    needle = token.encode()
    token_hits = [
        str(p.relative_to(run)) for p in run.rglob("*") if p.is_file() and not p.is_symlink() and needle in p.read_bytes()
    ]
    summary = summarise(spec, cmd, events, notes)
    summary.update(
        {
            "exit_code": exit_code,
            "timed_out": timed_out.is_set(),
            "wall_seconds": ended,
            "files_holding_the_token": token_hits,
            "stderr_tail": (run / "stderr.txt").read_text("utf-8", "replace")[-1500:],
        }
    )
    return json.loads(scrub(json.dumps(summary), out))


def scrub(text: str, out: Path) -> str:
    """Replace the scratch directory, and the name the CLI derives from it, by <out>."""
    for path in (str(out.resolve()), str(out)):
        text = text.replace(path, "<out>").replace(re.sub(r"[^A-Za-z0-9]", "-", path), "-out")
    return text


def tokens(usage: dict | None) -> dict | None:
    if not usage:
        return None
    return {
        "in": usage.get("input_tokens"),
        "cache_write": usage.get("cache_creation_input_tokens"),
        "cache_read": usage.get("cache_read_input_tokens"),
        "out": usage.get("output_tokens"),
    }


def short(tool_use_id: str | None) -> str | None:
    return None if tool_use_id is None else tool_use_id[-6:]


RESULT_KEYS = (
    "subtype", "is_error", "terminal_reason", "errors", "num_turns", "total_cost_usd", "usage", "modelUsage",
    "model_usage_since_last_result", "subagent_stats", "structured_output", "stop_reason", "queued_turn_count",
    "result_index", "permission_denials", "api_error_status", "result",
)  # fmt: skip


def summarise(spec, cmd, events, notes) -> dict:
    """One row per line of the stream that says something about calls, turns or spend."""
    rows = []
    calls = {}  # message id -> what the stream said about the call
    partial_by_parent = {}
    init = {}
    other = {}
    results = []
    for event in events:
        kind, parent = event.get("type"), event.get("parent_tool_use_id")
        row = {"_t": event["_t"], "type": kind}
        if "parent_tool_use_id" in event:
            row["parent"] = short(parent)
        if kind == "system" and event.get("subtype") == "init":
            if not init:
                init = {k: event.get(k) for k in ("claude_code_version", "model", "tools", "agents", "permissionMode", "apiKeySource")}
            row["subtype"] = "init"
        elif kind == "stream_event":
            inner = event["event"]
            name = inner.get("type")
            key = "main" if parent is None else "subagent"
            partial_by_parent.setdefault(key, {})
            partial_by_parent[key][name] = partial_by_parent[key].get(name, 0) + 1
            if name not in ("message_start", "message_delta"):
                continue
            row["event"] = name
            if name == "message_start":
                message = inner["message"]
                row.update({"id": message["id"][-6:], "model": message.get("model"), "usage": tokens(message.get("usage"))})
                calls.setdefault(message["id"], {"parent": short(parent), "model": message.get("model")})
                current = message["id"]
            else:
                row.update({"usage": tokens(inner.get("usage")), "stop_reason": inner.get("delta", {}).get("stop_reason")})
                if parent is None and calls:
                    calls[current]["message_delta"] = tokens(inner.get("usage"))
        elif kind in ("assistant", "user"):
            message = event.get("message", {})
            content = message.get("content")
            blocks = []
            if isinstance(content, list):
                for block in content:
                    label = block.get("type", "?")
                    if label == "tool_use":
                        label += f":{block.get('name')}:{short(block.get('id'))}"
                        if block.get("name") == "Agent":
                            row["agent_input"] = {k: v for k, v in block.get("input", {}).items() if k != "prompt"}
                    elif label == "tool_result":
                        label += f":{short(block.get('tool_use_id'))}"
                    blocks.append(label)
            else:
                blocks.append("text" if isinstance(content, str) else "?")
            row["blocks"] = blocks
            if kind == "assistant":
                row.update({"id": message["id"][-6:], "model": message.get("model"), "usage": tokens(message.get("usage"))})
                call = calls.setdefault(message["id"], {"parent": short(parent), "model": message.get("model")})
                call["parent"] = short(parent)
                call["assistant_lines"] = call.get("assistant_lines", 0) + 1
                call["assistant_usage"] = tokens(message.get("usage"))
                call.setdefault("blocks", []).extend(blocks)
            for extra_key in ("isSynthetic", "subagent_type", "agent_id"):
                if extra_key in event:
                    row[extra_key] = event[extra_key]
            outcome = event.get("tool_use_result")
            if isinstance(outcome, dict) and ("agentId" in outcome or "isAsync" in outcome):
                keep = ("status", "isAsync", "resolvedModel", "totalTokens", "totalToolUseCount", "totalDurationMs", "usage")
                row["agent_result"] = {k: outcome[k] for k in keep if k in outcome}
        elif kind == "result":
            row.update({k: event[k] for k in RESULT_KEYS if k in event})
            results.append({k: event[k] for k in RESULT_KEYS if k in event})
        elif kind == "rate_limit_event":
            row["rate_limit_info"] = event.get("rate_limit_info")
        else:
            row["subtype"] = event.get("subtype")
            label = f"{kind}/{event.get('subtype')}"
            other[label] = other.get(label, 0) + 1
            if kind == "system" and event.get("subtype") in ("status", "thinking_tokens"):
                continue
            row["keys"] = sorted(k for k in event if k not in ("_t", "type", "subtype", "uuid", "session_id"))
            for extra_key in ("status", "is_backgrounded", "spawn_depth", "patch", "usage", "last_tool_name", "description"):
                if extra_key in event:
                    row[extra_key] = event[extra_key]
            if "tool_use_id" in event:
                row["tool_use_id"] = short(event["tool_use_id"])
            if "tasks" in event:
                row["tasks"] = len(event["tasks"])
        rows.append(row)

    rows = sorted(rows + [dict(n, type="_probe") for n in notes], key=lambda r: r["_t"])
    main = [c for c in calls.values() if c["parent"] is None]
    sub = [c for c in calls.values() if c["parent"] is not None]
    return {
        "case": spec["name"],
        "about": spec["about"],
        "command": cmd,
        "messages": [PROMPTS[key] for key in spec["messages"]],
        "init": init,
        "counts": {
            "lines": len(events),
            "main_loop_calls": len(main),
            "main_loop_output_tokens_by_message_delta": sum((c.get("message_delta") or {}).get("out") or 0 for c in main),
            "main_loop_output_tokens_by_assistant_lines": sum((c.get("assistant_usage") or {}).get("out") or 0 for c in main),
            "subagent_calls_shown": len(sub),
            "subagent_output_tokens_by_assistant_lines": sum((c.get("assistant_usage") or {}).get("out") or 0 for c in sub),
            "subagent_models": sorted({c["model"] for c in sub if c.get("model")}),
            "stream_events": partial_by_parent,
            "other_lines": other,
        },
        "results": results,
        "stream": rows,
    }


def one_line(row: dict) -> str:
    head = f"{row['_t']:7.2f}  {row['type']}"
    rest = {k: v for k, v in row.items() if k not in ("_t", "type")}
    if row["type"] == "result":
        rest = {k: rest[k] for k in ("subtype", "terminal_reason", "num_turns", "total_cost_usd", "errors") if k in rest}
    return head + "  " + json.dumps(rest, separators=(",", ":"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("-k", metavar="TEXT", action="append", help="run the cases whose name contains TEXT (repeatable)")
    parser.add_argument("--list", action="store_true", help="list the cases and exit")
    parser.add_argument("--model", default="haiku")
    parser.add_argument("--out", type=Path, help="scratch directory for the runs (kept, never committed)")
    parser.add_argument("--results", type=Path, default=HERE / "results")
    parser.add_argument("--budget", help="override --max-budget-usd of the selected cases (to calibrate)")
    args = parser.parse_args()

    selected = [c for c in CASES if not args.k or any(k == c["name"] for k in args.k)]
    if args.list:
        for spec in selected:
            print(f"{spec['name']:34} {spec['about']}")
        return 0
    if args.out is None:
        parser.error("--out is required to run sessions")
    token = TOKEN_FILE.read_text().strip()
    args.out.mkdir(parents=True, exist_ok=True)
    args.results.mkdir(parents=True, exist_ok=True)

    for spec in selected:
        if args.budget:
            spec = dict(spec, budget=args.budget)
        print(f"== {spec['name']}: {spec['about']}", flush=True)
        summary = run_session(spec, args.model, token, args.out)
        if summary["files_holding_the_token"]:
            print(f"   TOKEN LITERAL FOUND IN: {summary['files_holding_the_token']}; nothing written", flush=True)
            return 1
        text = json.dumps(summary, indent=2)
        (args.results / f"{spec['name']}.json").write_text(text + "\n", encoding="utf-8")
        (args.results / f"{spec['name']}.stream.txt").write_text(
            "\n".join(one_line(row) for row in summary["stream"]) + "\n", encoding="utf-8"
        )
        raw = (args.out / spec["name"] / "events.jsonl").read_text("utf-8")
        (args.results / f"{spec['name']}.events.jsonl").write_text(scrub(raw, args.out), encoding="utf-8")
        print(f"   exit {summary['exit_code']}, {summary['wall_seconds']} s, timed out: {summary['timed_out']}", flush=True)
        print(f"   counts: {json.dumps({k: v for k, v in summary['counts'].items() if k != 'other_lines'})}", flush=True)
        for result in summary["results"]:
            keep = ("subtype", "terminal_reason", "num_turns", "total_cost_usd", "errors", "structured_output")
            print(f"   result: {json.dumps({k: result.get(k) for k in keep})}", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
