#!/usr/bin/env python3
"""PROTOTYPE: where does Claude Code read and write on Linux, outside what SEC-6 gives it?

Throwaway code for issue #85 and probe P-36, part 3 (SPEC 13.3 SEC-6, 10.3 AG-4). Never
merged. On macOS the CLI wrote a subagent's transcript and a socket under /tmp whatever
TMPDIR said (P-26), and looked for managed settings in a system location whatever the
flags (P-21). This asks the same of the Linux build, in the Lima guest.

Each case is one real `claude -p` session with the flags of AG-1 in `subscription` mode
and the environment of AG-11, TMPDIR included, which names a directory of the session's
own. The cases:

- trace: no outer sandbox, under strace. Which paths does the CLI look up, and which does
  it write, outside the working directory, the config directory and TMPDIR?
- trace-tmpdir-0775: the same with a TMPDIR that the group may write. The CLI refuses
  such a directory for its socket.
- standin: the `nouserns` stand-in of P-6 (bubblewrap with --disable-userns), with a
  directory of the run bound at /tmp, so that what is written there can be listed.
- standin-tmp-ro: the same with /tmp read-only. Does a session with a subagent complete
  when TMPDIR is the only temporary directory it can write?
- standin-tmp-ro-0775: the same with a TMPDIR that the group may write, so that the CLI
  has no directory for its socket.
- managed-visible: a managed-settings.json planted in /etc/claude-code, which sets one
  environment variable; the stand-in binds /etc as it is. Does a session started with
  `--setting-sources user` get the variable?
- managed-hidden: the same with an empty directory mounted over /etc/claude-code.

TMPDIR has mode 0700 unless the case says otherwise. The sessions of `trace` and of the
three `standin` cases are asked to start one subagent and to run one shell script,
`sh ./inside.sh`, which is the only command they may run. The others run the script alone.

    python3 cli_paths_probe.py --list
    python3 cli_paths_probe.py --no-login            # `trace` without a token: the start only
    python3 cli_paths_probe.py --token-stdin < token # every case

Run it as an ordinary user. The two `managed` cases need `sudo` without a password, to
plant and remove /etc/claude-code/managed-settings.json; they are skipped where that
directory already exists. The token is passed to the sessions in the environment only.
The script works under ~/sf-p36-* and removes it. The output names no host, no
account and no token.
"""

import argparse
import json
import os
import re
import shutil
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from collections import Counter
from pathlib import Path

HOME = Path.home()
SEARCH_PATH = f"{os.environ.get('PATH', '/usr/bin:/bin')}:{HOME}/.local/bin"
COMMAND = "sh ./inside.sh"
MANAGED_DIR = Path("/etc/claude-code")
MANAGED_VALUE = "set-by-managed-settings"

PROMPT_SUBAGENT = (
    'Call the Agent tool exactly once, with subagent_type "general-purpose", and give the subagent this '
    f"task word for word: 'Run the shell command `{COMMAND}` with the Bash tool and report the last line it "
    f"prints.' When the subagent has answered, run the shell command `{COMMAND}` yourself with the Bash tool. "
    "Then answer with the one word: done."
)
PROMPT_BASH = f"Run the shell command `{COMMAND}` with the Bash tool, then answer with the one word: done."

# name: (outer sandbox, how /tmp is mounted, mode of TMPDIR, managed settings, prompt, traced)
CASES = {
    "trace": ("none", None, 0o700, None, PROMPT_SUBAGENT, True),
    "trace-tmpdir-0775": ("none", None, 0o775, None, PROMPT_BASH, True),
    "standin": ("nouserns", "rw", 0o700, None, PROMPT_SUBAGENT, False),
    "standin-tmp-ro": ("nouserns", "ro", 0o700, None, PROMPT_SUBAGENT, False),
    "standin-tmp-ro-0775": ("nouserns", "ro", 0o775, None, PROMPT_SUBAGENT, False),
    "managed-visible": ("nouserns", "rw", 0o700, "visible", PROMPT_BASH, False),
    "managed-hidden": ("nouserns", "rw", 0o700, "hidden", PROMPT_BASH, False),
}

