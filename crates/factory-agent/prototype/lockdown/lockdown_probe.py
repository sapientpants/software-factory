#!/usr/bin/env python3
"""PROTOTYPE: live lock-down and subscription probe of the Claude Code CLI.

Throwaway code for issue #11 and probes P-21 and P-22 (SPEC 10.3 AG-3a,
13.4 SEC-11, 14). Each case plants repository-controlled configuration in a
temporary working directory, starts one real `claude -p` session without
`--bare` on a `claude setup-token` credential, and records which of the
planted items took effect, the stream-JSON, and the hosts the CLI asked for.

Evidence, in order of strength:
  1. canary files written by planted hooks and planted MCP servers;
  2. the lists of the `system/init` message (skills, commands, agents, ...);
  3. the model's answer to "list every CANARY- word in your context", for
     instruction files, which leave no other trace in the stream.

Every session runs behind a logging CONNECT proxy, with a clean environment
and its own HOME and CLAUDE_CONFIG_DIR. The token is read from a file, passed
only in the environment, and every file a run leaves behind is searched for
its literal value.
"""

import argparse
import json
import os
import re
import select
import shutil
import signal
import socket
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path

HERE = Path(__file__).resolve().parent
MCP_STUB = HERE / "mcp_stub.py"
TOKEN_FILE = Path.home() / ".config" / "sf-probe" / "oauth-token"
CHILD_PATH = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
CANARY_RE = re.compile(r"CANARY-[A-Z]+")

# Canary words, one per instruction file.
INSTRUCTION_FILES = {
    "CLAUDE.md": "CANARY-PROJECTMD",
    ".claude/CLAUDE.md": "CANARY-DOTCLAUDEMD",
    "CLAUDE.local.md": "CANARY-LOCALMD",
    ".claude/rules/probe.md": "CANARY-RULES",
    "AGENTS.md": "CANARY-AGENTSMD",
}
USER_MD_CANARY = "CANARY-USERMD"
APPEND_CANARY = "CANARY-APPENDED"

PROMPTS = {
    "canaries": (
        "List every word that starts with CANARY- and appears anywhere in your "
        "instructions or context. Reply with those words separated by spaces and "
        "nothing else. If there are none, reply NONE. Do not use any tools."
    ),
    "touch": (
        "Use the Bash tool to run exactly `touch perm-project-marker`, then exactly "
        "`touch perm-local-marker`, as two separate calls. Do not retry a call that "
        "is denied. Then reply with the single word DONE."
    ),
    "read": (
        "Use the Read tool to read the file README.md in the current directory, "
        "then reply with its first line and nothing else."
    ),
    "read-nested": (
        "Use the Read tool to read sub/file.txt. Then reply with two lines and nothing "
        "else. Line 1: every word that starts with CANARY- and appears anywhere in your "
        "instructions or context, or NONE. Line 2: the names of the skills available to "
        "you whose name starts with `nested-`, or NONE."
    ),
    "hello": "Reply with the single word READY. Do not use any tools.",
    "after-late-plant": (
        "Reply with two lines and nothing else. Line 1: every word that starts with "
        "CANARY- and appears anywhere in your instructions or context, or NONE. Line 2: "
        "the names of the skills available to you whose name starts with `late-`, or "
        "NONE. Do not use any tools."
    ),
    "structured": (
        "Use the Read tool to read README.md in the current directory. Then list "
        "every word that starts with CANARY- and appears anywhere in your "
        "instructions or context."
    ),
}

RESULT_SCHEMA = {
    "type": "object",
    "properties": {
        "first_line": {"type": "string"},
        "canaries": {"type": "array", "items": {"type": "string"}},
    },
    "required": ["first_line", "canaries"],
    "additionalProperties": False,
}

NONESSENTIAL = {"CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1"}
AGENTS_MD_OFF = {"pluginConfigs": {"cc-plugin-agents-md@builtin": {"options": {"instructionFiles": "claude-md"}}}}
SECOND_LAYER = {"disableAllHooks": True, "autoMemoryEnabled": False, **AGENTS_MD_OFF}

# AG-3a as SPEC draft 4 states it: no --bare, user settings only, a host MCP
# file and nothing else, a host --settings file.
AG3A = {"setting_sources": "user", "strict_mcp": True, "host_mcp": True, "host_settings": {}}


def case(name, about, **kw):
    base = {
        "name": name,
        "about": about,
        "fixture": "claude-md",
        "setting_sources": None,
        "strict_mcp": False,
        "host_mcp": False,
        "host_settings": None,  # None: no --settings; a dict: the hook plus these keys
        "flags": [],
        "env": dict(NONESSENTIAL),
        "allow": None,  # None: the proxy lets every host through
        "prompt": "canaries",
        "max_turns": 3,
        "trust": False,
        "stream_input": False,
        "turns": None,  # with stream_input: the prompts to send, one per turn
        "late_plant": None,  # after turn 1, write configuration into "work" or "config"
        "timeout": 180,
    }
    base.update(kw)
    return base


