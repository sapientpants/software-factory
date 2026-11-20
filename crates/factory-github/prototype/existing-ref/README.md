# Probe P-1: creating a ref that already exists

Throwaway scripts and their output for probe P-1 of SPEC 23.2 (#13). Not for merging. The record is the P-1 section of `docs/research/probes.md`; this directory is the evidence behind it.

The scripts write to `sapientpants/sf-scratch` with the `gh` login of whoever runs them. They create branch refs under `refs/heads/factory/` and leave them there; `results/deleted-refs.txt` lists the 43 that the run of 2026-11-20 created and that were deleted by hand afterwards.

| File | What it sends |
|---|---|
| `probe.sh <dir> [issue-number] [api-version]` | 17 single creates (the first, the repeats of it and the faulty ones), 3 reads, and 10 pairs of creates sent by two `gh api` processes at once |
| `race.sh <dir>` | 20 pairs of creates sent by one `curl --parallel` process, and 4 reads |
| `collide.sh <dir>` | A branch `factory`, then the create and the read of `factory/issue-9001` under it. Typed by hand on the day; the file holds the commands |

## Results

| Directory | Run |
|---|---|
| `results/2026-03-10/` | `probe.sh <dir> 9001 2026-03-10` |
| `results/2022-11-28/` | `probe.sh <dir> 8001 2022-11-28` |
| `results/pairs-curl/` | `race.sh <dir>` |
| `results/branch-named-factory/` | The commands of `collide.sh` |

One file per request. `a*` are the creates against one ref, `b*` the faulty creates, `c*` the reads, `r<i>-x` and `r<i>-y` the two creates of pair `i` with `r<i>-after` the read that followed. In `pairs-curl`, `t<i>-status.txt` holds the status and timings of both transfers and `t<i>-x.json`, `t<i>-y.json` their bodies.

What is not as the server sent it:

- **Headers are cut down** to the status line, `Date`, `X-GitHub-Api-Version-Selected` and `X-GitHub-Request-Id`. The rest named the scopes of the operator's token and its rate limit. Bodies are whole. A line starting `gh:` after a body is `gh`'s own error text.
- **The headers of the `curl` pairs were never kept**, only status, timings and body.
- **The listing that showed each ref of a `curl` pair at the winner's SHA was not saved.** `race.sh` does not make it; it was a `GET /git/matching-refs/heads/factory/issue-70` after the run.
- **`race.sh` printed `created` for both requests of every pair on the console.** Its `grep` for the message does not match `curl`'s pretty-printed JSON. The saved bodies are right.