# Run by the Bash tool, of the main loop and of the subagent.
INSIDE_SH = """#!/bin/sh
{
  echo "--- run pid $$"
  echo "TMPDIR=${TMPDIR-unset} CLAUDE_CODE_TMPDIR=${CLAUDE_CODE_TMPDIR-unset} SF_P36_MANAGED=${SF_P36_MANAGED-unset}"
  echo "variables named after a temporary directory, a socket or the CLI: $(env | grep -E '^[A-Z_]*(TMP|TEMP|SOCK|CLAUDE)[A-Z_]*=' | grep -v -E 'TOKEN|KEY' | sort | tr '\\n' ' ')"
  for target in ./canary-cwd /tmp/sf-p36-canary "${TMPDIR:-/nonexistent}/sf-p36-canary"; do
    if (: >"$target") 2>/dev/null; then echo "write $target: ok"; else echo "write $target: denied"; fi
  done
  echo "mktemp: $(mktemp 2>&1)"
  echo "user namespace: $(readlink /proc/self/ns/user)"
  echo "inside.sh ran to its end"
} 2>&1 | tee -a ./observed.txt
exit 0
"""


class Redactor:
    """Keeps the token, the account, the host and the scratch directory out of the output."""

    def __init__(self):
        self.pairs = []
        self.token = None

    def add(self, literal, label):
        if literal and len(literal) >= 3:
            self.pairs.append((literal, label))
            self.pairs.sort(key=lambda pair: -len(pair[0]))

    def clean(self, text):
        if self.token and self.token in text:
            text = text.replace(self.token, "<TOKEN: the literal was here>")
        for literal, label in self.pairs:
            text = text.replace(literal, label)
        return text


REDACT = Redactor()


def out(text=""):
    print(REDACT.clean(text), flush=True)


def indent(text, prefix="    "):
    return "\n".join(prefix + line for line in text.rstrip("\n").split("\n"))


def run(cmd, **kwargs):
    try:
        done = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60, **kwargs)
    except (OSError, subprocess.TimeoutExpired) as error:
        return None, f"{type(error).__name__}: {error}"
    return done.returncode, done.stdout.decode("utf-8", "replace").strip()


def read(path: Path):
    return path.read_text(errors="replace") if path.exists() else ""


def standin(run_dir: Path, claude: Path, tmp_mode, hide_managed):
    """The `nouserns` stand-in of P-6, with /tmp a directory of the run and not a tmpfs."""
    args = ["bwrap", "--unshare-user", "--unshare-pid", "--unshare-ipc", "--unshare-uts", "--unshare-cgroup-try"]
    # As in P-6 the network namespace is shared: the proxy of SEC-10 does not exist yet.
    args += ["--disable-userns", "--die-with-parent", "--new-session", "--hostname", "sandbox"]
    args += ["--ro-bind", "/usr", "/usr", "--ro-bind", "/etc", "/etc"]
    for name in ("bin", "sbin", "lib", "lib32", "lib64", "libx32"):
        path = Path("/") / name
        if path.is_symlink():
            args += ["--symlink", os.readlink(path), str(path)]
        elif path.is_dir():
            args += ["--ro-bind", str(path), str(path)]
    resolv = Path("/etc/resolv.conf").resolve()
    if not str(resolv).startswith(("/etc/", "/usr/")) and resolv.exists():
        args += ["--ro-bind", str(resolv), str(resolv)]
    args += ["--proc", "/proc", "--dev", "/dev"]
    args += ["--bind" if tmp_mode == "rw" else "--ro-bind", str(run_dir / "tmp"), "/tmp"]
    if hide_managed:
        args += ["--ro-bind", str(run_dir / "empty"), str(MANAGED_DIR)]
    args += ["--ro-bind", str(claude), "/sf/bin/claude", "--ro-bind", str(run_dir / "host"), "/sf/host"]
    for name in ("work", "home", "tmpdir", "out"):
        args += ["--bind", str(run_dir / name), f"/sf/{name}"]
    args += ["--chdir", "/sf/work", "--"]
    return args