CASES = [
    # Controls: without any lock-down the planted items must load, or a later
    # "not loaded" proves nothing.
    case("baseline", "no lock-down flag; control for every planted item"),
    case("baseline-agents-md", "the same in a tree with AGENTS.md and no CLAUDE.md or CLAUDE.local.md", fixture="agents-md"),
    case("baseline-agents-md-local-md", "AGENTS.md next to a CLAUDE.local.md, no CLAUDE.md", fixture="agents-md-local-md"),
    # AG-3a as written.
    case("ag3a", "AG-3a: --setting-sources user, --strict-mcp-config, host --mcp-config and --settings", **AG3A),
    case("ag3a-agents-md", "AG-3a in a tree with AGENTS.md and no CLAUDE.md", fixture="agents-md", **AG3A),
    # The premise of AG-3a: bare mode cannot use the subscription token.
    case("bare-with-token", "AG-3a plus --bare", **{**AG3A, "flags": ["--bare"]}),
    # Each control of AG-3a alone.
    case("only-setting-sources", "--setting-sources user alone", setting_sources="user"),
    case("only-strict-mcp", "--strict-mcp-config with the host MCP file, alone", strict_mcp=True, host_mcp=True),
    # Each second-layer switch of report 11 alone, to see what it covers.
    case("only-disable-slash-commands", "--disable-slash-commands alone", flags=["--disable-slash-commands"]),
    case("only-disable-all-hooks", "disableAllHooks in --settings, alone", host_settings={"disableAllHooks": True}),
    case("only-claude-mds-env", "CLAUDE_CODE_DISABLE_CLAUDE_MDS=1 alone", env={**NONESSENTIAL, "CLAUDE_CODE_DISABLE_CLAUDE_MDS": "1"}),
    case(
        "only-claude-mds-env-agents-md",
        "CLAUDE_CODE_DISABLE_CLAUDE_MDS=1 alone, AGENTS.md tree",
        fixture="agents-md",
        env={**NONESSENTIAL, "CLAUDE_CODE_DISABLE_CLAUDE_MDS": "1"},
    ),
    case("only-agents-md-plugin-config", "the pluginConfigs entry for AGENTS.md in --settings, alone", fixture="agents-md", host_settings=AGENTS_MD_OFF),
    case(
        "ag3a-second-layer",
        "AG-3a plus --disable-slash-commands, disableAllHooks, the AGENTS.md entry and autoMemoryEnabled false",
        fixture="agents-md",
        **{**AG3A, "host_settings": SECOND_LAYER},
        flags=["--disable-slash-commands"],
    ),
    case("ag3a-disable-slash-commands", "AG-3a plus --disable-slash-commands", **{**AG3A, "flags": ["--disable-slash-commands"]}),
    # One-flag alternatives.
    case("safe-mode", "--safe-mode with the host MCP file and --settings", strict_mcp=True, host_mcp=True, host_settings={}, flags=["--safe-mode"]),
    case("restricted", "--restricted with the host MCP file and --settings", strict_mcp=True, host_mcp=True, host_settings={}, flags=["--restricted"]),
    # Permission rules of an excluded settings file (changelog 2.1.314).
    case("perm-baseline", "project and local permissions.allow rules, no lock-down", prompt="touch", max_turns=6, flags=["--permission-prompts", "none"]),
    case("perm-ag3a", "the same rules under AG-3a", prompt="touch", max_turns=6, **{**AG3A, "flags": ["--permission-prompts", "none"]}),
    # Linked git worktree whose main checkout holds .claude/ (report 11, 5).
    case("worktree-baseline", "linked worktree without .claude/, main checkout has one; no lock-down", fixture="worktree"),
    case("worktree-ag3a", "the same under AG-3a", fixture="worktree", **AG3A),
    # Instruction files and skills that the CLI picks up on demand, in a subdirectory.
    case("nested-baseline", "CLAUDE.md, a path-scoped rule and a skill for a subdirectory, read on demand; no lock-down", fixture="nested", prompt="read-nested", max_turns=5),
    case("nested-ag3a", "the same under AG-3a", fixture="nested", prompt="read-nested", max_turns=5, **AG3A),
    # Configuration that appears in the working directory while the session runs.
    case(
        "late-baseline",
        "hooks, a skill, CLAUDE.md and .mcp.json written between two turns into a tree that already has a .claude/; no lock-down",
        fixture="seeded-tree",
        stream_input=True,
        turns=["hello", "after-late-plant"],
        late_plant="work",
        flags=["--input-format", "stream-json"],
    ),
    case(
        "late-ag3a",
        "the same under AG-3a",
        fixture="seeded-tree",
        stream_input=True,
        turns=["hello", "after-late-plant"],
        late_plant="work",
        **{**AG3A, "flags": ["--input-format", "stream-json"]},
    ),
    case(
        "late-config-ag3a",
        "hooks and a skill written between two turns into the config directory, under AG-3a",
        fixture="seeded-tree",
        stream_input=True,
        turns=["hello", "after-late-plant"],
        late_plant="config",
        **{**AG3A, "flags": ["--input-format", "stream-json"]},
    ),
    # A plugin folder under .claude/skills/ and the trust record.
    case("plugin-untrusted", "plugin folder under .claude/skills/, no trust record, no lock-down", fixture="plugin"),
    case("plugin-trusted", "the same with a trust record in .claude.json; control", fixture="plugin", trust=True),
    case("plugin-trusted-ag3a", "trust record present, AG-3a", fixture="plugin", trust=True, **AG3A),
    # Hosts (SEC-11).
    case("net-default-env", "AG-3a, no traffic switch in the environment", env={}, **AG3A),
    case("net-allow-api-only", "AG-3a, the proxy allows api.anthropic.com only", allow=["api.anthropic.com"], **AG3A),
    case("net-allow-api-only-default-env", "the same without the traffic switch", allow=["api.anthropic.com"], env={}, **AG3A),
    case("net-deny-all", "AG-3a, the proxy denies every host", allow=[], timeout=420, **AG3A),
    # Caps (AG-14) and the whole AG-1 line.
    case("cap-max-turns", "--max-turns 1 on a prompt that needs two turns", prompt="read", max_turns=1, **AG3A),
    case("cap-max-budget", "--max-budget-usd 0.0001 on a prompt that needs two turns", prompt="read", max_turns=6, **{**AG3A, "flags": ["--max-budget-usd", "0.0001"]}),
    case(
        "full-ag1",
        "every flag of AG-1 except --bare, with stream-JSON input",
        prompt="structured",
        max_turns=6,
        stream_input=True,
        **{
            **AG3A,
            "flags": [
                "--input-format", "stream-json",
                "--permission-mode", "dontAsk",
                "--permission-prompts", "none",
                "--tools", "Read,Glob,Grep",
                "--allowedTools", "Read",
                "--json-schema", json.dumps(RESULT_SCHEMA),
                "--max-budget-usd", "1",
                "--append-system-prompt-file", "@APPEND@",
            ],
        },
    ),
]


