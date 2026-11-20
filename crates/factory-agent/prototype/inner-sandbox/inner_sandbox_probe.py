#!/usr/bin/env python3
"""PROTOTYPE: does Claude Code's own Bash sandbox work under bubblewrap?

Throwaway code for issue #12 and probe P-6 (SPEC 10.3 AG-8, 13.3 SEC-8). Never merged.

Each case is one real `claude -p` session on a `claude setup-token` credential, with the
flags of AG-1 in `subscription` mode. The session is asked to run one shell script with
the Bash tool. The script writes what it can see and do into the working directory, so
the record does not depend on what the model says. A case is the pair of:

- an outer sandbox: none, a bubblewrap stand-in for level `process` (13.3), or the same
  stand-in with `--disable-userns`, in which nothing can create a user namespace;
- the `sandbox` block of the host's `--settings` file.

Run it on the server as an ordinary user, without sudo:

    python3 inner_sandbox_probe.py | tee results.txt
    ssh <server> python3 - < inner_sandbox_probe.py | tee results.txt

The token is read from the environment (CLAUDE_CODE_OAUTH_TOKEN) or from
~/.config/sf-probe/oauth-token, and is passed to the sessions in the environment only.
The script writes under ~/.cache/sf-p6-* and one empty file in /tmp, and removes both when
it ends. Each session may run one command, `sh ./inside.sh`, and nothing else. It
prints no host name, no account name and no address.
"""

import argparse
import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from pathlib import Path

HOME = Path.home()
TOKEN_FILE = HOME / ".config" / "sf-probe" / "oauth-token"
SEARCH_PATH = f"{os.environ.get('PATH', '/usr/bin:/bin')}:{HOME}/.local/bin"

COMMAND = "sh ./inside.sh"
PROMPT = f"Run the shell command `{COMMAND}` with the Bash tool, then tell me in one word whether it ran."
TMP_CANARY = Path("/tmp/sf-p6-canary")

ON = {"enabled": True}
STRICT = {"enabled": True, "allowUnsandboxedCommands": False}

# (name, outer sandbox, the `sandbox` block of --settings or None for no block)
CASES = [
    ("none-default", "none", None),
    ("none-on-strict", "none", STRICT),
    ("bwrap-default", "bwrap", None),
    ("bwrap-off", "bwrap", {"enabled": False}),
    ("bwrap-on", "bwrap", ON),
    ("bwrap-on-strict", "bwrap", STRICT),
    ("bwrap-on-weaker", "bwrap", {**STRICT, "enableWeakerNestedSandbox": True}),
    ("nouserns-default", "nouserns", None),
    ("nouserns-off", "nouserns", {"enabled": False}),
    ("nouserns-on", "nouserns", ON),
    ("nouserns-on-strict", "nouserns", STRICT),
    ("nouserns-on-weaker", "nouserns", {**STRICT, "enableWeakerNestedSandbox": True}),
    ("nouserns-on-fail", "nouserns", {**STRICT, "failIfUnavailable": True}),
]

NAMESPACES = ("user", "mnt", "pid", "net", "ipc")

# Becomes the CLI after recording the namespaces the CLI starts in.
START_SH = """#!/bin/sh
for ns in user mnt pid net ipc; do
  printf '%s %s\\n' "$ns" "$(readlink /proc/self/ns/$ns)"
done >"$SF_OUT/claude-ns.txt"
exec claude "$@"
"""

