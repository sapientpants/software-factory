# Probe P-8: who edited an issue, and when

Throwaway scripts and their output for probe P-8 of SPEC 23.2 (#15). Not for merging. The record is the P-8 section of `docs/research/probes.md`; this directory is the evidence behind it.

The scripts write to `sapientpants/sf-scratch` with the `gh` login of whoever runs them. They create issues and leave them there, and `bot.sh` pushes a workflow to the branch `probe/p-8` of that repository. They are `bash` scripts: `lib.sh` has a variable named `path`, which `zsh` ties to `PATH`.

| File | What it does |
|---|---|
| `lib.sh` | The REST and GraphQL calls, and `observe`, which reads an issue three ways: one GraphQL query, `GET /issues/{n}` and `GET /issues/{n}/events` |
| `api.sh <dir>` | Creates an issue and edits it in 18 steps through REST and GraphQL, with a read after each. `V=<version>` sets the REST API version |
| `order.sh <dir>` | Creates an issue, then sends a label and an edit back to back, in both orders, at once, and in one call: 35 trials, with a read after each |
| `probe-p8.yml` | The workflow that edits two issues as `github-actions[bot]`, and reads one of them with its own token |
| `bot.sh <dir> start` | Creates the operator's issue, the branch and the workflow file, which starts the run |
| `bot.sh <dir> finish` | After the run: reads both issues, edits both as the operator, reads again |
| `web.sh <dir> create`, `web.sh <dir> read <step>` | Creates the issue for the edits in the web interface, and reads it after each of them |

## Results

| Directory | Run | Issue on `sf-scratch` |
|---|---|---|
| `results/api-2026-03-10/` | `api.sh <dir>` | #5 |
| `results/api-2022-11-28/` | `V=2022-11-28 api.sh <dir>`; its console output is `results/api-2022-11-28.console.txt` | #7 |
| `results/order/` | `order.sh <dir>`; `results/order.summary.tsv` is one line per trial, made from the reads afterwards | #6 |
| `results/bot/` | `bot.sh <dir> start`, the workflow run 44809990832, `bot.sh <dir> finish` | #8 (the operator's), #9 (the workflow's) |
| `results/web/` | `web.sh <dir> create`, then five edits in Chrome with a `web.sh <dir> read` after each, and a sixth read after a revision was deleted | #10 |

A file `<step>.txt` is the response to a write. `<step>.read-graphql.json`, `<step>.read-rest-issue.json` and `<step>.read-rest-events.json` are the three reads that followed it.

What is not as the server sent it, and what went wrong:

- **Headers are cut down** to the status line, `Date`, `X-GitHub-Api-Version-Selected` and `X-GitHub-Request-Id`. The rest named the scopes of the operator's token and its rate limit. Bodies of writes are whole. The two REST reads are cut down by `jq` to the fields `observe` names.
- **The event reads of `results/api-2026-03-10/` have no `id`.** `observe` was given the field after that run.
- **The event reads of `results/order/` stop at 30 events.** `GET /issues/{n}/events` returns the oldest 30 unless asked for more, and `observe` does not page. `zz-all-events.read-rest-events.json` is all 87, fetched with `--paginate` after the run. The GraphQL reads ask for the first 100 timeline items and are whole.
- **`results/bot/b30-target-actor-ids.*` were typed by hand** after `bot.sh finish`, to get the numeric ids of the actors. No script makes them.
- **The edits in the web interface were made by a session driving Chrome**, signed in as the operator. `results/web/00-inputs.txt` has the time of each read; the time of each edit is in the read that follows it.
- **What the pages showed was not saved.** The session read two things off the page and kept no screenshot: on #8, "Last edited by sapientpants" with a menu that lists `sapientpants`, `github-actions` and `sapientpants` again; on #10, after the deletion, the revision marked "Deleted" with "sapientpants deleted this revision". Both can be seen again on the issues.
- **Issue #11 on the scratch repository is a mistake.** A stray `api.sh /dev/null` created it and stopped. It was never edited, and it is closed with a comment that says so.
- **`results/order/o81-gh-issue-edit.gh.txt`** is the output of `gh issue edit`, which is the issue's URL.