# --------------------------------------------------------------------------
# Fixture


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def hook_command(canary: Path, label: str) -> str:
    """A hook that leaves a canary file holding its SF_PROBE_* environment."""
    return f"sh -c 'env | grep \"^SF_PROBE_\" > \"{canary / label}\"; exit 0'"


def hooks_block(canary: Path, origin: str, events=("SessionStart", "UserPromptSubmit", "PreToolUse", "Stop")) -> dict:
    block = {}
    for event in events:
        entry = {"hooks": [{"type": "command", "command": hook_command(canary, f"hook-{origin}-{event}")}]}
        if event == "PreToolUse":
            entry["matcher"] = "*"
        block[event] = [entry]
    return block


def mcp_file(canary: Path, name: str) -> dict:
    return {"mcpServers": {name: {"command": sys.executable, "args": [str(MCP_STUB), str(canary / f"mcp-{name}")]}}}


def skill_md(name: str) -> str:
    return f"---\nname: {name}\ndescription: Planted by the lock-down probe. Not for use.\n---\n\nReply with the word PLANTED.\n"


def plant_dot_claude(root: Path, prefix: str) -> None:
    """A skill, a command and a subagent under <root>/.claude/."""
    write(root / ".claude" / "skills" / f"{prefix}-skill" / "SKILL.md", skill_md(f"{prefix}-skill"))
    write(
        root / ".claude" / "commands" / f"{prefix}-command.md",
        "---\ndescription: Planted by the lock-down probe. Not for use.\n---\n\nReply with the word PLANTED.\n",
    )
    write(
        root / ".claude" / "agents" / f"{prefix}-agent.md",
        f"---\nname: {prefix}-agent\ndescription: Planted by the lock-down probe. Not for use.\n---\n\nYou are a planted subagent.\n",
    )


def git(cwd: Path, *args: str) -> None:
    env = {
        "PATH": CHILD_PATH,
        "HOME": str(cwd),
        "GIT_CONFIG_GLOBAL": "/dev/null",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_AUTHOR_NAME": "probe",
        "GIT_AUTHOR_EMAIL": "probe@example.invalid",
        "GIT_COMMITTER_NAME": "probe",
        "GIT_COMMITTER_EMAIL": "probe@example.invalid",
    }
    subprocess.run(["git", *args], cwd=cwd, env=env, check=True, capture_output=True)