# Run by the CLI's Bash tool. @OUTSIDE@ is a directory that the outer sandbox leaves
# writable and that is not the working directory.
INSIDE_SH = """#!/bin/sh
{
  echo "--- run pid $$"
  for ns in user mnt pid net ipc; do echo "ns $ns $(readlink /proc/self/ns/$ns)"; done
  grep -E '^(NoNewPrivs|Seccomp|Seccomp_filters):' /proc/self/status | tr -s '\\t' ' ' | sed 's/^/status /'
  echo "processes visible: $(ls /proc | grep -c '^[0-9]')"
  echo "interfaces: $(sed -n '3,$s/:.*//p' /proc/net/dev | tr -s ' \\n' ' ')"
  echo "proxy variables: $(env | grep -i -E '^(https?|all|no)_proxy=' | sed 's|//[^@]*@|//<credentials>@|' | sort | tr '\\n' ' ')"
  for target in ./canary-cwd @OUTSIDE@/canary-outside @TMP_CANARY@; do
    if (: >"$target") 2>/dev/null; then echo "write $target: ok"; else echo "write $target: denied"; fi
  done
  echo "https://api.anthropic.com: $(curl -sS -o /dev/null -w '%{http_code}' --max-time 15 https://api.anthropic.com 2>&1 | tr '\\n' ' ')"
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
    """A short command: (exit code, output), or (None, reason) if it could not start."""
    try:
        done = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60, **kwargs)
    except (OSError, subprocess.TimeoutExpired) as error:
        return None, f"{type(error).__name__}: {error}"
    return done.returncode, done.stdout.decode("utf-8", "replace").strip()


def first_line(cmd, containing=""):
    code, text = run(cmd)
    if code != 0:
        return f"failed ({text.splitlines()[-1][:120] if text else code})"
    return next((line for line in text.splitlines() if containing in line), "")[:80]


def outer_command(kind, run_dir: Path, claude: Path):
    """The argv prefix that puts a command in the outer sandbox, and the paths as seen inside."""
    if kind == "none":
        return [], {name: run_dir / name for name in ("work", "home", "host", "outside", "out", "bin")}
    args = ["bwrap", "--unshare-user", "--unshare-pid", "--unshare-ipc", "--unshare-uts", "--unshare-cgroup-try"]
    # The network namespace is shared: the proxy of SEC-10 does not exist yet, and the
    # CLI has to reach the model endpoint.
    if kind == "nouserns":
        args += ["--disable-userns"]
    args += ["--die-with-parent", "--new-session", "--hostname", "sandbox"]
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
    args += ["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]
    args += ["--ro-bind", str(claude), "/sf/bin/claude", "--ro-bind", str(run_dir / "host"), "/sf/host"]
    for name in ("work", "home", "outside", "out"):
        args += ["--bind", str(run_dir / name), f"/sf/{name}"]
    args += ["--chdir", "/sf/work", "--"]
    return args, {name: Path("/sf") / name for name in ("work", "home", "host", "outside", "out", "bin")}


def preflight(claude: Path, scratch: Path):
    out("## Preflight\n")
    out(f"Collected {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} as uid {os.getuid()}.")
    pretty = [line for line in Path("/etc/os-release").read_text().splitlines() if line.startswith("PRETTY_NAME=")]
    out(pretty[0] if pretty else "/etc/os-release: no PRETTY_NAME")
    out(f"uname -srm: {first_line(['uname', '-srm'])}")
    out(f"python3: {sys.version.split()[0]}")
    for tool, flag, containing in (("bwrap", "--version", ""), ("socat", "-V", "socat version"), ("curl", "--version", "")):
        path = shutil.which(tool)
        out(f"{tool}: {first_line([tool, flag], containing) + ' (' + path + ')' if path else 'not installed'}")
    out(f"claude: {first_line([str(claude), '--version'])} ({claude})")
    limit = Path("/proc/sys/user/max_user_namespaces")
    out(f"user.max_user_namespaces: {limit.read_text().strip() if limit.exists() else 'absent'}")

    out("\nThe outer sandboxes without the CLI (expected: nested ok under bwrap, fail under nouserns):\n")
    run_dir = scratch / "preflight"
    for name in ("work", "home", "host", "outside", "out"):
        (run_dir / name).mkdir(parents=True)
    inner = ["bwrap", "--unshare-user", "--unshare-pid", "--ro-bind", "/", "/", "--proc", "/proc", "--dev", "/dev", "--", "true"]
    for kind in ("bwrap", "nouserns"):
        prefix, _ = outer_command(kind, run_dir, claude)
        for label, cmd in (("starts", ["true"]), ("nested", inner), ("claude --version", ["/sf/bin/claude", "--version"])):
            code, text = run(prefix + cmd)
            detail = "ok" if code == 0 else f"fail (exit {code}): {text.splitlines()[-1] if text else ''}"
            out(f"{kind:9} {label:17} {detail}")


def run_case(name, kind, sandbox_block, claude: Path, scratch: Path, token, model, timeout):
    run_dir = scratch / name
    for sub in ("work", "home/.claude", "host", "outside", "out", "bin"):
        (run_dir / sub).mkdir(parents=True)
    prefix, inside = outer_command(kind, run_dir, claude)
    if kind == "none":
        (run_dir / "bin" / "claude").symlink_to(claude)

    settings = {} if sandbox_block is None else {"sandbox": sandbox_block}
    (run_dir / "host" / "settings.json").write_text(json.dumps(settings, indent=2) + "\n")
    (run_dir / "host" / "start.sh").write_text(START_SH)
    (run_dir / "work" / "inside.sh").write_text(INSIDE_SH.replace("@OUTSIDE@", str(inside["outside"])).replace("@TMP_CANARY@", str(TMP_CANARY)))

    # The flags of AG-1 in `subscription` mode (AG-3a), with Bash as the only tool and
    # the one command as the only one allowed: in the cases without a sandbox the tool
    # runs as this account. The two flags that take a list come first, so that neither
    # takes the prompt.
    cmd = prefix + ["/bin/sh", str(inside["host"] / "start.sh"), "-p"]
    cmd += ["--tools", "Bash", "--allowedTools", f"Bash({COMMAND})"]
    cmd += ["--setting-sources", "user", "--strict-mcp-config"]
    cmd += ["--output-format", "stream-json", "--verbose", "--model", model, "--session-id", str(uuid.uuid4())]
    cmd += ["--permission-mode", "dontAsk", "--permission-prompts", "none"]
    cmd += ["--settings", str(inside["host"] / "settings.json"), "--max-turns", "4"]
    cmd += ["--debug-file", str(inside["out"] / "debug.log"), PROMPT]
    env = {
        "PATH": f"{inside['bin']}:/usr/local/bin:/usr/bin:/bin",
        "HOME": str(inside["home"]),
        "CLAUDE_CONFIG_DIR": str(inside["home"] / ".claude"),
        "CLAUDE_CODE_OAUTH_TOKEN": token,
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
        "DISABLE_AUTOUPDATER": "1",
        "LANG": "C.UTF-8",
        "SF_OUT": str(inside["out"]),
    }

    started = time.monotonic()
    timed_out = threading.Event()
    with open(run_dir / "out" / "events.jsonl", "wb") as raw, open(run_dir / "out" / "stderr.txt", "wb") as stderr:
        proc = subprocess.Popen(
            cmd, cwd=run_dir / "work", env=env, stdin=subprocess.DEVNULL, stdout=raw, stderr=stderr, start_new_session=True
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

    report(name, kind, settings, cmd, run_dir, exit_code, timed_out.is_set(), wall, token)
    return summarise(name, kind, sandbox_block, run_dir, exit_code, timed_out.is_set())


def read(path: Path):
    return path.read_text(errors="replace") if path.exists() else ""


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


def observed_runs(run_dir: Path):
    """One dict per run of inside.sh: its namespaces and the other lines it wrote."""
    runs = []
    for line in read(run_dir / "work" / "observed.txt").splitlines():
        if line.startswith("--- run"):
            runs.append({"ns": {}, "lines": []})
        elif runs and line.startswith("ns "):
            _, ns, value = line.split(" ", 2)
            runs[-1]["ns"][ns] = value
        elif runs:
            runs[-1]["lines"].append(line)
    return runs


def verdict(run_dir: Path, observed):
    """Whether a run of inside.sh was in the CLI's own namespaces."""
    cli = dict(line.split(" ", 1) for line in read(run_dir / "out" / "claude-ns.txt").splitlines() if " " in line)
    if not cli:
        return "the CLI's namespaces were not recorded"
    new = [ns for ns in NAMESPACES if observed["ns"].get(ns) != cli.get(ns)]
    outside = next((line for line in observed["lines"] if "canary-outside" in line), "")
    wrote = "wrote" if outside.endswith("ok") else "could not write"
    if not new:
        return f"NOT sandboxed by the CLI: every namespace is the CLI's own; {wrote} outside the working directory"
    return f"sandboxed by the CLI: new {', '.join(new)} namespace; {wrote} outside the working directory"


