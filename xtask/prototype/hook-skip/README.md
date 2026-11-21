# Probe P-27: skipping a git hook

Throwaway code for probe P-27 of `SPEC.md` 23.2 (#21). The answer is recorded in `docs/research/probes.md` on `main`.

- `probe.sh` is the git half. It builds two repositories in a temporary directory: one whose hooks all fail, to see which command lines skip them, and one whose pre-commit hook prints what git handed it. Run it with `sh probe.sh`.
- `output.txt` is its output on git 2.56.1 on macOS, on 2026-11-21. Paths under the temporary directory are printed as `<work>/`.

The other half of the probe, what a `PreToolUse` hook of Claude Code did with such commands in a live session, was given to the session by hand. Its commands and results are in the probe record.