def plant_repository(work: Path, canary: Path, fixture: str) -> Path:
    """Build the working directory and return the directory to run in."""
    work.mkdir(parents=True)
    write(work / "README.md", "lock-down probe fixture\n\nNothing to see here.\n")
    git(work, "init", "-q", "-b", "main")
    git(work, "add", "README.md")
    git(work, "commit", "-q", "-m", "fixture")

    if fixture == "worktree":
        # The main checkout holds an untracked .claude/; the linked worktree has none.
        plant_dot_claude(work, "main")
        linked = work.parent / "linked"
        git(work, "worktree", "add", "-q", str(linked), "-b", "linked")
        return linked

    if fixture == "seeded-tree":
        # A .claude/ the CLI finds, and watches, at start; plant_late() adds to it.
        plant_dot_claude(work, "seed")
        write(work / ".claude" / "settings.json", "{}\n")
        return work

    for rel, word in INSTRUCTION_FILES.items():
        if fixture.startswith("agents-md") and rel in ("CLAUDE.md", ".claude/CLAUDE.md"):
            continue
        if fixture == "agents-md" and rel == "CLAUDE.local.md":
            # A CLAUDE.local.md alone stops the CLI from reading AGENTS.md.
            continue
        write(work / rel, f"# Planted instructions\n\nThe canary word of the file `{rel}` is {word}.\n")

    plant_dot_claude(work, "probe")
    write(
        work / ".claude" / "settings.json",
        json.dumps(
            {
                "hooks": hooks_block(canary, "project"),
                "env": {"SF_PROBE_PROJECT_ENV": "set-by-project-settings"},
                "permissions": {"allow": ["Bash(touch perm-project-marker)"]},
            },
            indent=2,
        ),
    )
    write(
        work / ".claude" / "settings.local.json",
        json.dumps(
            {
                "hooks": hooks_block(canary, "local", events=("SessionStart",)),
                "env": {"SF_PROBE_LOCAL_ENV": "set-by-local-settings"},
                "permissions": {"allow": ["Bash(touch perm-local-marker)"]},
            },
            indent=2,
        ),
    )
    write(work / ".mcp.json", json.dumps(mcp_file(canary, "project-mcp"), indent=2))

    if fixture == "nested":
        write(work / "sub" / "file.txt", "a file in a subdirectory\n")
        write(work / "sub" / "CLAUDE.md", "# Planted instructions\n\nThe canary word of the file `sub/CLAUDE.md` is CANARY-NESTEDMD.\n")
        write(
            work / ".claude" / "rules" / "scoped.md",
            '---\npaths:\n  - "sub/**"\n---\n\nThe canary word of the path-scoped rule is CANARY-SCOPEDRULE.\n',
        )
        write(work / "sub" / ".claude" / "skills" / "nested-skill" / "SKILL.md", skill_md("nested-skill"))

    if fixture == "plugin":
        plugin = work / ".claude" / "skills" / "probe-plugin"
        write(
            plugin / ".claude-plugin" / "plugin.json",
            json.dumps({"name": "probe-plugin", "version": "0.0.1", "description": "Planted by the lock-down probe."}),
        )
        write(plugin / "skills" / "plugin-skill" / "SKILL.md", skill_md("plugin-skill"))
        write(plugin / "hooks" / "hooks.json", json.dumps({"hooks": hooks_block(canary, "plugin", events=("SessionStart",))}))
    return work


def plant_late(cwd: Path, canary: Path) -> None:
    """What an agent could write into its own working directory mid-session."""
    write(cwd / "CLAUDE.md", "# Planted instructions\n\nThe canary word of the late `CLAUDE.md` is CANARY-LATEMD.\n")
    write(cwd / ".claude" / "skills" / "late-skill" / "SKILL.md", skill_md("late-skill"))
    events = ("UserPromptSubmit", "Stop")
    write(cwd / ".claude" / "settings.json", json.dumps({"hooks": hooks_block(canary, "late-project", events=events)}, indent=2))
    write(cwd / ".claude" / "settings.local.json", json.dumps({"hooks": hooks_block(canary, "late-local", events=events)}, indent=2))
    write(cwd / ".mcp.json", json.dumps(mcp_file(canary, "late-mcp"), indent=2))


def plant_late_config(config: Path, canary: Path) -> None:
    """The same, written into the per-execution config directory (the `user` source)."""
    write(config / "settings.json", json.dumps({"hooks": hooks_block(canary, "late-user", events=("UserPromptSubmit", "Stop"))}, indent=2))
    write(config / "skills" / "late-user-skill" / "SKILL.md", skill_md("late-user-skill"))


def plant_user_config(config: Path, canary: Path, trusted: Path | None) -> None:
    """The per-execution config directory (AG-4): the `user` source."""
    write(config / "settings.json", json.dumps({"hooks": hooks_block(canary, "user", events=("SessionStart",))}, indent=2))
    write(config / "CLAUDE.md", f"# User instructions\n\nThe canary word of the user-level file is {USER_MD_CANARY}.\n")
    write(config / "skills" / "user-skill" / "SKILL.md", skill_md("user-skill"))
    if trusted is not None:
        write(config / ".claude.json", json.dumps({"projects": {str(trusted): {"hasTrustDialogAccepted": True}}}))


# --------------------------------------------------------------------------
# Egress proxy and connection watcher


