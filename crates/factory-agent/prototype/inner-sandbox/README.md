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
```

On the server the operator becomes the factory's user with `sudo`, which asks for a password, so the script is copied there and started from a login. That user has no CLI and no token of its own. It is given a copy of the binary to read, and the token on standard input, which the operator's shell opens:

```sh
# On the Mac
scp inner_sandbox_probe.py <server>:/tmp/
# On the server, as the operator
install -m 755 "$(readlink -f ~/.local/bin/claude)" /tmp/claude-p6
sudo -H -u sf python3 /tmp/inner_sandbox_probe.py --claude /tmp/claude-p6 --token-stdin \
  < ~/.config/sf-probe/oauth-token | tee /tmp/p6-server.txt
rm /tmp/claude-p6 /tmp/inner_sandbox_probe.py
# On the Mac
scp <server>:/tmp/p6-server.txt results/server-<date>.txt
```

It needs `bwrap`, `python3`, `curl` and a native `claude` binary, and runs as an ordinary user. The CLI's sandbox also needs `socat`; without it the cases that turn the sandbox on show what a missing dependency does, which is a different question. The token is read from standard input with `--token-stdin`, from `CLAUDE_CODE_OAUTH_TOKEN`, or from `~/.config/sf-probe/oauth-token` (mode 0600) of the account that runs the script, and is passed to the sessions in the environment only. The output replaces the home directory, the account's name, the host name and the token's literal value, should one appear.

What a run leaves behind: nothing of its own. It works in `~/.cache/sf-p6-*`, or in `/var/tmp/sf-p6-*` when the user has no home directory to write in, and writes one empty file in `/tmp`, and removes both. In the two `none` cases the CLI itself makes `/tmp/claude-<uid>` and `/tmp/cc-socks` or `/tmp/cc-socks-<uid>`, as any session of that user does.

Thirteen sessions of one or two turns on the `haiku` alias: a pass reported 0.01 API-equivalent dollars.

## Results

| File | Run |
|---|---|
| `results/server-2026-11-20.txt` | The server, at 18:35 UTC, started by the operator from a login as `sudo -H -u sf python3 /tmp/inner_sandbox_probe.py --claude /tmp/claude-p6 --token-stdin`, so as the factory's user, with a copy of the operator's Claude Code 2.1.311 binary. Debian 13, x86-64, bubblewrap 0.12.0, **no `socat`**. The script at commit `cbe7a0c` |
| `results/container-2026-11-20.txt` | A Debian 13 container on the Mac (arm64, Docker Desktop's 7.0.14-linuxkit kernel), built from the `Dockerfile` here and started with `--privileged`. The script ran the way it is to run on the server: as a system user named `sf` with no home directory, started from a directory that user cannot read, with a copy of the binary in `/tmp` and the token on standard input. Claude Code 2.1.311, bubblewrap 0.12.0, socat 1.8.0.3. The script at commit `cbe7a0c` |

In the server's file the names of 13 network interfaces are replaced by `<13 names redacted>`, on 12 lines. The repository is public and the operator chose to name nothing that identifies the server. Trailing spaces are dropped in both files. Since these runs the script prints how many interfaces there are besides `lo`, and no names.

P-6 asks about the server. The server has no `socat`, which the CLI's sandbox needs, so the container run is the only one in which that sandbox started.

## What the server run showed

- **With no `sandbox` block, or with `enabled: false`, the Bash tool runs in the CLI's own namespaces** under both outer sandboxes and without one. The command wrote in all three places and reached `api.anthropic.com`. Nothing on stderr.
- **With `enabled: true` the CLI started no sandbox, in any of the seven cases.** It wrote to stderr `Sandbox disabled: sandbox is enabled but dependencies are missing: socat not installed` and `Commands will run WITHOUT sandboxing`, and the command ran as if the setting were absent. `allowUnsandboxedCommands: false` and `enableWeakerNestedSandbox` changed nothing. The session ended with `success` and exit code 0, and the stream says nothing of it: the `system/init` message has no field that mentions a sandbox.
- **With `failIfUnavailable: true` as well, the CLI did not start:** exit code 1 after one second, no `system/init` message, a `result` of subtype `error_during_execution`, and on stderr `sandbox required but unavailable` and `sandbox.failIfUnavailable is set`.
- **bubblewrap nests in the stand-in on the server** and does not under `--disable-userns`: the preflight lines `nested`.

## What the container run showed

The container run was a rehearsal of the script. It differs from the server in having `socat`.

The container has no `sudo`, so `runuser` stood in for it:

```sh
docker build -t sf-p6-dryrun .
CLAUDE_CODE_OAUTH_TOKEN="$(cat ~/.config/sf-probe/oauth-token)" \
  docker run --rm --privileged -u 0 -e CLAUDE_CODE_OAUTH_TOKEN \
  -v "$PWD/inner_sandbox_probe.py:/tmp/inner_sandbox_probe.py:ro" sf-p6-dryrun sh -c '
    useradd -r sf && install -m 755 "$(readlink -f /home/probe/.local/bin/claude)" /tmp/claude-p6 &&
    chmod 700 /home/probe && cd /home/probe &&
    printenv CLAUDE_CODE_OAUTH_TOKEN | runuser -u sf -- env -u CLAUDE_CODE_OAUTH_TOKEN HOME=/home/sf \
      python3 /tmp/inner_sandbox_probe.py --claude /tmp/claude-p6 --token-stdin'
```

- **With no `sandbox` block, or with `enabled: false`, the Bash tool runs in the CLI's own namespaces** under both outer sandboxes. The CLI starts no sandbox of its own.
- **With `enabled: true` under the `bwrap` stand-in, the CLI's sandbox works nested.** The command runs in a new user, mount, PID and network namespace, with two seccomp filters, cannot write outside the working directory and is refused `api.anthropic.com` by the CLI's proxy. `enableWeakerNestedSandbox` was not needed.
- **With `enabled: true` under `nouserns`, every Bash command fails:** `bwrap: Creating new namespace failed`. The session still ends with `success` and exit code 0. `failIfUnavailable` does not stop the CLI at startup, and `enableWeakerNestedSandbox` does not help.
- **Unless `allowUnsandboxedCommands` is `false`, the model may run the command again outside the sandbox.** It did in three passes of four, of which the kept output is the last, with `dangerouslyDisableSandbox: true` in the tool call, and the command then ran in the CLI's namespaces.

## What neither run shows

- **The CLI's sandbox starting on the server.** That needs `socat` there. The container has another kernel, another architecture, and Docker around the outer sandbox.
- **The factory's sandbox.** The stand-in shares the network namespace and has no seccomp filter and no proxy.
- **A microVM**, which P-6 also asks about. That is M2 work (D-5).
- **Another model or a longer session.** Whether a model retries outside the sandbox is the model's choice; one short prompt on `haiku` was tried.
