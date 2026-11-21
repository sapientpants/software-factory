# PROTOTYPE: turns, budget and subagents on a headless Claude Code stream

Throwaway code for issue #47 and probe P-26 (SPEC 14, BUD-6 and BUD-10). It is
not the contract test of SPEC 19.4 and is never merged; the answer it produced
is in `docs/research/probes.md` on `main`.

`turns_probe.py` runs one real `claude -p` session per case, on the command
line of AG-1 in `subscription` mode: every flag AG-1 names, no `--bare`, a
`claude setup-token` credential in the environment. Each case gets a fresh
working directory (a git repository with a `README.md` and two chains of files,
each file naming the next), its own `HOME` and an empty `CLAUDE_CONFIG_DIR`,
and host-written `--settings`, `--mcp-config` and system prompt files.

Two questions:

1. With more than one message on a stream-JSON input, does `--max-turns` start
   over with each message, and does the budget total run on?
2. In a session that starts a subagent, which lines are the subagent's model
   calls, what token counts do they carry, do the totals in `result` include
   them, and do the two caps count them?

```sh
python3 turns_probe.py --list
python3 turns_probe.py --out <scratch dir> --results results/pass-N          # every case
python3 turns_probe.py --out <scratch dir> --results results/pass-N -k NAME  # one case, by its exact name
```

The token is read from `~/.config/sf-probe/oauth-token` (mode 0600, made by
`claude setup-token`). It is passed in the environment only. After each session
every file the session left is searched for the token's literal value, and
nothing of a session that holds it is written to the results.

Every session runs on the `haiku` alias; in one case the subagent runs on
`sonnet`. A whole pass reported about 0.12 API-equivalent dollars (the sum of
the last `total_cost_usd` of each session), and the 56 kept sessions 0.29.

## Results

Per case and pass, three files:

- `<case>.json`: the command line, the messages, counts, every `result`, and
  one row per stream line that says something about calls, turns or spend;
- `<case>.stream.txt`: the same rows, one per line, with the harness's own
  notes (`_probe`) in between: when a message was sent, whether the process
  was alive 3 s after a result, when standard input was closed;
- `<case>.events.jsonl`: the raw stream. The scratch directory appears as
  `<out>`.

`_t` is seconds since the process was started, taken when the harness read the
line.

- `results/pass-1`: every case, with the harness as committed.
- `results/pass-2`, `results/pass-3`: the five cases in which a budget cap is
  reached while a subagent runs, again. What the stream shows there differs
  from run to run. `pass-2` also holds a second run of the two
  `subagent-as-specified` cases: the environment and the flag that SPEC AG-11
  and AG-1 name after this probe, together.
- Added after a review of the record, and counted apart in it:
  `subagent-main-loop-max-turns`, in all three passes, in which the main loop
  reaches `--max-turns` while a background subagent runs; and
  `bash-background` and `bash-background-off`, in `pass-1`, which ask the Bash
  tool for a background command without and with
  `CLAUDE_CODE_DISABLE_BACKGROUND_TASKS=1`.
- `results/first-runs`: the 17 sessions run while the cases were being added,
  with the same chains and budgets. They are counted with the passes. Three
  earlier sessions, with a chain of six files and a read loop that slept, were
  not kept.
- `results/sums.txt`: the output of `sums.py` over all four directories.

## How a case is driven

The harness sends the first message, and after each `result` waits 3 s, notes
whether the process is alive, and sends the next message or closes standard
input. In the cases that have the Agent tool it first waits until the stream
has listed no background task for 5 s (`system/background_tasks_changed`), or
until another `result` arrives: a background subagent makes the CLI emit a
`result` before the work is done. `subagent-input-closed` does not wait.

The budgets of the cap cases were set from the costs the earlier sessions
reported, so that the cap falls inside the subagent's calls. They depend on
whether the prompt cache is warm; `--budget` overrides them.

## Limits

One CLI version (2.1.323), on macOS, without the factory's sandbox. The
subagent is the built-in `general-purpose` one, started by the model because
the message asks for it; whether it runs in the foreground is the model's
choice unless the message or `CLAUDE_CODE_DISABLE_BACKGROUND_TASKS` settles
it. No subagent started a subagent. The plan limit was not reached, and usage
credits are off on the account.