def report(name, kind, settings, cmd, run_dir: Path, exit_code, timed_out, wall, token):
    events = events_of(run_dir)
    out(f"\n## Case {name}\n")
    out(f"Outer sandbox: {kind}. --settings: {json.dumps(settings)}")
    shown = [arg if arg != PROMPT else "<prompt>" for arg in cmd]
    out(f"Command: {' '.join(shown)}")
    out(f"Exit code {exit_code}{', killed at the timeout' if timed_out else ''}, after {wall:.0f}s.")

    init = next((event for event in events if event.get("type") == "system" and event.get("subtype") == "init"), None)
    if init:
        mentions = {key: value for key, value in init.items() if "sandbox" in json.dumps({key: value}).lower()}
        out(f"init: version {init.get('claude_code_version')}, tools {init.get('tools')}, fields that mention a sandbox: {json.dumps(mentions) if mentions else 'none'}")
    else:
        out("init: no system/init message")

    for event in events:  # in the order of the stream
        for block in blocks([event], "assistant", "tool_use"):
            out(f"tool_use {block.get('name')}: {json.dumps(block.get('input'))}")
        for block in blocks([event], "user", "tool_result"):
            out(f"tool_result{' (is_error)' if block.get('is_error') else ''}:")
            out(indent(text_of(block.get("content", ""))[:2500]))
        if event.get("type") == "result":
            out(f"result: subtype {event.get('subtype')}, is_error {event.get('is_error')}, turns {event.get('num_turns')}, "
                f"cost {event.get('total_cost_usd')}, denials {json.dumps(event.get('permission_denials'))}")
            out(indent(str(event.get("result", ""))[:400]))
        elif event.get("type") == "unparsed":
            out(f"unparsed stdout line: {event['text']}")

    out(f"The CLI's namespaces: {' '.join(read(run_dir / 'out' / 'claude-ns.txt').split()) or 'not recorded'}")
    observed = read(run_dir / "work" / "observed.txt")
    results = [text_of(block.get("content", "")) for block in blocks(events, "user", "tool_result")]
    if not observed:
        out("observed.txt: absent, inside.sh left no record")
    elif any(observed.strip() in result for result in results):
        out("observed.txt, which inside.sh wrote in the working directory: the same text as the tool_result above")
    else:
        out("observed.txt, which inside.sh wrote in the working directory:")
        out(indent(observed))
    for one in observed_runs(run_dir):
        out(f"Verdict: {verdict(run_dir, one)}")

    stderr = read(run_dir / "out" / "stderr.txt").strip()
    if stderr:
        out("stderr, last lines:")
        out(indent("\n".join(line[:400] for line in stderr.splitlines()[-12:])))
    words = ("sandbox", "bwrap", "bubblewrap", "socat", "seccomp")
    debug = [line[:400] for line in read(run_dir / "out" / "debug.log").splitlines() if any(word in line.lower() for word in words) and "[uds-messaging]" not in line]
    out(f"Debug-log lines that mention the sandbox: {len(debug)}{', the first 40:' if len(debug) > 40 else ''}")
    if debug:
        out(indent("\n".join(debug[:40])))

    needle = token.encode()
    holding = [str(path.relative_to(run_dir)) for path in run_dir.rglob("*") if path.is_file() and not path.is_symlink() and needle in path.read_bytes()]
    if holding:
        out(f"THE TOKEN LITERAL IS IN: {holding} (removed with the scratch directory)")