def listing(root: Path, limit=120):
    """Every entry under root, as `type size path`."""
    rows = []
    for path in sorted(root.rglob("*")):
        try:
            info = path.lstat()
        except OSError:
            continue
        kind = (
            "socket" if stat.S_ISSOCK(info.st_mode) else "link" if stat.S_ISLNK(info.st_mode)
            else "dir" if stat.S_ISDIR(info.st_mode) else "fifo" if stat.S_ISFIFO(info.st_mode) else "file"
        )
        rows.append(f"{kind:6} {info.st_size if kind == 'file' else '-':>8} {path.relative_to(root)}")
    if not rows:
        return "(empty)"
    more = [f"... and {len(rows) - limit} more"] if len(rows) > limit else []
    return "\n".join(rows[:limit] + more)


def top_level(root: Path):
    return {path.name for path in root.iterdir()} if root.is_dir() else set()


def path_strings(node, trail=""):
    """Every string in a JSON value that is one absolute path, with the keys that lead to it."""
    if isinstance(node, dict):
        for key, value in node.items():
            yield from path_strings(value, f"{trail}.{key}" if trail else key)
    elif isinstance(node, list):
        for value in node:
            yield from path_strings(value, trail + "[]")
    elif isinstance(node, str) and node.startswith("/") and len(node) < 300 and not re.search(r"\s", node):
        yield trail, node


def events_of(run_dir: Path):
    events = []
    for line in read(run_dir / "out" / "events.jsonl").splitlines():
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            events.append({"type": "unparsed", "text": line[:300]})
    return events


def blocks(events, role, kind):
    for event in events:
        if event.get("type") != role:
            continue
        content = event.get("message", {}).get("content", [])
        for block in content if isinstance(content, list) else []:
            if isinstance(block, dict) and block.get("type") == kind:
                yield block


def text_of(content):
    if isinstance(content, str):
        return content
    return "\n".join(part.get("text", "") for part in content if isinstance(part, dict))


# --- strace -------------------------------------------------------------------------

WRITE_CALLS = {
    "mkdir", "mkdirat", "unlink", "unlinkat", "rmdir", "rename", "renameat", "renameat2", "symlink",
    "symlinkat", "link", "linkat", "chmod", "fchmodat", "chown", "fchownat", "truncate", "mknod", "mknodat",
    "utimensat", "bind",
}
LINE = re.compile(r"^(\d+)\s+(.*)$")
CALL = re.compile(r"^(\w+)\((.*)\)\s+= (-?\d+|\?)(?: (E\w+))?")
QUOTED = re.compile(r'"((?:[^"\\]|\\.)*)"')


def trace_rows(trace: Path, root_pid_label="claude"):
    """(actor, call, path, outcome, is a write) for every traced call that names a path."""
    actor, pending, first = {}, {}, None
    for raw in read(trace).splitlines():
        match = LINE.match(raw)
        if not match:
            continue
        pid, rest = match.group(1), match.group(2)
        if first is None:
            first = pid
            actor[pid] = root_pid_label
        if rest.endswith("<unfinished ...>"):
            pending[pid] = rest[: -len("<unfinished ...>")]
            continue
        resumed = re.match(r"^<\.\.\. \w+ resumed>(.*)$", rest)
        if resumed:
            rest = pending.pop(pid, "") + resumed.group(1)
        call = CALL.match(rest)
        if not call:
            continue
        name, args, value, errno = call.groups()
        if name in ("clone", "clone3", "fork", "vfork"):
            if value.isdigit() and value != "0":
                actor[value] = actor.get(pid, "?")
            continue
        paths = [bytes(text, "utf-8").decode("unicode_escape", "replace") for text in QUOTED.findall(args)]
        if name == "execve":
            if errno is None and paths:
                actor[pid] = Path(paths[0]).name
            paths = paths[:1]
        elif name in ("bind", "connect"):
            sun = re.search(r'sun_path=@?"((?:[^"\\]|\\.)*)"', args)
            if not sun:
                continue
            paths = [sun.group(1)]
        elif name in ("rename", "renameat", "renameat2", "link", "linkat", "symlink", "symlinkat"):
            paths = paths[:2]
        else:
            paths = paths[:1]
        write = name in WRITE_CALLS or bool(re.search(r"O_(WRONLY|RDWR|CREAT|TRUNC|APPEND)", args))
        for path in paths:
            yield actor.get(pid, "?"), name, path, errno or "ok", write


