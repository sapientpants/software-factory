# Probe P-15: encodings that neutralise mentions, references and closing keywords

Throwaway scripts and their output for probe P-15 of SPEC 23.2 (#16). Not for merging. The record is the P-15 section of `docs/research/probes.md`; this directory is the evidence behind it.

The scripts write to `sapientpants/sf-scratch` with the `gh` login of whoever runs them. They create issues, pull requests and branches and leave them there, they push a workflow to the branch `probe/p-15` of that repository, and `land.sh` puts 41 commits on its `main`: the workflow's 13 empty ones, and for each of the 14 pull requests its one empty commit and a merge commit. The workflow's writes are those of `github-actions[bot]`, so a mention of the operator in them is a mention by another account, and the operator is notified. The shell scripts are `bash` scripts.

| File | What it does |
|---|---|
| `lib.sh` | The REST call that keeps its response, and the names of the encodings |
| `gen.py <dir>` | Writes the text of every case to `<dir>/payload/`, from the issue numbers in `<dir>/ids.tsv` |
| `setup.sh <dir> <step>` | What the operator's token writes: the target issues, the pull requests, the operator's commits, and the push of the workflow branch that starts a run |
| `probe-p15.yml` | The workflow: one shell step that clones the branch and runs the half of `bot.sh` named in `p15/part` |
| `bot.sh <half>` | What the workflow writes as `github-actions[bot]`: issues, comments, edits, pull request bodies and commits |
| `land.sh <dir> commits\|pulls` | Fast-forwards `main` to the workflow's commits; merges the pull requests |
| `read.graphql`, `observe.sh <dir> <name>` | Reads back every issue and pull request of the probe, the commits and the operator's notifications |
| `summarise.py <dir> <name>` | Makes one line per case from a read. Its docstring explains the columns |

A case is a surface and an encoding. The surfaces are a comment, an issue body, an issue title, a pull request body, a commit subject and a commit body. The encodings are `plain` (none), `fence`, `code`, `backslash`, `zwj` (U+200D), `zwsp` (U+200B) and `entity` (a numeric character reference). `gen.py` has the exact text.

## The rounds

Each round was written after the one before had been read.

| Round | Run | What it added | On `sf-scratch` |
|---|---|---|---|
| 1 | `setup.sh targets`, `payload`, `pulls`, `workflow text`, then `workflow commits`; runs 44822844750 and 44823392368 | The seven encodings on every surface. `land.sh commits`, then `land.sh pulls` | Targets #12 to #45, pull requests #46 to #59, the workflow's issues #60 to #79, the branch `probe/p-15-commits` |
| 2 | `setup.sh targets2`, `payload`, `operator`, `workflow second`; run 44824730765 | A comment and an issue body that get the text by an edit; six other spellings of a reference; two commits by the operator on a branch | Targets #80 to #87, the workflow's issues #88 to #96, the branches `probe/p-15-variants` and `probe/p-15-operator` |
| 3 | `setup.sh targets3`, `payload`, `operator3` | The commit messages of round 1 again, committed and pushed by the operator; a commit in the bot's name that the operator pushed; two more spellings | Targets #97 and #98, the operator's issue #99, 15 more commits on `probe/p-15-operator` |
| 4 | `setup.sh targets4`, `payload`, `workflow fourth`; run 44869393490 | Character references other than the decimal one, with and without a zero-width space after the `&`; a fence that the text ends, and a longer fence around the same text | Targets #100 to #107, the workflow's issues #108 to #116 |

The workflow branch `probe/p-15` has four commits, one for each push: `78298a3` (round 1, the text), `fec3914` (round 1, the commits), `2607f85` (round 2) and `8224023` (round 4).

## Results

| Path | What it is |
|---|---|
| `results/00-inputs.txt` | The `gh` version, the API version, the commits and runs, and the times of the two landings |
| `results/ids.tsv` | The name of every target and pull request, and its number |
| `results/payload/` | The text of every case, as written to GitHub |
| `results/setup/`, `results/land/` | The response to each write made with the operator's token |
| `results/run-log-*.txt` | The logs of the four workflow runs, since Actions logs expire |
| `results/obs1-after-workflow/` | The read after the workflow's first two runs, with the commits on a branch |
| `results/obs2-after-commits-on-main/` | After `main` was fast-forwarded to those commits |
| `results/obs3-after-merges/` | After the 14 pull requests were merged |
| `results/obs4-after-second-round/`, `obs5-after-third-round/`, `obs7-after-fourth-round/` | After each later round |
| `results/obs6-next-day/` | Seven hours after round 3, before round 4: nothing had changed |

In a read, `items.json` is one GraphQL result for each issue and pull request, `timelines-rest.json` is `GET /issues/{n}/timeline` for each, `commits.json` lists the workflow's 13 commits, `commits-html.json` has the commit messages as GitHub renders them, and `notifications.json` is `GET /repos/{owner}/{repo}/notifications?all=true` with the operator's token. `summary.tsv`, `summary-second.tsv` and `summary-fourth.tsv` are made from those by `summarise.py`.

What is not as the server sent it, and what went wrong:

- **Headers are cut down** to the status line, `Date`, `X-GitHub-Api-Version-Selected` and `X-GitHub-Request-Id`. The rest named the scopes of the operator's token and its rate limit. Bodies of writes are whole. The reads are cut down by `jq` to the fields `observe.sh` names.
- **The first run made no commits.** `bot.sh` gave `git commit-tree` its message with `-F` and a path relative to `p15/`, and `commit-tree` opens that path from the top of the work tree. The run went on and ended green; its log has the 13 errors. `bot.sh` was split into halves and the second push ran the commits alone. The `bot.sh` at `78298a3` on the scratch repository is that first version; the one here is the last.
- **The first read was taken twice.** The first attempt came before the commits existed and was overwritten.
- **`main` was fast-forwarded by hand.** The first attempt was typed into `zsh`, which read `$HEAD:refs` as a modifier, and pushed nothing. `land.sh commits` was written afterwards from the command that worked.
- **`setup.sh operator` ran twice.** The first run gave `git -C` a relative path for the message file, made no commit, and pushed `probe/p-15-operator` at the head of `main`. The second added the two commits.
- **`obs2` has a `commits-html.json` that no script made.** The query was typed by hand and then moved into `observe.sh`. It lacks the `branch` field, and `obs1` has no such file, so the summaries of `obs1` show no rendering for commits.
- **The `gh` binary changed under the probe.** Rounds 1 to 3 and the reads up to `obs6` ran on `gh` 2.104.0. An upgrade to 2.105.0 replaced the binary while `obs7` was being read, 40 calls failed, and that read was deleted and made again on 2.105.0.
- **The summaries were made at the end,** by the last version of `summarise.py`, from the reads as they were taken.
- **Nothing was cleaned up.** Issues and pull requests #12 to #116 stay on the scratch repository, 9 of the issues closed by the probe and the 14 pull requests merged. So do the branches `probe/p-15`, `probe/p-15-commits`, `probe/p-15-variants`, `probe/p-15-operator` and the 14 `probe/p-15-prc-*` and `probe/p-15-pre-*`, and the 41 commits on `main`. The operator's inbox has 13 notifications from the probe.