def summarise(name, kind, sandbox_block, run_dir: Path, exit_code, timed_out):
    events = events_of(run_dir)
    calls = list(blocks(events, "assistant", "tool_use"))
    unsandboxed = sum(1 for call in calls if (call.get("input") or {}).get("dangerouslyDisableSandbox"))
    result = next((event for event in events if event.get("type") == "result"), {})
    runs = observed_runs(run_dir)
    return {
        "case": name,
        "settings": "-" if sandbox_block is None else json.dumps(sandbox_block, separators=(",", ":")),
        "exit": "timeout" if timed_out else exit_code,
        "result": result.get("subtype", "none"),
        "calls": f"{len(calls)} ({unsandboxed} asking to skip the sandbox)",
        "runs": "; ".join(verdict(run_dir, one).split(":")[0] for one in runs) or "inside.sh did not run",
        "cost": result.get("total_cost_usd") or 0,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("-k", metavar="TEXT", help="only the cases whose name contains TEXT")
    parser.add_argument("--list", action="store_true", help="list the cases and stop")
    parser.add_argument("--model", default="haiku")
    parser.add_argument("--timeout", type=int, default=240, help="seconds per session")
    parser.add_argument("--keep", action="store_true", help="keep the scratch directory")
    args = parser.parse_args()

    cases = [case for case in CASES if not args.k or args.k in case[0]]
    if args.list:
        for name, kind, block in cases:
            print(f"{name:20} outer {kind:9} sandbox {json.dumps(block)}")
        return 0

    REDACT.add(str(HOME), "~")
    REDACT.add(HOME.name, "<user>")
    REDACT.add(socket.gethostname(), "<host>")
    out("# Probe P-6 (issue #12): the agent CLI's Bash sandbox under bubblewrap\n")
    if os.getuid() == 0:
        out("This is root. Run it again as an ordinary user.")
        return 2

    found = shutil.which("claude", path=SEARCH_PATH)
    if not found or not shutil.which("bwrap"):
        out(f"claude: {found or 'not found'}; bwrap: {shutil.which('bwrap') or 'not found'}. Both are needed.")
        return 2
    claude = Path(found).resolve()
    if claude.read_bytes()[:4] != b"\x7fELF":
        out(f"{claude} is not a native binary, so it cannot be bound into the sandbox as one file.")
        return 2

    token = os.environ.get("CLAUDE_CODE_OAUTH_TOKEN", "").strip()
    source = "the environment"
    if not token and TOKEN_FILE.exists():
        token, source = TOKEN_FILE.read_text().strip(), str(TOKEN_FILE)
    if not token:
        out(f"No token: set CLAUDE_CODE_OAUTH_TOKEN or write the output of `claude setup-token` to {TOKEN_FILE} (mode 0600).")
        return 2
    REDACT.token = token

    cache = Path(os.environ.get("XDG_CACHE_HOME") or HOME / ".cache")
    cache.mkdir(parents=True, exist_ok=True)
    # Not under /tmp: the CLI's sandbox leaves the temporary directory writable, and the
    # write test needs a directory that is neither that nor the working directory.
    scratch = Path(tempfile.mkdtemp(prefix="sf-p6-", dir=cache))
    REDACT.add(str(scratch), "<scratch>")
    try:
        preflight(claude, scratch)
        out(f"\nToken from {source}. Model {args.model}. {len(cases)} sessions, at most {args.timeout}s each.")
        rows = [run_case(name, kind, block, claude, scratch, token, args.model, args.timeout) for name, kind, block in cases]
    finally:
        TMP_CANARY.unlink(missing_ok=True)  # written by the cases that run outside any sandbox
        if args.keep:
            print(f"Kept: {scratch}", file=sys.stderr)
        else:
            shutil.rmtree(scratch, ignore_errors=True)

    out("\n## Summary\n")
    out("| Case | `sandbox` in --settings | Exit | Result | Bash calls | Each run of inside.sh |")
    out("|---|---|---|---|---|---|")
    for row in rows:
        out(f"| {row['case']} | `{row['settings']}` | {row['exit']} | {row['result']} | {row['calls']} | {row['runs']} |")
    out(f"\nSum of total_cost_usd: {sum(row['cost'] for row in rows):.4f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