def report_trace(run_dir: Path, given):
    """What the trace shows outside the directories a session is given."""
    rows = list(trace_rows(run_dir / "out" / "trace.txt"))
    out(f"Trace: {len(rows)} calls that name a path.")

    def where(path):
        if not path.startswith("/"):
            return "(relative)"
        for label, root in given.items():
            if path == root or path.startswith(root + "/"):
                return label
        return "/" + path.split("/")[1]

    def collapse(path):
        # Names that differ from run to run and trees that hold many files.
        path = re.sub(r"^/etc/ssl/certs/.+", "/etc/ssl/certs/*", path)
        path = re.sub(r"^/usr/share/zoneinfo/.+", "/usr/share/zoneinfo/*", path)
        return path

    by_place = Counter(where(path) for _, _, path, _, _ in rows)
    out("Calls by place: " + ", ".join(f"{place} {count}" for place, count in by_place.most_common()))

    baseline = {"/usr", "/lib", "/lib64", "/bin", "/sbin", "/proc", "/sys", "/dev", "(relative)", *given}
    seen = {}
    for actor, name, path, outcome, write in rows:
        place = where(path)
        if place in baseline:
            continue
        key = (place, collapse(path))
        entry = seen.setdefault(key, {"actors": set(), "calls": set(), "outcomes": set(), "write": False})
        entry["actors"].add(actor)
        entry["calls"].add(name)
        entry["outcomes"].add(outcome)
        entry["write"] |= write and outcome == "ok"

    def show(title, keys, limit=150):
        out(f"\n{title}: {len(keys)}")
        for key in keys[:limit]:
            entry = seen[key]
            out(f"    {key[1]}  [{','.join(sorted(entry['outcomes']))}]  {','.join(sorted(entry['calls']))}  by {','.join(sorted(entry['actors']))}")
        if len(keys) > limit:
            out(f"    ... and {len(keys) - limit} more")

    keys = sorted(seen)
    show("Written, created or removed outside the working directory, the config directory and TMPDIR", [key for key in keys if seen[key]["write"]])
    show("Looked up under /etc, and not written", [key for key in keys if key[0] == "/etc" and not seen[key]["write"]])
    show("Looked up elsewhere outside what the session is given, and not written", [key for key in keys if key[0] != "/etc" and not seen[key]["write"]])
    named = sorted({(collapse(path), outcome) for _, _, path, outcome, _ in rows if re.search(r"managed|claude-code|ClaudeCode", path)})
    out(f"\nEvery path that holds `managed`, `claude-code` or `ClaudeCode`, wherever it lies: {len(named)}")
    for path, outcome in named[:80]:
        out(f"    {path}  [{outcome}]")


# --- one case -----------------------------------------------------------------------


