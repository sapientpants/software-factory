# PROTOTYPE: the agent CLI's Bash sandbox under bubblewrap

Throwaway code for issue #12 and probe P-6 (SPEC 10.3 AG-8, 13.3 SEC-8). Not for merging. The answer goes in `docs/research/probes.md` on `main` once the script has run on the server.

`inner_sandbox_probe.py` runs one real `claude -p` session per case, on a `claude setup-token` credential, with the flags of AG-1 in `subscription` mode and Bash as the only tool. The session is asked to run `sh ./inside.sh`, and the permission rule allows that command and no other. `inside.sh` prints its namespaces, its seccomp state, the network interfaces it sees, whether it can write in the working directory, in a second directory and in `/tmp`, and whether it reaches `api.anthropic.com`. It appends the same text to a file in the working directory, so the record does not depend on what the model reports. The harness records the namespaces the CLI itself started in and compares.

A case is an outer sandbox and a `sandbox` block in the host's `--settings` file:

| Outer sandbox | |
|---|---|
| `none` | The CLI runs as the account, with its own `HOME` |
| `bwrap` | A stand-in for level `process` (13.3): user, PID, IPC, UTS and cgroup namespaces, explicit mounts, a private `/proc` and `/tmp`. The network namespace is shared, because the proxy of SEC-10 does not exist and the CLI has to reach the model endpoint. No seccomp filter |
| `nouserns` | The same with `--disable-userns`: nothing inside can create a user namespace. This stands for any outer sandbox that forbids nested namespaces, which a seccomp filter may also do |

```sh
python3 inner_sandbox_probe.py --list
python3 inner_sandbox_probe.py                  # every case, on this machine
python3 inner_sandbox_probe.py -k nouserns      # the cases named *nouserns*
ssh <server> python3 - < inner_sandbox_probe.py | tee results/server-<date>.txt
```

It needs `bwrap`, `python3`, `curl` and a native `claude` binary, and runs as an ordinary user. The CLI's sandbox also needs `socat`; without it the cases that turn the sandbox on show what a missing dependency does, which is a different question. The token is read from `CLAUDE_CODE_OAUTH_TOKEN` or from `~/.config/sf-probe/oauth-token` (mode 0600) of the account that runs the script, and is passed in the environment only. The output replaces the home directory, the account's name, the host name and the token's literal value, should one appear.

What a run leaves behind: nothing of its own. It works in `~/.cache/sf-p6-*` and writes one empty file in `/tmp`, and removes both. In the two `none` cases the CLI itself makes `/tmp/claude-<uid>` and `/tmp/cc-socks`, as any session of that account does.

Thirteen sessions of one or two turns on the `haiku` alias: a pass reported 0.01 API-equivalent dollars.

## Results

| File | Run |
|---|---|
| `results/container-2026-11-20.txt` | A Debian 13 container on the Mac (arm64, Docker Desktop's 7.0.14-linuxkit kernel), built from the `Dockerfile` here and started with `--privileged`, as an unprivileged user. Claude Code 2.1.311, bubblewrap 0.12.0, socat 1.8.0.3 |

The container run is a rehearsal of the script, not the answer to P-6: the probe asks about the server.

```sh
docker build -t sf-p6-dryrun .
CLAUDE_CODE_OAUTH_TOKEN="$(cat ~/.config/sf-probe/oauth-token)" \
  docker run --rm --privileged -e CLAUDE_CODE_OAUTH_TOKEN \
  -v "$PWD/inner_sandbox_probe.py:/probe.py:ro" sf-p6-dryrun python3 /probe.py
```

What it showed there:

- **With no `sandbox` block, or with `enabled: false`, the Bash tool runs in the CLI's own namespaces** under both outer sandboxes. The CLI starts no sandbox of its own.
- **With `enabled: true` under the `bwrap` stand-in, the CLI's sandbox works nested.** The command runs in a new user, mount, PID and network namespace, with two seccomp filters, cannot write outside the working directory and is refused `api.anthropic.com` by the CLI's proxy. `enableWeakerNestedSandbox` was not needed.
- **With `enabled: true` under `nouserns`, every Bash command fails:** `bwrap: Creating new namespace failed`. The session still ends with `success` and exit code 0. `failIfUnavailable` does not stop the CLI at startup, and `enableWeakerNestedSandbox` does not help.
- **Unless `allowUnsandboxedCommands` is `false`, the model may run the command again outside the sandbox.** It did in two passes of three, with `dangerouslyDisableSandbox: true` in the tool call, and the command then ran in the CLI's namespaces.

## What the container run does not show

- **The server.** Another kernel, another architecture, and no Docker around the outer sandbox.
- **The factory's sandbox.** The stand-in shares the network namespace and has no seccomp filter and no proxy.
- **A microVM**, which P-6 also asks about. That is M2 work (D-5).
- **Another model or a longer session.** Whether a model retries outside the sandbox is the model's choice; one short prompt on `haiku` was tried.