class Proxy:
    """A CONNECT proxy that logs each requested host and can refuse hosts."""

    def __init__(self, allow):
        self.allow = None if allow is None else set(allow)
        self.log = []
        self.started = time.monotonic()
        self.sock = socket.socket()
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(64)
        self.port = self.sock.getsockname()[1]
        threading.Thread(target=self._accept, daemon=True).start()

    def _accept(self):
        while True:
            try:
                conn, _ = self.sock.accept()
            except OSError:
                return
            threading.Thread(target=self._serve, args=(conn,), daemon=True).start()

    def _record(self, method, host, port, verdict):
        entry = {"t": round(time.monotonic() - self.started, 2), "method": method, "host": host, "port": port, "verdict": verdict}
        self.log.append(entry)
        return entry

    def _serve(self, conn):
        upstream = None
        try:
            conn.settimeout(30)
            head = b""
            while b"\r\n\r\n" not in head and len(head) < 65536:
                chunk = conn.recv(4096)
                if not chunk:
                    return
                head += chunk
            method, target, _ = head.split(b"\r\n", 1)[0].decode("latin-1").split(" ", 2)
            if method != "CONNECT":
                # Plain HTTP is never forwarded; a request for it is itself the finding.
                host = re.sub(r"^https?://", "", target).split("/", 1)[0]
                self._record(method, host, 80, "refused-plain-http")
                conn.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                return
            host, _, port = target.rpartition(":")
            if self.allow is not None and host not in self.allow:
                self._record(method, host, int(port), "denied")
                conn.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                return
            try:
                upstream = socket.create_connection((host, int(port)), timeout=30)
            except OSError as err:
                self._record(method, host, int(port), f"unreachable: {err}")
                conn.sendall(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                return
            entry = self._record(method, host, int(port), "allowed")
            conn.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
            conn.settimeout(None)
            sent = received = 0
            while True:
                readable, _, _ = select.select([conn, upstream], [], [], 300)
                if not readable:
                    break
                done = False
                for src in readable:
                    data = src.recv(65536)
                    if not data:
                        done = True
                        break
                    if src is conn:
                        upstream.sendall(data)
                        sent += len(data)
                    else:
                        conn.sendall(data)
                        received += len(data)
                entry["bytes_out"], entry["bytes_in"] = sent, received
                if done:
                    break
        except (OSError, ValueError):
            pass
        finally:
            conn.close()
            if upstream is not None:
                upstream.close()

    def close(self):
        self.sock.close()


def process_tree(root: int) -> list[int]:
    out = subprocess.run(["ps", "-axo", "pid=,ppid="], capture_output=True, text=True).stdout
    children = {}
    for line in out.splitlines():
        pid, ppid = line.split()
        children.setdefault(int(ppid), []).append(int(pid))
    tree, stack = [], [root]
    while stack:
        pid = stack.pop()
        tree.append(pid)
        stack.extend(children.get(pid, []))
    return tree


class ConnectionWatcher(threading.Thread):
    """Samples the sockets of the CLI's process tree for traffic past the proxy.

    A sample every 0.3 s can miss a short connection, and a lookup made through
    the system resolver is not a socket of these processes. It is a check on
    the proxy log, not a packet capture.
    """

    def __init__(self, root: int, proxy_port: int):
        super().__init__(daemon=True)
        self.root, self.proxy_port = root, proxy_port
        self.seen = set()
        self.samples = 0
        self.stop = threading.Event()

    def run(self):
        via_proxy = f"->127.0.0.1:{self.proxy_port}"
        while not self.stop.is_set():
            pids = ",".join(str(p) for p in process_tree(self.root))
            out = subprocess.run(["lsof", "-nP", "-i", "-a", "-p", pids], capture_output=True, text=True).stdout
            self.samples += 1
            for line in out.splitlines()[1:]:
                parts = line.split()
                if len(parts) < 9:
                    continue
                name = " ".join(parts[7:])
                if via_proxy not in name:
                    self.seen.add(f"{parts[0]} {name}")
            self.stop.wait(0.3)


# --------------------------------------------------------------------------
# One session


def build_command(spec, model, session_id, host_dir: Path, canary: Path) -> list[str]:
    cmd = ["claude", "-p", "--output-format", "stream-json", "--verbose", "--model", model, "--session-id", session_id]
    cmd += ["--max-turns", str(spec["max_turns"])]
    cmd += ["--debug-file", str(host_dir / "debug.log")]
    if spec["setting_sources"]:
        cmd += ["--setting-sources", spec["setting_sources"]]
    if spec["host_mcp"]:
        write(host_dir / "mcp.json", json.dumps(mcp_file(canary, "host-mcp"), indent=2))
        cmd += ["--mcp-config", str(host_dir / "mcp.json")]
    if spec["strict_mcp"]:
        # After --mcp-config, which takes several values: as the last option
        # before the prompt it takes the prompt for a second file.
        cmd += ["--strict-mcp-config"]
    if spec["host_settings"] is not None:
        settings = {"hooks": hooks_block(canary, "flag", events=("SessionStart",)), **spec["host_settings"]}
        write(host_dir / "settings.json", json.dumps(settings, indent=2))
        cmd += ["--settings", str(host_dir / "settings.json")]
    write(host_dir / "append.md", f"The canary word of the appended system prompt file is {APPEND_CANARY}.\n")
    cmd += [str(host_dir / "append.md") if flag == "@APPEND@" else flag for flag in spec["flags"]]
    return cmd


def run_session(spec, model, token, out: Path) -> dict:
    run = out / spec["name"]
    if run.exists():
        shutil.rmtree(run)
    canary, host_dir, home = run / "canary", run / "host", run / "home"
    for d in (canary, host_dir, home, run / "tmp"):
        d.mkdir(parents=True)
    cwd = plant_repository(run / "work", canary, spec["fixture"]).resolve()
    config = home / ".claude"
    plant_user_config(config, canary, cwd if spec["trust"] else None)

    proxy = Proxy(spec["allow"])
    proxy_url = f"http://127.0.0.1:{proxy.port}"
    env = {
        "PATH": CHILD_PATH,
        "HOME": str(home),
        "CLAUDE_CONFIG_DIR": str(config),
        "CLAUDE_CODE_OAUTH_TOKEN": token,
        "TMPDIR": str(run / "tmp"),
        "LANG": "en_US.UTF-8",
        "HTTPS_PROXY": proxy_url,
        "HTTP_PROXY": proxy_url,
        "https_proxy": proxy_url,
        "http_proxy": proxy_url,
        **spec["env"],
    }
    session_id = str(uuid.uuid4())
    cmd = build_command(spec, model, session_id, host_dir, canary)
    turns = [PROMPTS[key] for key in (spec["turns"] or [spec["prompt"]])]
    if not spec["stream_input"]:
        cmd.append(turns[0])

    def send(proc, text):
        message = {"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": text}]}}
        proc.stdin.write((json.dumps(message) + "\n").encode())
        proc.stdin.flush()

    started = time.monotonic()
    with open(run / "stderr.txt", "wb") as stderr:
        proc = subprocess.Popen(
            cmd,
            cwd=cwd,
            env=env,
            stdin=subprocess.PIPE if spec["stream_input"] else subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=stderr,
            start_new_session=True,
        )
        watcher = ConnectionWatcher(proc.pid, proxy.port)
        watcher.start()
        timed_out = threading.Event()

        def kill():
            timed_out.set()
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

        timer = threading.Timer(spec["timeout"], kill)
        timer.start()
        if spec["stream_input"]:
            send(proc, turns[0])

        events, result_at, stdin_closed_at, results_seen = [], None, None, 0
        with open(run / "events.jsonl", "wb") as raw:
            for line in proc.stdout:
                raw.write(line)  # the raw stream is kept before it is parsed (AG-6)
                raw.flush()
                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    event = {"type": "unparsed", "text": line.decode("utf-8", "replace")[:500]}
                event["_t"] = round(time.monotonic() - started, 2)
                events.append(event)
                if event.get("type") != "result":
                    continue
                results_seen += 1
                if not spec["stream_input"]:
                    result_at = result_at or time.monotonic()
                elif results_seen < len(turns):
                    if spec["late_plant"] and results_seen == 1:
                        if spec["late_plant"] == "config":
                            plant_late_config(config, canary)
                        else:
                            plant_late(cwd, canary)
                        time.sleep(4)  # the CLI watches these paths; give it time to notice
                        events.append({"type": "_probe", "late_plant_written_at": round(time.monotonic() - started, 2)})
                    send(proc, turns[results_seen])
                elif result_at is None:
                    # With stream-JSON input the CLI waits for more input: see whether
                    # it is still alive a moment later, then close stdin.
                    result_at = time.monotonic()
                    time.sleep(3)
                    events.append({"type": "_probe", "alive_3s_after_result": proc.poll() is None})
                    stdin_closed_at = time.monotonic()
                    proc.stdin.close()
        exit_code = proc.wait()
        ended = time.monotonic()
        timer.cancel()
        watcher.stop.set()
        watcher.join()
    proxy.close()

    token_hits = []
    needle = token.encode()
    for path in run.rglob("*"):
        if path.is_file() and not path.is_symlink() and needle in path.read_bytes():
            token_hits.append(str(path.relative_to(run)))

    summary = summarise(spec, cmd, events, run, cwd, canary, config)
    summary.update(
        {
            "exit_code": exit_code,
            "timed_out": timed_out.is_set(),
            "wall_seconds": round(ended - started, 1),
            "seconds_from_result_to_exit": None if result_at is None else round(ended - result_at, 1),
            "seconds_from_stdin_close_to_exit": None if stdin_closed_at is None else round(ended - stdin_closed_at, 1),
            "proxy": {"mode": "allow all" if spec["allow"] is None else f"allow only {sorted(spec['allow'])}", "requests": proxy.log},
            "hosts": hosts_table(proxy.log),
            "sockets_not_to_the_proxy": sorted(watcher.seen),
            "socket_samples": watcher.samples,
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


def hosts_table(log) -> dict:
    table = {}
    for entry in log:
        row = table.setdefault(f"{entry['host']}:{entry['port']}", {"requests": 0, "verdicts": []})
        row["requests"] += 1
        if entry["verdict"] not in row["verdicts"]:
            row["verdicts"].append(entry["verdict"])
    return table


def summarise(spec, cmd, events, run: Path, cwd: Path, canary: Path, config: Path) -> dict:
    inits = [e for e in events if e.get("type") == "system" and e.get("subtype") == "init"]
    init = inits[0] if inits else {}
    results = [e for e in events if e.get("type") == "result"]
    result = results[-1] if results else {}
    texts, usages, tool_uses = [], {}, []
    for event in events:
        if event.get("type") != "assistant":
            continue
        message = event.get("message", {})
        usages[message.get("id")] = message.get("usage")
        for block in message.get("content", []):
            if block.get("type") == "text":
                texts.append(block["text"])
            elif block.get("type") == "tool_use":
                tool_uses.append({"name": block["name"], "input": block.get("input")})
    reply = "\n".join(texts + [str(result.get("result", "")), json.dumps(result.get("structured_output", ""))])
    words = sorted(set(CANARY_RE.findall(reply)))

    canaries = {p.name: p.read_text("utf-8", "replace").split() for p in sorted(canary.iterdir())}
    listed = {key: init.get(key) for key in ("skills", "slash_commands", "agents", "plugins", "mcp_servers")}
    names = lambda key: [x if isinstance(x, str) else x.get("name") for x in (listed.get(key) or [])]  # noqa: E731
    mcp = {s.get("name"): s.get("status") for s in (listed.get("mcp_servers") or [])}

    def any_named(*wanted):
        pool = names("skills") + names("slash_commands") + names("agents") + names("plugins")
        return any(w in item for item in pool if item for w in wanted)

    # Names the CLI announced after the first init: with stream-JSON input every
    # turn starts with a new init, and a changed command list is its own event.
    later = [n for e in inits[1:] for key in ("skills", "slash_commands") for n in e.get(key) or []]
    later += [c.get("name") for e in events if e.get("subtype") == "commands_changed" for c in e.get("commands") or []]
    debug = debug_lines(run / "host" / "debug.log", run)

    # Environment of processes the CLI started, as the canary files recorded it.
    child_env = sorted({line for lines in canaries.values() for line in lines})

    # What the working directory planted. Anything but False, None or [] means it took effect.
    from_repository = {
        "CLAUDE.md": "CANARY-PROJECTMD" in words,
        ".claude/CLAUDE.md": "CANARY-DOTCLAUDEMD" in words,
        "CLAUDE.local.md": "CANARY-LOCALMD" in words,
        ".claude/rules/": "CANARY-RULES" in words,
        "AGENTS.md": "CANARY-AGENTSMD" in words,
        "project skill": any_named("probe-skill"),
        "project command": any_named("probe-command"),
        "project subagent": any_named("probe-agent"),
        "project hooks (.claude/settings.json)": sorted(n for n in canaries if n.startswith("hook-project-")),
        "local hooks (.claude/settings.local.json)": sorted(n for n in canaries if n.startswith("hook-local-")),
        "project env block": any("SF_PROBE_PROJECT_ENV" in line for line in child_env),
        "local env block": any("SF_PROBE_LOCAL_ENV" in line for line in child_env),
        "project permissions.allow": (cwd / "perm-project-marker").exists(),
        "local permissions.allow": (cwd / "perm-local-marker").exists(),
        ".mcp.json server started": "mcp-project-mcp" in canaries,
        ".mcp.json server listed": mcp.get("project-mcp"),
        "main checkout .claude/ read from a linked worktree": any_named("main-skill", "main-command", "main-agent"),
        "plugin folder under .claude/skills/": any_named("probe-plugin", "plugin-skill") or "hook-plugin-SessionStart" in canaries,
        "CLAUDE.md of a subdirectory, on reading a file there": "CANARY-NESTEDMD" in words,
        "path-scoped rule, on reading a matching file": "CANARY-SCOPEDRULE" in words,
        "skill of a subdirectory, on reading a file there": "nested-skill" in later,
        "CLAUDE.md written mid-session": "CANARY-LATEMD" in words,
        "skill written mid-session": "late-skill" in later,
        "hooks of a .claude/settings.json written mid-session": sorted(n for n in canaries if n.startswith("hook-late-project-")),
        "hooks of a .claude/settings.local.json written mid-session": sorted(n for n in canaries if n.startswith("hook-late-local-")),
        ".mcp.json written mid-session": "mcp-late-mcp" in canaries,
    }
    # The config directory is the `user` source: the host's own, but only where the
    # sandbox keeps the worker from writing to it (AG-4).
    from_config_directory_mid_session = {
        "user hooks written mid-session into the config directory": sorted(n for n in canaries if n.startswith("hook-late-user-")),
        "user skill written mid-session into the config directory": "late-user-skill" in later,
    }
    # The host's own configuration. It should keep working under a lock-down.
    from_host = {
        "user CLAUDE.md": USER_MD_CANARY in words,
        "user skill": any_named("user-skill"),
        "user hook": "hook-user-SessionStart" in canaries,
        "--settings hook": "hook-flag-SessionStart" in canaries,
        "--mcp-config server started": "mcp-host-mcp" in canaries,
        "--mcp-config server listed": mcp.get("host-mcp"),
        "--append-system-prompt-file": APPEND_CANARY in words,
    }

    transcript_files = sorted(str(p.relative_to(config)) for p in config.rglob("*") if p.is_file())
    return {
        "case": spec["name"],
        "about": spec["about"],
        "fixture": spec["fixture"],
        "command": [c if c != cmd[-1] or spec["stream_input"] else "<prompt>" for c in cmd],
        "environment_switches": spec["env"],
        "prompts": [PROMPTS[key] for key in (spec["turns"] or [spec["prompt"]])],
        "took_effect": {
            "from_the_repository": from_repository,
            "from_the_config_directory_mid_session": from_config_directory_mid_session,
            "from_the_host": from_host,
        },
        "canary_words_in_reply": words,
        "canary_files": sorted(canaries),
        "environment_seen_by_children": child_env,
        "tool_uses": tool_uses,
        "init": {
            "keys": sorted(k for k in init if k != "_t"),
            "claude_code_version": init.get("claude_code_version"),
            "apiKeySource": init.get("apiKeySource"),
            "model": init.get("model"),
            "permissionMode": init.get("permissionMode"),
            "memory_paths": init.get("memory_paths"),
            "capabilities": init.get("capabilities"),
            "tools": init.get("tools"),
            **listed,
        },
        "event_sequence": [
            "/".join(str(x) for x in (e.get("type"), e.get("subtype")) if x) + f"@{e.get('_t')}" for e in events if e.get("type") != "_probe"
        ],
        "probe_notes": [e for e in events if e.get("type") == "_probe"],
        "rate_limit_events": [e for e in events if e.get("type") == "rate_limit_event"],
        "api_retry_events": [e for e in events if e.get("subtype") == "api_retry"],
        "system_events_other_than_init": [e for e in events if e.get("type") == "system" and e.get("subtype") not in ("init", "api_retry")][:12],
        "assistant_usage_by_message_id": usages,
        "result": result,
        "results_before_the_last": results[:-1],
        "debug_log_lines": debug,
        "config_dir_files_after_run": transcript_files,
    }


DEBUG_WANTED = re.compile(
    r"permissions\.allow|permission update|Loading skills from|Loaded \d+ unique skills|Watching for changes"
    r"|plugin cc-plugin-agents-md:|Policy limits: (Fetched|Failed)|Remote settings: (No|Fetched|Failed|Loaded)"
    r"|scope|MCP server \"[^\"]+\": (Starting|Connection established)|Hooks: \w+ \[|Found \d+ plugins"
    r"|trust|[Nn]onessential|\[API REQUEST\]|isTelemetryEnabled|[Ss]afe mode|[Rr]estricted|[Bb]are mode"
    r"|setting sources|settingSources|disableAllHooks|hooks? (are )?disabled|slash commands"
    r"|Settings changed|[Dd]ynamic(ally)? .*skills?|skills? (changed|reload)|Detected change|Reloading"
)


def debug_lines(path: Path, run: Path) -> list[str]:
    """The lines of the CLI's debug log that say what it loaded or skipped."""
    if not path.exists():
        return []
    lines = []
    for line in path.read_text("utf-8", "replace").splitlines():
        # The run directory is named after the case; keep the name out of the match.
        line = line.replace(str(run.resolve()), "<run>").replace(str(run), "<run>")
        line = re.sub(r"^\S+Z ", "", line)
        if ".claude.json" in line:
            continue
        if DEBUG_WANTED.search(line) and line not in lines:
            lines.append(line[:500])
    return lines[:150]


# --------------------------------------------------------------------------
# Report


def effective(group: dict) -> list[str]:
    return [key for key, value in group.items() if value not in (False, None, [])]


def overview(summaries: list[dict]) -> str:
    """One table row per case: what took effect, how the session ended, the hosts."""
    lines = [
        "# Lock-down probe: overview",
        "",
        "Generated by `lockdown_probe.py` from the `<case>.json` files next to it.",
        "",
        "| Case | What it is | Took effect from the repository | From the host | Exit, result | Hosts asked for |",
        "|---|---|---|---|---|---|",
    ]
    for s in summaries:
        took = s["took_effect"]
        repository = effective(took["from_the_repository"]) + effective(took["from_the_config_directory_mid_session"])
        hosts = [f"{host} ({row['requests']}, {'/'.join(row['verdicts'])})" for host, row in s["hosts"].items()]
        ending = f"{s['exit_code']}, {s['result'].get('subtype')}" + (", killed at the time limit" if s["timed_out"] else "")
        cells = [
            f"`{s['case']}`",
            s["about"],
            "; ".join(repository) or "nothing",
            "; ".join(effective(took["from_the_host"])) or "nothing",
            ending,
            "; ".join(hosts) or "none",
        ]
        lines.append("| " + " | ".join(c.replace("|", "\\|") for c in cells) + " |")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("-k", metavar="TEXT", action="append", help="run the cases whose name contains TEXT (repeatable)")
    parser.add_argument("--list", action="store_true", help="list the cases and exit")
    parser.add_argument("--model", default="haiku")
    parser.add_argument("--out", type=Path, required=False, help="scratch directory for the runs (kept, never committed)")
    parser.add_argument("--results", type=Path, default=HERE / "results")
    parser.add_argument("--keep-events", metavar="CASE", action="append", default=[], help="also copy the raw stream of CASE into the results")
    args = parser.parse_args()

    selected = [c for c in CASES if not args.k or any(k in c["name"] for k in args.k)]
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
        print(f"== {spec['name']}: {spec['about']}", flush=True)
        summary = run_session(spec, args.model, token, args.out)
        text = json.dumps(summary, indent=2, sort_keys=False)
        if token in text:
            print("   the token is in the summary; nothing written", flush=True)
            return 1
        (args.results / f"{spec['name']}.json").write_text(text + "\n", encoding="utf-8")
        if spec["name"] in args.keep_events:
            raw = (args.out / spec["name"] / "events.jsonl").read_text("utf-8")
            (args.results / f"{spec['name']}.events.jsonl").write_text(scrub(raw, args.out), encoding="utf-8")
        took = summary["took_effect"]
        loaded = effective(took["from_the_repository"]) + effective(took["from_the_config_directory_mid_session"])
        print(f"   exit {summary['exit_code']}, result {summary['result'].get('subtype')}, {summary['wall_seconds']} s", flush=True)
        print(f"   from the repository: {', '.join(loaded) or 'nothing'}", flush=True)
        print(f"   from the host: {', '.join(effective(took['from_the_host'])) or 'nothing'}", flush=True)
        print(f"   hosts: {', '.join(summary['hosts']) or 'none'}", flush=True)
        if summary["files_holding_the_token"]:
            print(f"   TOKEN LITERAL FOUND IN: {summary['files_holding_the_token']}", flush=True)

    summaries = [json.loads(p.read_text()) for p in (args.results / f"{c['name']}.json" for c in CASES) if p.exists()]
    if summaries:
        (args.results / "overview.md").write_text(overview(summaries), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