def run_case(name, claude: Path, scratch: Path, token, model, timeout):
    kind, tmp_mode, tmpdir_mode, managed, prompt, traced = CASES[name]
    run_dir = scratch / name
    for sub in ("work", "home/.claude", "host", "tmpdir", "tmp", "out", "bin", "empty"):
        (run_dir / sub).mkdir(parents=True)
    # The CLI takes a directory for its socket only if no directory on the way to it can
    # be written by the group or by others without the sticky bit.
    run_dir.chmod(0o700)
    (run_dir / "tmpdir").chmod(tmpdir_mode)
    (run_dir / "tmp").chmod(0o1777)  # as a machine's /tmp
    if kind == "none":
        prefix = []
        inside = {sub: str(run_dir / sub) for sub in ("work", "home", "host", "tmpdir", "out", "bin")}
        (run_dir / "bin" / "claude").symlink_to(claude)
    else:
        prefix = standin(run_dir, claude, tmp_mode, managed == "hidden")
        inside = {sub: f"/sf/{sub}" for sub in ("work", "home", "host", "tmpdir", "out", "bin")}

    (run_dir / "host" / "settings.json").write_text(json.dumps({"sandbox": {"enabled": False}}, indent=2) + "\n")  # AG-8
    (run_dir / "work" / "inside.sh").write_text(INSIDE_SH)

    tools = "Bash,Agent" if prompt is PROMPT_SUBAGENT else "Bash"
    allowed = [f"Bash({COMMAND})"] + (["Agent"] if prompt is PROMPT_SUBAGENT else [])
    # The flags of AG-1 in `subscription` mode (AG-3a) that a session with its prompt on
    # the command line takes. The two flags that take a list come first, so that neither
    # takes the prompt (P-21).
    cmd = [f"{inside['bin']}/claude", "-p", "--tools", tools, "--allowedTools", *allowed]
    cmd += ["--setting-sources", "user", "--strict-mcp-config"]
    cmd += ["--output-format", "stream-json", "--verbose", "--model", model, "--session-id", str(uuid.uuid4())]
    cmd += ["--permission-mode", "dontAsk", "--permission-prompts", "none"]
    cmd += ["--settings", f"{inside['host']}/settings.json", "--max-turns", "8", "--max-budget-usd", "0.50"]
    cmd += ["--forward-subagent-text", "--debug-file", f"{inside['out']}/debug.log", prompt]
    if traced:
        calls = "%file,bind,connect,clone,clone3,fork,vfork"
        cmd = ["strace", "-f", "-s", "400", "-e", f"trace={calls}", "-o", f"{inside['out']}/trace.txt"] + cmd
    env = {  # AG-11, with the variables P-6 set
        "PATH": f"{inside['bin']}:/usr/local/bin:/usr/bin:/bin",
        "HOME": inside["home"],
        "TMPDIR": inside["tmpdir"],
        "CLAUDE_CONFIG_DIR": f"{inside['home']}/.claude",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
        "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS": "1",
        "DISABLE_AUTOUPDATER": "1",
        "LANG": "C.UTF-8",
    }
    if token:
        env["CLAUDE_CODE_OAUTH_TOKEN"] = token

    host_tmp_before = top_level(Path("/tmp")) if kind == "none" else set()
    started = time.monotonic()
    timed_out = threading.Event()
    with open(run_dir / "out" / "events.jsonl", "wb") as raw, open(run_dir / "out" / "stderr.txt", "wb") as stderr:
        proc = subprocess.Popen(
            prefix + cmd, cwd=run_dir / "work", env=env, stdin=subprocess.DEVNULL, stdout=raw, stderr=stderr, start_new_session=True
        )

        def kill():
            timed_out.set()
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

        timer = threading.Timer(timeout, kill)
        timer.start()
        exit_code = proc.wait()
        timer.cancel()
    wall = time.monotonic() - started

    events = events_of(run_dir)
    out(f"\n## Case {name}\n")
    out(f"Outer sandbox: {kind}{'' if tmp_mode is None else ', /tmp bound ' + ('read-write' if tmp_mode == 'rw' else 'read-only')}"
        f"{'' if managed is None else ', /etc/claude-code ' + managed}. TMPDIR={inside['tmpdir']}, mode {tmpdir_mode:04o}")
    out(f"Command: {' '.join(arg if arg != prompt else '<prompt>' for arg in prefix + cmd)}")
    out(f"Exit code {exit_code}{', killed at the timeout' if timed_out.is_set() else ''}, after {wall:.0f}s.")

    init = next((event for event in events if event.get("type") == "system" and event.get("subtype") == "init"), None)
    out(f"init: {'version ' + str(init.get('claude_code_version')) + ', tools ' + str(init.get('tools')) if init else 'no system/init message'}")
    named = {}
    for event in events:
        label = f"{event.get('type')}/{event.get('subtype')}" if event.get("subtype") else str(event.get("type"))
        for trail, value in path_strings(event):
            if value.startswith(inside["work"]) and "tmp" not in trail.lower():
                continue
            if label in ("assistant", "user") and not re.search(r"tmp|sock|output_file", trail + value, re.I):
                continue
            named.setdefault((label, trail, value), None)
    out(f"Paths that the stream names, outside the working directory: {len(named)}")
    for label, trail, value in list(named)[:60]:
        out(f"    {label} {trail} = {value}")

    sub_calls = sum(1 for event in events if event.get("type") == "assistant" and event.get("parent_tool_use_id"))
    for event in events:
        for block in blocks([event], "assistant", "tool_use"):
            who = "subagent" if event.get("parent_tool_use_id") else "main loop"
            out(f"tool_use ({who}) {block.get('name')}: {json.dumps(block.get('input'))[:400]}")
        for block in blocks([event], "user", "tool_result"):
            out(f"tool_result{' (is_error)' if block.get('is_error') else ''}:")
            out(indent(text_of(block.get("content", ""))[:1800]))
        if event.get("type") == "result":
            out(f"result: subtype {event.get('subtype')}, is_error {event.get('is_error')}, turns {event.get('num_turns')}, "
                f"cost {event.get('total_cost_usd')}, denials {json.dumps(event.get('permission_denials'))}")
            out(indent(str(event.get("result", ""))[:400]))
        elif event.get("type") == "unparsed":
            out(f"unparsed stdout line: {event['text']}")
    out(f"Model calls of a subagent on the stream: {sub_calls}")

    observed = read(run_dir / "work" / "observed.txt")
    out(f"observed.txt, which inside.sh appended to in the working directory ({observed.count('--- run')} runs):")
    out(indent(observed or "(absent)"))

    if kind == "none":
        new = sorted(top_level(Path("/tmp")) - host_tmp_before)
        out(f"New in the machine's /tmp after the session: {', '.join(new) or 'nothing'}")
        for entry in new:
            path = Path("/tmp") / entry
            if path.is_dir() and not path.is_symlink():
                out(indent(listing(path), f"    {entry}/ "))
    else:
        out("The directory bound at /tmp, after the session:")
        out(indent(listing(run_dir / "tmp")))
    out("The directory that TMPDIR names, after the session:")
    out(indent(listing(run_dir / "tmpdir")))
    out("The home directory of the session, which holds the config directory, after the session:")
    out(indent(listing(run_dir / "home", 60)))

    stderr = read(run_dir / "out" / "stderr.txt").strip()
    if stderr:
        out("stderr, last lines:")
        out(indent("\n".join(line[:400] for line in stderr.splitlines()[-12:])))
    words = re.compile(r"managed|/tmp|tmpdir|EACCES|EROFS|read-only|socket|policy", re.I)
    debug = []
    for line in read(run_dir / "out" / "debug.log").splitlines():
        line = re.sub(r"^\S+T\S+Z?\s+", "", line)[:700]
        if words.search(line) and line not in debug:
            debug.append(line)
    out(f"Debug-log lines that mention a managed location, a temporary directory, a socket or a refused write: {len(debug)}{', the first 60:' if len(debug) > 60 else ''}")
    if debug:
        out(indent("\n".join(debug[:60])))

    if traced:
        given = {"work": inside["work"], "config": inside["home"], "TMPDIR": inside["tmpdir"], "out": inside["out"],
                 "host": inside["host"], "bin": inside["bin"], "cli": str(claude.parent.parent)}
        report_trace(run_dir, given)

    if token:
        needle = token.encode()
        holding = [str(path.relative_to(run_dir)) for path in run_dir.rglob("*") if path.is_file() and not path.is_symlink() and needle in path.read_bytes()]
        if holding:
            out(f"THE TOKEN LITERAL IS IN: {holding} (removed with the scratch directory)")

    result = next((event for event in events if event.get("type") == "result"), {})
    runs = [line for line in observed.splitlines() if line.startswith("TMPDIR=")]
    return {
        "case": name,
        "exit": "timeout" if timed_out.is_set() else exit_code,
        "result": result.get("subtype", "none"),
        "sub": sub_calls,
        "runs": observed.count("inside.sh ran to its end"),
        "managed": ", ".join(sorted({line.split("SF_P36_MANAGED=")[1] for line in runs})) or "-",
        "cost": result.get("total_cost_usd") or 0,
    }


