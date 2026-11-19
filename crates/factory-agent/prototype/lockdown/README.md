# PROTOTYPE: live lock-down and subscription probe of the Claude Code CLI

Throwaway code for issue #11 and probes P-21 and P-22 (SPEC 10.3 AG-3a, 13.4
SEC-11, 14). It is not the contract test of SPEC 19.4 and is never merged; the
answer it produced is in `docs/research/probes.md` on `main`.

`lockdown_probe.py` runs one real `claude -p` session per case, without
`--bare`, on a `claude setup-token` credential. Each case gets a fresh working
directory with repository-controlled configuration planted in it (`CLAUDE.md`
and its relatives, `AGENTS.md`, `.claude/settings.json` and
`.claude/settings.local.json` with hooks, an `env` block and permission rules,
a skill, a command, a subagent, a `.mcp.json`), its own `HOME` and
`CLAUDE_CONFIG_DIR` with user-level configuration, and a host-written
`--settings` and `--mcp-config` file. It records what took effect:

- hooks and MCP servers leave a canary file when they run;
- skills, commands, subagents, plugins and MCP servers are read from the
  `system/init` message, and from the later `init` and `commands_changed`
  messages for anything found after the start;
- instruction files each hold a canary word, and the model is asked to list
  the words it can see. This is the one part that depends on the model, so
  every "not loaded" has a control case in which the same file is listed.

Every session runs behind a CONNECT proxy in the harness that logs each host
the CLI asks for and can refuse hosts. A watcher samples the sockets of the
CLI's process tree for a connection that does not go to the proxy.

```sh
python3 lockdown_probe.py --list
python3 lockdown_probe.py --out <scratch dir>              # every case
python3 lockdown_probe.py --out <scratch dir> -k ag3a      # the cases named *ag3a*
```

The token is read from `~/.config/sf-probe/oauth-token` (mode 0600, made by
`claude setup-token`). It is passed in the environment only. After each
session every file the session left is searched for the token's literal
value, and a summary that contains it is not written.

Each session is one or two short turns on the `haiku` alias. The whole matrix
reported about 0.04 API-equivalent dollars per pass (the sum of `total_cost_usd`).

`results/` holds the pass the probe record was written from: one summary per
case, `overview.md`, and the raw stream-JSON of nine cases
(`*.events.jsonl`). The scratch directory appears in them as `<out>`. The
harness adds `--debug-file` to every command line; the selected debug-log
lines in each summary come from it.

Limits: one CLI version (2.1.311), on macOS, without the factory's sandbox.
The proxy sees host names, not request paths. No session was driven into a
plan limit.