def plant_managed():
    """Plants the managed settings of the two `managed` cases. Returns whether it did."""
    if MANAGED_DIR.exists():
        out(f"{MANAGED_DIR} exists already: the two `managed` cases are skipped, and nothing there is touched.")
        return False
    body = json.dumps({"env": {"SF_P36_MANAGED": MANAGED_VALUE}})
    code, text = run(["sudo", "-n", "sh", "-c", f"mkdir {MANAGED_DIR} && printf '%s\\n' '{body}' >{MANAGED_DIR}/managed-settings.json"])
    if code != 0:
        out(f"Could not plant {MANAGED_DIR}/managed-settings.json ({text}): the two `managed` cases are skipped.")
        return False
    out(f"Planted {MANAGED_DIR}/managed-settings.json: {body}")
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("-k", metavar="TEXT", help="only the cases whose name contains TEXT")
    parser.add_argument("--list", action="store_true", help="list the cases and stop")
    parser.add_argument("--model", default="haiku")
    parser.add_argument("--timeout", type=int, default=300, help="seconds per session")
    parser.add_argument("--keep", action="store_true", help="keep the scratch directory")
    parser.add_argument("--claude", metavar="PATH", help="the CLI's native binary")
    parser.add_argument("--token-stdin", action="store_true", help="read the token from standard input")
    parser.add_argument("--no-login", action="store_true", help="run without a token: the sessions fail, and what the CLI did before shows")
    args = parser.parse_args()

    names = [name for name in CASES if not args.k or args.k in name]
    if args.list:
        for name in names:
            kind, tmp_mode, tmpdir_mode, managed, prompt, traced = CASES[name]
            print(f"{name:22} outer {kind:9} /tmp {tmp_mode or 'as it is':9} TMPDIR {tmpdir_mode:04o} managed {managed or '-':8} "
                  f"{'subagent' if prompt is PROMPT_SUBAGENT else 'bash':9}{' strace' if traced else ''}")
        return 0

    REDACT.add(str(HOME), "~")
    REDACT.add(HOME.name, "<user>")
    REDACT.add(HOME.name.split(".")[0], "<user>")
    REDACT.add(socket.gethostname(), "<host>")
    out("# Probe P-36, part 3 (issue #85): where the agent CLI reads and writes on Linux\n")
    if os.getuid() == 0:
        out("This is root. Run it again as an ordinary user.")
        return 2
    found = args.claude or shutil.which("claude", path=SEARCH_PATH)
    missing = [tool for tool in ("bwrap", "strace") if not shutil.which(tool)]
    if not found or missing:
        out(f"claude: {found or 'not found'}; missing: {missing}. All three are needed.")
        return 2
    claude = Path(found).resolve()

    token = None
    if args.token_stdin:
        token = sys.stdin.read().strip()
    elif not args.no_login:
        token = os.environ.get("CLAUDE_CODE_OAUTH_TOKEN", "").strip()
    if not token and not args.no_login:
        out("No token: pass --token-stdin or set CLAUDE_CODE_OAUTH_TOKEN, or pass --no-login.")
        return 2
    REDACT.token = token

    # In the home directory and not in ~/.cache, which the group may write on Debian: the
    # CLI then refuses every directory below it for its socket.
    scratch = Path(tempfile.mkdtemp(prefix="sf-p36-", dir=HOME))
    REDACT.add(str(scratch), "<scratch>")
    os.chdir(scratch)
    planted = False
    try:
        out(f"Collected {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} as uid {os.getuid()}.")
        out(f"{run(['uname', '-srm'])[1]}; {run(['bwrap', '--version'])[1]}; {run(['strace', '-V'])[1].splitlines()[0]}")
        out(f"claude: {run([str(claude), '--version'], env={'HOME': str(scratch), 'PATH': '/usr/bin:/bin'})[1]} ({claude})")
        out(f"Token: {'none (--no-login)' if not token else 'given'}. Model {args.model}. At most {args.timeout}s a session.")
        out(f"{MANAGED_DIR} before the cases: {'present' if MANAGED_DIR.exists() else 'absent'}")
        rows = []
        for name in names:
            if CASES[name][3] is not None and not planted:
                planted = plant_managed()
                if not planted:
                    continue
            rows.append(run_case(name, claude, scratch, token, args.model, args.timeout))
    finally:
        if planted:
            code, text = run(["sudo", "-n", "rm", "-r", str(MANAGED_DIR)])
            out(f"\nRemoved {MANAGED_DIR}: {'yes' if code == 0 else text}")
        if args.keep:
            print(f"Kept: {scratch}", file=sys.stderr)
        else:
            shutil.rmtree(scratch, ignore_errors=True)

    out("\n## Summary\n")
    out("| Case | Exit | Result | Subagent calls on the stream | Runs of inside.sh | SF_P36_MANAGED in them |")
    out("|---|---|---|---|---|---|")
    for row in rows:
        out(f"| {row['case']} | {row['exit']} | {row['result']} | {row['sub']} | {row['runs']} | {row['managed']} |")
    out(f"\nSum of total_cost_usd: {sum(row['cost'] for row in rows):.4f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
