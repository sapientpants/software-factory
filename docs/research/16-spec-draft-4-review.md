# Review of SPEC.md draft 4

One reviewer per lens read draft 4 of `SPEC.md` as it stood at commit 9c55344 (2026-11-22), limited to what milestones M0, M1a and M1b are built from. The lenses are consistency, evidence (the spec against `probes.md` and the reports), security and buildability (whether the three milestones can be built from the text without a further decision). The review was started on 2026-11-22 and finished on 2026-11-25, on the same commit. Every `SPEC.md:<line>` below is a line of that commit.

This is a record of findings. Nothing here is addressed yet, and no finding changed the spec.

## How far to trust it

- **No sceptic judged the findings.** The review of draft 1 (`09-spec-review.md`) gave every finding to a second reader who tried to refute it. This one did not: each reviewer was told to try to refute its own findings before reporting them, and says under **Confidence** what it did.
- **The quotations were checked by script.** Each quoted string was compared with the line it cites, by the reviewer and again when this file was put together. No misquotation was found.
- **Some findings rest on a reviewer's own knowledge of a tool** and on no probe of this repository: that `cargo deny check` fails without its advisory database (C1, B6), which file names clippy, rustup, nextest and mise read (S3), what GitHub collapses in a diff (S10) and where the CLI's web search runs (S12). Each says so.
- **No reviewer read everything.** Each lens ends with what it read in full, what it sampled and what it did not read.

## The findings in brief

51 findings: 1 critical, 24 major, 26 minor.

| Lens | Critical | Major | Minor |
|---|---:|---:|---:|
| Consistency | 0 | 3 | 7 |
| Evidence | 0 | 4 | 10 |
| Security | 0 | 6 | 6 |
| Buildability | 1 | 11 | 3 |

The mechanical check of references found no miss: all 245 hyphenated ids and G1 to G6 are defined once, and every id and section number cited in `SPEC.md`, `GLOSSARY.md` and `AGENTS.md` resolves. The evidence reviewer found no gate or invariant that rests on something the probe record contradicts. The security reviewer found no sequence that breaks an invariant of 3.1 in M1 as the text stands.

The one critical finding is B1: M0's exit criterion asks for a cold build and one agent turn on the server under bubblewrap, and no M0 item produces a command that runs them.

Points that more than one lens reached on its own:

- **`cargo deny check` in an implementer's sandbox** (C1, B6). D-28 decided that `cargo xtask check` passes there; the advisory database is shown to verify alone.
- **What M1a records, and what `sf run` does at a failure or a wait** before rounds, the outbox and the poll loop exist (B7, B8, B9, C8).
- **Tests that guard the first real sessions arrive after them**: the contract test and the escape tests at level `process` (C7, S8, E1).
- **The readiness lint, the issue parser and SEC-2 before M3** (C3, B10, S4).
- **What the probe suite and `sf doctor` are in M0** (B5, E4).
- **The secret rule and the literal-token scan** (B12, S7).
- **The bubblewrap backend where the sandbox and the proxy meet** (B2, E5, E7).

| Id | Severity | Milestone | Finding |
|---|---|---|---|
| C1 | major | M1b | `cargo deny check` is a step of the check command and cannot pass in an implementer's sandbox |
| C2 | major | M1b | DEV-9 rests on the ruleset of DEV-8, which M1b replaces with one whose admin bypass lets a session-written pull request merge without CI |
| C3 | major | M1a | 9.4 leaves skip tokens in a pull request title to the readiness lint, which nothing enforces before M3 |
| C4 | minor | M1a | Capture runs "in new sandbox instances" against one instance per stage execution |
| C5 | minor | M1a | The roles table of 10.2 gives repository instructions to the planner only |
| C6 | minor | M1a | DEP-1 requires the mode in every pull request body; VAL-14a makes it conditional |
| C7 | minor | M1a | The roadmap first names the contract test in M2; BUD-10 and DEV-7 lean on it from M1a |
| C8 | minor | M1a | The M1a store has `runs` and `events`; 23.2 fills a `probes` table in M1a |
| C9 | minor | M1a | The introduction of section 14 calls the CLI's caps a second line only, against BUD-6 and BUD-10 |
| C10 | minor | M0 | The approval-label list in `AGENTS.md` leaves out labels that SEC-1 lists |
| E1 | major | M1a, M1b | The lock-down was observed on one CLI version; a caveat of the record is dropped and AG-5's tested range has no value |
| E2 | major | M1a | Rule 6 of 5.3 reads the edit history in one GraphQL read; P-8 records that it can run over a page, and no paging rule exists |
| E3 | major | M0 | The hostile-tree suite lists case collisions, for which 9.4 has no rule and P-7 decided nothing |
| E4 | major | M0 | 23.2 says `sf doctor --probe` answers each probe on a scratch repository; six answered rows leave their installation-token half open |
| E5 | minor | M0 | `--disable-userns` was observed on bubblewrap 0.12.0; the pinned runners install 0.9.0 |
| E6 | minor | M1a | The stall deadlines carry no tunable mark and no measurement |
| E7 | minor | M0, M1a | The CLI wrote under `/tmp` outside `TMPDIR` and reads managed locations; SEC-6 accounts for neither |
| E8 | minor | M0 | That Lima with `vz` and nested virtualisation gives `/dev/kvm` is recorded nowhere |
| E9 | minor | M1a | P-15 asks for a contract test of the encoder against GitHub; 19.4 has none |
| E10 | minor | M0 | DEV-10 credits P-27 with a fact the record does not hold |
| E11 | minor | M0 | The `git` floor of 2.42 comes from a requirement that does not apply; nothing below 2.47.3 ran the suite |
| E12 | minor | M1a | The P-22 row says both caps end the session; P-26 shows they do not with stream-JSON input |
| E13 | minor | M1a | BUD-4 takes all token counts from `message_delta`; the record shows only the output count there |
| E14 | minor | M1a | VAL-5d cites a Unicode definition that P-29 read only in a summary |
| S1 | major | M1b | `CLAUDE.local.md` is in no protected list, and an unlocked session that finds one no longer reads `AGENTS.md` |
| S2 | major | M1a | A directory with the layout of a bare git repository passes capture and integrity without a flag |
| S3 | major | M1b | Check tools read a second file name that no pattern matches |
| S4 | major | M1a | SEC-2 has no enforcement point in 5.3, 8.2 or the M1a row |
| S5 | major | M1a | The export is keyed by candidate SHA and writable by verify; nothing says a restarted verify gets a new one |
| S6 | major | M1a | VAL-5a does not switch off rename detection |
| S7 | minor | M1a | The literal-token scan has no id and no stated scope, and its test is an exit criterion of M2 |
| S8 | minor | M1a | The contract test and the escape tests at level `process` are not required before the first real sessions |
| S9 | minor | M1b | The cap of 50 paths in byte order lets a worker push paths out of the flag listing |
| S10 | minor | M1b | The hidden-diff rules follow git attributes and not what GitHub collapses |
| S11 | minor | M1b | The evidence table of 11.6 cannot hold the fence that GH-5a requires |
| S12 | minor | M1a | No rule lists a role's tools, and web search goes out through the model host |
| B1 | critical | M0 | The exit check of M0 has no entry point |
| B2 | major | M0 | The in-sandbox forwarder, the proxy's protocol and the seccomp filter are named and not specified |
| B3 | major | M0 | WS-7 gives the toolchain no location, and 9.5 puts its resolution in a crate that may do no I/O |
| B4 | major | M0, M1a | The operator configuration has no key names, and the price table of BUD-3 has no home |
| B5 | major | M0 | `sf doctor`, the GitHub client and the probe suite are each specified by text that reaches into later milestones |
| B6 | major | M0, M1b | The sandbox step has no outcome on the operator's Mac; `cargo deny check` has none without network |
| B7 | major | M1a | Resume needs records that M1a's two tables do not hold |
| B8 | major | M1a | What `sf run` does when a stage fails is specified for a session limit alone |
| B9 | major | M1b | `sf run` gains the CI and human-review waits while the poll loop is M2 |
| B10 | major | M1a, M1b | The written form of an acceptance criterion is not specified, and no row builds the issue parser |
| B11 | major | M1a | `fake-github` is in no milestone row |
| B12 | major | M1a | The secret rule has no definition; three rows of the 11.3 table are placed in no milestone |
| B13 | minor | M1b | The `CODEOWNERS` comparison is given to a test in a crate that may not depend on `factory-core` |
| B14 | minor | M0 | Several pre-1.0 stack rows lack an exact pin, and the two unwrap lints bind in test code without a word on it |
| B15 | minor | M0 | The release build is one stack row |

## Since the commit that was reviewed

The decision on the factory's GitHub Apps was taken on 2026-11-22, after the reviewers had their copy. It decides that the Apps are created by hand through M1, which answers the App part of B5; that the answers of P-1, P-8 and P-25 are observed again with an installation token, with the App part of P-5, which is the second observation E4 asks for; and which probes M0's exit owes, which the table at the end of the evidence lens sets out. The rest of B5 and E4 stands.

## Lens: consistency

Lens: consistency. Scope: M0, M1a, M1b. Ten findings: none critical, three major, seven minor. Line numbers are those of `SPEC.md` at 9c55344 unless another file is named.

The mechanical check of references found no miss: see `## Covered`.

### C1 [severity: major] [milestone: M1b] §19.5 DEV-2, DEV-7, DEV-11, 21.1 BOOT-6, D-28 vs 11.2 VAL-1, 13.3 SEC-6, 13.4 SEC-11

**Problem.** D-28 has the implementer run `cargo xtask check` with every step but the sandbox steps, and has the command pass there. The step `cargo deny check` cannot pass there, because the advisory database is shown to verify alone.

- `SPEC.md:1604` (DEV-2) lists `cargo deny check` in the set.
- `SPEC.md:1624` (DEV-11): "A step is not run only where the table of DEV-7 says that it does not run in that place, which it says of the sandbox steps alone (DEV-2a)." And: "In every other place a step that cannot run fails by name".
- `SPEC.md:1616` (DEV-7, worker column): "When the implementer runs the check (AG-21): every step but the sandbox step at level `process`, which is not run (SEC-6b, DEV-11)".
- `SPEC.md:1765` (D-28): "the other steps run, and the command exits with 0 and names in its last line what was not run", and "Failing by name in a worker was not taken: the command, which a session is told must pass, could not pass there."
- `SPEC.md:1704` (BOOT-6) counts on it: "a failure of a check outside the list, `typos` or `cargo deny` for example, shows first in CI, as a CI round and not a verify round (7.3), unless the implementer ran that check itself."
- `SPEC.md:908` (VAL-1): "The host fetches the RustSec advisory database daily (never a worker) and exposes it read-only to verify; `cargo deny` in verify runs without fetching."
- `SPEC.md:1260` (SEC-6, read-only row): "the host-fetched advisory database (verify only)".
- `SPEC.md:1272` (SEC-11): "`github.com` and `api.github.com` are never on a worker's list."

So in an implementer's sandbox `cargo deny check` has no database and no way to fetch one. By DEV-11 it then fails by name, and `cargo xtask check` exits non-zero in every implementer session on the factory repository, which is the outcome D-28 set out to avoid. The same holds for the blocking item of onboarding at `SPEC.md:1633`, "a single-command setup and test entry point that passes on the default branch under the sandbox", when it is tried in an implementer's sandbox.

On the factory repository the database is also mounted for the one stage that does not use it: the verify list of BOOT-6 (`SPEC.md:1696`, "The four commands of 11.2: formatting, clippy, nextest and the doc tests") holds no `cargo deny`. Two older sentences still name `cargo deny` in verify as the control without that exception: `SPEC.md:1038` (VAL-5g, "`cargo deny` in verify checks the versions (VAL-1)") and `SPEC.md:1199` (13.1). On the factory repository it runs in CI.

**Proposed fix.** D-28 should win, since it is the later decision and the rest of BOOT-6 is built on it. In SEC-6, change "the host-fetched advisory database (verify only)" to "the host-fetched advisory database (verify and implementer sessions)", and in VAL-1 change "exposes it read-only to verify" to "exposes it read-only to verify and to an implementer session". The database is public data that the host fetched, so this opens nothing. The alternative is a second row in DEV-7 that marks the advisory part of `cargo deny check` as not run in a worker, which DEV-11 would then have to stop calling "the sandbox steps alone". In VAL-5g and 13.1, write "`cargo deny` in verify or in CI".

**Confidence.** High. I looked for any sentence that gives an implementer the database (WS-7a gives it the check tools only, `SPEC.md:690`), for a DEV-7 row that excuses the step, and for a `cargo deny` invocation limited to checks that need no database (DEV-2 says `cargo deny check`). The review of draft 2 added "verify only" (`docs/research/10-spec-recheck.md:346`); the conflict is new with D-27 and D-28. That `cargo deny check` fails without the database is my knowledge of the tool, not something I ran.

### C2 [severity: major] [milestone: M1b] §19.5 DEV-9 vs DEV-8, 20.3, D-20

**Problem.** DEV-9 names the ruleset of DEV-8 as the guarantee for every commit on `main`. DEV-8 and D-20 remove that ruleset in M1b, and the one that replaces it lets every pull request the operator wrote through.

- `SPEC.md:1622` (DEV-9): "nothing on a clone stops a human from skipping a hook. What holds for every commit that reaches `main` is the ruleset of DEV-8."
- `SPEC.md:1621` (DEV-8): "the two checks of DEV-5 are required, with GitHub Actions as their source; force pushes and deletions are blocked; there is no bypass actor", and then "In M1b, once the factory reports its check runs (5.5), the operator replaces it with the ruleset of 20.3 (step 5 of 20.1)".
- `SPEC.md:1757` (D-20): "a ruleset on `main` requires both jobs and has no bypass actor; the ruleset of 20.3 replaces it at M1b".
- `SPEC.md:1648` (20.3): "The repository-admin role is therefore a bypass actor in pull-request-only mode ... Human pull requests are merged through that bypass and are outside the factory's guarantee."
- `SPEC.md:1662` (BOOT-1): sessions build all of M1, so they keep writing pull requests after the swap.

From the swap on, a pull request that a session wrote merges through the bypass, and no rule holds it to the two CI checks. That covers the rest of M1b and every later human pull request. DEV-9's sentence is then false, and D-20's "no bypass actor" ends without a decision that says so. The person who writes the ruleset in M1b cannot tell whether the DEV-8 ruleset is deleted or kept beside the new one.

**Proposed fix.** Decide which is meant. If CI is to stay required for the operator's own pull requests, change "replaces" to "is added beside" in DEV-8, D-20 and BOOT-6 (`SPEC.md:1687`): the DEV-8 ruleset stays, with no bypass actor, and the ruleset of 20.3 carries the approval, the code-owner review and the factory checks with its bypass. 4.1 then has to say that mode detection accepts the second ruleset. If the replacement is meant, change DEV-9 to: "What holds for every commit that reaches `main` is the ruleset of DEV-8 until M1b. From then a pull request that the operator wrote merges through the bypass of 20.3, and no rule holds it to CI."

**Confidence.** Medium. The quotes are exact, and 23.3 (`SPEC.md:1833`, "their own pull requests bypass the ruleset") shows the bypass is known. I found no sentence that keeps CI required for a human pull request after the swap. Whether the operator meant to give that up is what I cannot tell; that a bypass covers the whole ruleset is my reading of GitHub, not checked here.

### C3 [severity: major] [milestone: M1a] §9.4 subject validation vs 8.2 step 1, SEC-4, 22

**Problem.** 9.4 leaves skip tokens in a pull request title to the readiness lint, and three other passages say nothing enforces that lint before M3.

- `SPEC.md:713` (9.4): "The pull request title and the squash title are generated by the host from the issue title, after the sanitisation of SEC-4, with every reference (GH-5a) removed (skip tokens are already excluded by the lint, 6.4)".
- `SPEC.md:398` (6.4) is where that rule lives: "the title contains no skip token (9.4)".
- `SPEC.md:567` (8.2): "Until the lint is enforced (M3, 22), a body over the cap of SEC-4 still ends the claim here".
- `SPEC.md:1222` (SEC-4): "Before M3 nothing enforces the lint (22)".
- `SPEC.md:1729` (22, M3): "Issue forms and readiness lint enforced".

M1a publishes a draft pull request whose title comes from the issue title, so in M1a, M1b and M2 a title such as `Fix build [skip ci]` reaches the pull request title unchecked. D-35 found the same hole for the body cap and moved that one check into the claim; the skip token was left behind. A builder of M1a's publish step reads that the case is already excluded and adds no check.

**Proposed fix.** In 9.4, replace the parenthesis with: "and every skip token of this section removed; the lint of 6.4 also refuses such a title from M3". Removing is the smaller change and matches how references are handled in the same sentence. The alternative is to end the claim on such a title before M3, as 8.2 does for the body cap.

**Confidence.** High that the premise is false before M3. I looked for an M1 or M2 rule that checks the title (8.2 step 1, SEC-4, the M1a and M1b rows) and found none. The practical harm is small, since only the operator writes titles (SEC-2).

### C4 [severity: minor] [milestone: M1a] §9.4 step 2 vs 13.3, GLOSSARY "Stage execution"

**Problem.** One capture is one stage execution with one sandbox instance in two places, and several instances in a third.

- `SPEC.md:707` (9.4): "The host's git binary runs with `quarantine/` as its repository and the workspace as its work tree, in new sandbox instances with no network and no credentials ... Everything the commands of this step share is in `quarantine/`."
- `SPEC.md:1247` (13.3): "One instance exists per stage execution".
- `GLOSSARY.md:38`: "One execution of a stage: one agent session, one capture, or one verify command sequence ... Each gets its own sandbox instance."

Draft 3 had "In a new sandbox instance"; the plural came with commit e7a8166. A builder cannot tell whether each git command of the staging step gets an instance of its own. It decides what AG-15 journals (`SPEC.md:865`, one sandbox record before the first exec) and what an activity row holds.

**Proposed fix.** If one instance is meant, write "in a new sandbox instance" at `SPEC.md:707` and drop or reword the sentence about what the commands share. If one instance per command is meant, say so in 13.3 and in the glossary.

**Confidence.** High that the texts differ; I found no sentence that says which is meant.

### C5 [severity: minor] [milestone: M1a] §10.2 vs AG-2a, 10.4

**Problem.** The roles table gives repository instructions to the planner alone; two other passages give them to the implementer and the reviewer as well.

- `SPEC.md:773` and `SPEC.md:775` (10.2, "Context given"): "Planner | ... | Snapshot, repository instructions" and "Implementer | ... | Snapshot, plan, `progress.md`, feedback from failed gates".
- `SPEC.md:786` (AG-2a): "The implementer and planner receive them through the system prompt file. The reviewer's system prompt contains only the factory's role prompt; repository instructions reach it in a delimited data block."
- `SPEC.md:809` (10.4): "then the repository instructions (AG-2a), except for the reviewer."

M1a builds the implementer and no planner, so the one role the table names is the one M1a does not have.

**Proposed fix.** AG-2a wins. Add "repository instructions" to the implementer's row, and "repository instructions (as data)" to the reviewer's.

**Confidence.** High. The table names the instructions for one role, so it cannot be read as leaving them out for all.

### C6 [severity: minor] [milestone: M1a] §4.1 DEP-1 vs 11.6 VAL-14a

**Problem.** DEP-1 requires the mode in every pull request body; VAL-14a makes it conditional for the body M1a writes.

- `SPEC.md:117` (DEP-1): "The mode MUST be shown in `sf status`, in the status comment and in every pull request body."
- `SPEC.md:1126` (VAL-14a): "the draft pull request carries a cut of this template and nothing else (D-33): item 1; of item 2 the integrity flags with their listing (VAL-3), and the enforcement mode, the risk and the tier where the run holds them".

Nothing places mode detection (4.1) in M1a, so one builder adds it to meet the MUST and another leaves the mode out as VAL-14a allows. D-33 lists the point among those the operator was not asked about (`SPEC.md:1770`).

**Proposed fix.** VAL-14a wins as the later and narrower rule. Add to DEP-1: "Before M1b a pull request body shows it only where the run holds it (VAL-14a)." Or put mode detection in the M1a row and drop "where the run holds them" for the mode.

**Confidence.** High on the texts. Medium on the effect: tier and risk are always held after 8.2 step 1, so in practice only the mode is in doubt.

### C7 [severity: minor] [milestone: M1a] §22 (M2 row) vs BUD-10, 19.4, DEV-7

**Problem.** The roadmap first names the contract test in M2, and only one part of it. Rules that M1a builds rest on other parts.

- `SPEC.md:1728` (22, M2): "lock-down contract test and plan-limit pause".
- `SPEC.md:1358` (BUD-10, built in M1a by `SPEC.md:1726`, "per-session limits (BUD-6 to BUD-10)"): "Every subagent of the probe ran in the foreground under it, and the contract test asserts that one still does (19.4)."
- `SPEC.md:1599` (19.4): the contract test covers "the stream shapes of probe P-26 that the limits rest on".
- `SPEC.md:1618` (DEV-7) treats it as a test that already runs: "When a session runs them (DEV-4)" in the development VM and "The contract test, on a schedule (19.4)" on the server.

The rows of M0, M1a and M1b do not name the contract test. The M1a builder cannot tell whether the parts for the stream shapes and for the flags (AG-1, AG-5) are due with the Claude driver or wait for M2.

**Proposed fix.** Add to the M1a row, after the Claude driver: "with the contract test of 19.4 for the flags and the stream shapes that the limits rest on". Leave "lock-down contract test" in M2 for the part of AG-3a.

**Confidence.** Medium. No sentence puts the test in M1a outright; the conflict is between the roadmap's silence and the present tense of BUD-10 and DEV-7.

### C8 [severity: minor] [milestone: M1a] §22 (M1a row) vs 23.2

**Problem.** The roadmap gives the M1a store two tables; 23.2 has a third filled in M1a.

- `SPEC.md:1726` (22, M1a): "Store with `runs` and `events`".
- `SPEC.md:1791` (23.2): "the answer is recorded in `docs/research/probes.md` and, once the store exists (M1a), imported into its `probes` table."

The M2 row adds "outbox, activities and recovery" and no row adds `probes`. The same doubt holds for the per-call records of BUD-4, which BUD-7 wants "from M1a all the same" (`SPEC.md:1352`) and 18.1 keeps in a table `spend`.

**Proposed fix.** Either write the M1a row as "Store with `runs`, `events`, `spend` and `probes`", or change 23.2 to "once the store has a `probes` table" and say in BUD-7 that M1a keeps per-call usage in `events`.

**Confidence.** High on the texts; the sentence of 23.2 is older than draft 4 and was reviewed in neither earlier review.

### C9 [severity: minor] [milestone: M1a] §14 intro vs BUD-6, BUD-10

**Problem.** The introduction of section 14 calls the CLI's caps a second line only. Two rules added in draft 4 say otherwise, one for each cap.

- `SPEC.md:1330`: "Agent-side caps are a second line only: they are client-side estimates and can be exceeded."
- `SPEC.md:1351` (BUD-6, on `--max-turns`): "So it is no second line for a session's turn limit".
- `SPEC.md:1359` (BUD-10): "it is the CLI's own cap (AG-14) that stops a session for what they spend: at the limit and not at the threshold of BUD-1."

For subagent spend the CLI's cap is the first and only line, and for turns it is no line at all. AG-14 (`SPEC.md:864`) already carries the exception ("except for what subagents spend (BUD-10)"); the introduction does not.

**Proposed fix.** BUD-6 and BUD-10 win, since they rest on probe P-26. Replace the sentence at `SPEC.md:1330` with: "The agent CLI's own caps are not relied on, with one exception: its budget cap is what stops a session for what subagents spend (BUD-10). Its turn cap covers one message and no subagent (BUD-6)."

**Confidence.** High. No builder would build differently, which is why this is minor.

### C10 [severity: minor] [milestone: M0] AGENTS.md "Labels that mean approval" vs 13.2 SEC-1, 5.2

**Problem.** `AGENTS.md` gives the operator's inputs as a closed list that is shorter than the one it cites.

- `AGENTS.md:59`: "`state:ready`, `factory:plan-approved`, `factory:hold`, `factory:restart`, `factory:stop`, `tier:*` and `risk:*` are the operator's inputs to the factory (SPEC 5.2, SEC-1). Only the operator applies or removes them."
- `SPEC.md:1209` (SEC-1): "Human inputs that change factory behaviour are: `state:ready`, `factory:hold`, `factory:restart`, `factory:plan-approved`, `factory:stop`, `tier:*`, `risk:*`, `priority:*`, `type:ci` and `type:spec-delta`."

A session that reads `AGENTS.md` alone is told nothing against applying `type:spec-delta`, `type:ci` or `priority:*` with the operator's `gh` login. SEC-1 would honour each, and `type:spec-delta` lifts the protection of `docs/specs/**` (`SPEC.md:208`, `SPEC.md:919`). D-21 keeps a rule in `AGENTS.md` exactly where a session that read nothing else would do harm.

**Proposed fix.** SEC-1 wins. In `AGENTS.md`, add `priority:*`, `type:ci` and `type:spec-delta` to the list, or write "every human input of SEC-1" in place of the list.

**Confidence.** High on the difference. The three labels do little before M2, so the harm in M0 and M1 is small.

### Consistency: covered

**Read in full:** `SPEC.md` sections 1 to 23 and both appendices, every row of 23.1 (D-1 to D-37) and of 23.2 (P-1 to P-34) included; `GLOSSARY.md`; `AGENTS.md`; the first screen of `docs/research/10-spec-recheck.md`.

**Sampled only:** `docs/research/09-spec-review.md` and the rest of `10-spec-recheck.md` (searched for earlier findings on the same points); `docs/research/probes.md` (not read); `xtask/src/check.rs`, `xtask/tests/structure.rs`, `Cargo.toml`, `mise.toml`, `rust-toolchain.toml`, `.claude/settings.json`, `.githooks/pre-commit` and `docs/agents/triage-labels.md` (looked at against DEV-1, DEV-9, DEV-10, D-17 and 19.1); the draft 3 text at 4b982b0 (searched for four passages, to see what draft 4 changed).

**Mechanical checks (by three scripts kept outside the tree):**

- Requirement ids: 245 defined in `SPEC.md`, beside the goals G1 to G6. None is defined twice. Every id cited in `SPEC.md`, `GLOSSARY.md` and `AGENTS.md` has a definition. The only unmatched tokens are not requirement ids: `AC-1` (an example criterion), `S0` and `S1` (example keys of 10.5), `D145` (a definition of the Unicode Standard), `SHA-256` and `UTF-8`.
- Section numbers: every cited number of the form `n.n` or `n.n.n`, and every "section n", is a heading. The unmatched numbers are versions and quantities.
- By hand: the cited steps, rules and items exist ("step 5 of 20.1", "rule 6 of 5.3", "item 5 of 11.6", "12.3 step 1"), and each `(D-n)` citation in the body matches what its row decides.

**Areas checked and found clean for M0 to M1b:**

- 23.1 against the sections each decision changed: D-16 to D-18, D-20 to D-35 and D-37 agree with their sections, apart from C1 and C2. D-31's "the outbound encoder is built in M2" is a record that D-33 supersedes in its own row.
- The roadmap rows of M0, M1a and M1b against the body: the encoder (GH-5a, GH-5c), VAL-14a, AG-2a and AG-2b, AG-11a, WS-7a, GH-3a, the split of VAL-5e between M1a and M1b, VAL-5f, VAL-5g, BUD-6 to BUD-10 and BOOT-6 are placed alike in both, apart from C3, C7 and C8.
- Repeated values: the session limits (D-22, section 14), the caps of SEC-4 and AG-2b, the 50 paths of VAL-3 and D-33, the round caps, the timeouts of 8.2 and 16.2, the sandbox levels (13.3, SEC-7, D-7) and the pipeline order (7.3, `AGENTS.md`).
- The path-pattern rules VAL-5b to VAL-5g against their example tables, against VAL-3a and against the tests of 19.4.
- `GLOSSARY.md`: no term is used in M0 to M1b text in a sense that differs from its definition. "Round" and "Candidate" are narrower than their M2 uses (a plan round, the update commit of 12.4), which is out of scope.
- The status row: it matches the draft's state (draft 4, unreviewed, dated as the head commit). The counts of Appendix B match the two review files (59 and 47).

**Not judged:** anything that first matters in M2 or later; whether the M1 rows are complete (what a gate failure does before rounds exist, where the issue parser and mode detection are placed), which is another lens.

## Lens: evidence

Snapshot: commit 9c55344. Scope: what M0, M1a and M1b are built from. All line numbers are at that commit. `probes.md` stands for `docs/research/probes.md`.

Overall: the answered probes are written into the spec with unusual care. In most places the spec names the version, the token and the platform an observation was made on, and where a rule goes beyond the observation it leans to the safe side (a flag too many, text removed that the page shows). No finding is critical: I found no gate or invariant that rests on something the record contradicts. The findings below are four places where a builder of M0 or M1 would have to guess, and ten dropped caveats or unmeasured numbers.

Counts: 0 critical, 4 major, 10 minor.

---

### E1 [severity: major] [milestone: M1a, M1b] §10.3 AG-3a, AG-5, AG-1; INV-10; §22 M2 row; probe P-21 (and P-26)

**Problem.** The lock-down that stands in for bare mode was observed on one CLI version, on macOS, outside a sandbox, and the record names a known leak in that version that was not tested. The spec keeps the first three caveats and drops the fourth, and it never says which version M1 runs.

- Spec, `SPEC.md:798`: "On Claude Code 2.1.311 these kept out of the session everything that probe P-21 planted in the working directory and that took effect without them" and "so the planner and the implementer load none of these files from the workspace either, and repository instructions reach a session only through AG-2a (INV-10). This was observed on one CLI version, on macOS and outside the sandbox."
- Spec, `SPEC.md:800`: "The driver checks the CLI version against a tested range at startup ... Outside the range, the daemon refuses to dispatch." No range is given anywhere in the spec (`2.1.314` has no hit in `SPEC.md`).
- Spec, `SPEC.md:784`: "Flag names are those of Claude Code 2.1.311; the last was observed on 2.1.323 (probe P-26)."
- Evidence, `probes.md:403`: "The leak that 2.1.314 fixed, an excluded settings file "still supplying some settings, such as auto mode rules and saved permission consents", did not show for a `permissions.allow` rule in `.claude/settings.local.json`. Auto mode rules were not planted."
- Evidence, `probes.md:415`: "Report 11 also advised a floor of 2.1.314 for `subscription` mode, or accepting the leak that version fixed. Neither is done here: 2.1.314 was not run, and the floor belongs to the tested range of AG-5."
- Evidence, `probes.md:416`: "Not planted: output styles, workflows, LSP configuration, a status line, hooks in the frontmatter of a skill or a subagent, auto mode rules."
- Evidence, `probes.md:1811` (P-26): "Other CLI versions.** One version, as for P-21 and P-22, and not the one those ran on."

So the decision that the record hands to AG-5 (a floor of 2.1.314, or accepting the leak) is taken nowhere. The evidence is also split over two versions: the lock-down (P-21), the hosts (P-22) and the inner sandbox (P-6) on 2.1.311, and the turn count, the subagent lines, `--forward-subagent-text` and `CLAUDE_CODE_DISABLE_BACKGROUND_TASKS` (P-26) on 2.1.323. No version has been observed with all of what AG-1, AG-3a, AG-11, BUD-6 and BUD-10 rely on. The server holds 2.1.311 (`SPEC.md:152`: "Second run: Claude Code 2.1.311 under the operator's home directory, the version P-21 and P-22 ran on").

This matters in M1 because the contract test that would close the gap is M2 work (`SPEC.md:1728`, M2 scope: "lock-down contract test and plan-limit pause"), while M1b runs real implementer sessions on the factory repository, whose `.claude/settings.json` holds real hooks (DEV-9, DEV-10). DEV-9 leans on the lock-down for exactly that (`SPEC.md:1622`: "a worker reads no project settings (AG-3a)").

**Proposed fix.** In AG-5, state the tested range as it is today and what it rests on, for example: "Through M1 the tested range is one version, named in operator configuration. It is a version on which the cases of P-21, P-22 and P-26 have all been run; 2.1.311 and 2.1.323 have each seen only part of them. The range starts at 2.1.314 or later, the version that fixed an excluded settings file still supplying auto mode rules and saved permission consents (report 11), which P-21 did not plant." Either move the lock-down part of the contract test from M2 to M1a (it is the P-21 harness run in the development VM and on the server), or say in 23.3 that M1 runs on the macOS observation alone.

**Confidence.** High that the caveat and the decision are missing: I searched `SPEC.md` for `2.1.314`, "auto mode", "tested range" and "managed" and read AG-1 to AG-8, 19.4 and 23.3. Medium on severity: M1 is attended, and what leaks would still run inside the sandbox.

---

### E2 [severity: major] [milestone: M1a] §5.3 rule 6, §13.2 SEC-1 and SEC-2, §8.2 step 1; probe P-8

**Problem.** Rule 6 says one read is enough. The probe found that the first page of the events is not the latest, that an issue can hold more edits or events than one page, and that paging was never tried. None of this is in the spec (no hit for "cursor", "per_page", "pagination", "first page" or "from the end").

- Spec, `SPEC.md:226`: "The check and the snapshot come from one GraphQL read (title, body, `lastEditedAt`, rename events, the `labeled` event), so they cannot diverge."
- Spec, `SPEC.md:1212` (SEC-2): "the issue's author and every editor of its title and body must be authorised actors" and "The editors of the body are those its edit history names, one for every edit".
- Spec, `SPEC.md:1209` (SEC-1): "each is honoured only if the most recent `labeled` event for that exact label, with no later `unlabeled` event".
- Evidence, `probes.md:1206`: "`GET /issues/{n}/events` returned the first 30 of 87, and the GraphQL query above asks for the first 100. Rule 6 needs the last rename and the last `labeled` event, so the factory's read has to ask from the end."
- Evidence, `probes.md:1192`: "An issue can hold more of either than one page of a read returns, and a read that stops at the first page has not seen every editor."
- Evidence, `probes.md:1228`: "The longest history here has 20 entries and the longest timeline 87 items. Paging through either was not tried."

A builder who follows "one GraphQL read" with the probe's own query (first 100) gets the oldest events. On an issue with a long timeline that read misses the latest `labeled` event (rule 6, SEC-1) and the later editors (SEC-2), and the miss is silent. SEC-2 needs every editor, which no single page guarantees.

**Proposed fix.** Add to rule 6: "The read asks for the events from the end: the last rename event and the last `labeled` and `unlabeled` events of the label. Probe P-8 found that both the REST list and a GraphQL read that asks for the first items return the oldest." Add to SEC-2: "The edit history and the rename events are read to their end. Where either has more entries than one page, the claim pages through it before it decides, and an issue whose history cannot be read whole is not dispatched. Paging was not observed (P-8)." Soften `SPEC.md:226` to "from one GraphQL read, and from further pages of the same connections where they have any".

**Confidence.** Medium to high. The spec does name the right events ("most recent", "last rename event"); what it drops is how they are reached and that the many-page case is unobserved. I looked for a paging rule in 5.3, 5.8 (GH-14, GH-17), 8.2 and 13.2 and found none.

---

### E3 [severity: major] [milestone: M0] §9.4 WS-9, steps 1 to 3; §22 M0 exit criterion; probe P-7

**Problem.** M0 ends when "the hostile-tree suite passes" (`SPEC.md:1725`). The suite of WS-9 lists "case collisions" (`SPEC.md:720`: "LFS pointers; case collisions; executable bits"), and WS-9 says "Probe P-7 did this for the git commands of this section". For that case the record says the commands of 9.4 do nothing, and that the prototype passed it with a check the spec does not have.

- Evidence, `probes.md:232`: "Two names that differ only in case, in one directory (Linux only) | Nothing in 9.4 | Git stages both, and the tree holds both. The prototype's ingest carried a check of its own that rejects them; the spec has no such rule (see "Still open")."
- Evidence, `probes.md:309`: "Case collisions. WS-9 lists them in the suite, and 9.4 does not say what capture does with two names that differ only in case; git accepts them." The same item ends "Not decided here."

Nothing was decided since: "collision" has one hit in `SPEC.md`, the list in WS-9. So the suite has a case with no expected outcome, and the exit criterion of M0 cannot be evaluated for it. The builder has to choose between a candidate that holds both names and a rejection. The choice interacts with VAL-5d, which treats the two names as one, and with claim and onboarding, which would have to screen a base tree that already holds a collision (`probes.md:309`).

A second case has the same standing and is smaller: `probes.md:312`, "A directory with the layout of a bare repository, under a name other than `.git`, passes capture ... Whether 11.3 should flag it is not decided here."

**Proposed fix.** Decide the case and write it into 9.4, either way. For example, in step 3: "Two entries of one directory of the new tree whose names have the same fold (VAL-5d) are rejected, unless the previous tree holds both." Or, in WS-9: "Capture admits two names that differ only in case, as git does; the suite asserts that both are in the candidate and in the export." Then change the sentence of WS-9 to "Probe P-7 did this ... except for case collisions, which its prototype rejected with a check that 9.4 does not hold".

**Confidence.** High. I read 9.1 to 9.4, 8.2 step 1 and 20.1 step 2 for a rule on case collisions and found none.

---

### E4 [severity: major] [milestone: M0] §23.2 first paragraph, §17 CLI-1, §22 M0 scope and exit criterion; probes P-1, P-5, P-8, P-15, P-25, P-32 and the record as a whole

**Problem.** 23.2 says how probes are answered, and the record shows that almost none was answered that way. Together with the exit criterion this leaves M0's "probe suite" undefined.

- Spec, `SPEC.md:1791`: "`sf doctor --probe` answers each against a scratch repository, except P-4 (WI-1b) and the part of P-28 that is read in the web interface" and "All are M0 unless marked."
- Spec, `SPEC.md:1486` (CLI-1): "`--probe` additionally runs the live probe suite (23.2) against a scratch repository and records the answers."
- Spec, `SPEC.md:1725`: M0 scope "`sf doctor`; the probe suite (23.2)", exit "All M0 probes answered and recorded".

Two gaps follow from the record.

1. Twelve of the 21 answered probes are not questions about a scratch repository at all, and no command of `sf` could answer them as the record did: P-6 and P-24 (the server, run by the operator), P-7 (a git prototype), P-16 (hosted runners), P-21, P-22 and P-26 (live agent sessions on the Mac), P-27, P-29, P-30, P-31 and P-34 (scripts on the Mac; for example `probes.md:1815`, "Answered 2026-11-21 for #51, on the operator's Mac"). The exception list of 23.2 names two.
2. Six rows say "Answered" while their record leaves open the half that the factory will actually exercise, the installation token: `probes.md:621` (P-1) "Every call here used the operator's OAuth token. What the App gets when it lacks a permission was not observed"; `probes.md:1225` (P-8) "What the factory's App reads with the permissions of 5.7 was not observed."; `probes.md:1434` (P-15) "what the factory's push leaves was not observed."; `probes.md:1505` (P-25) "Whether an App with write access to issues creates a label the same way was not observed."; `probes.md:2714` (P-32) "Everything was written and read with the operator's token."; and P-5, whose row says so itself. The rows carry the caveat, so this is not a wrong summary. But the exit criterion counts them as answered, and nothing schedules the second observation before M1a builds the claim (8.2 step 3), the label read (GH-3a), the read of rule 6 and the encoder on them.

**Proposed fix.** Replace the sentence of 23.2 with what is true: "A probe is answered by whatever can observe it: a script, a live session, a workflow run or the operator on the server. The record says which. `sf doctor --probe` (M0) repeats, with the factory App's installation token on the scratch repository, the probes whose answer may depend on the token: P-1, P-5, P-8, P-15, P-25 and P-32, and it answers P-2, P-3, P-14 and P-20." Add to the M0 exit criterion: "A row that says 'not an installation token' counts as answered when that repeat is recorded."

**Confidence.** High. I checked every "Answered" row against the first paragraph of its record.

---

### E5 [severity: minor] [milestone: M0] §13.3 SEC-6b, §19.5 DEV-6 and DEV-7; probes P-16, P-6

**Problem.** The means by which a worker is kept from creating a user namespace was observed on bubblewrap 0.12.0 only. The CI jobs that must run the sandbox tests get 0.9.0.

- Spec, `SPEC.md:1264`: "The bubblewrap backend is started with `--disable-userns`, the means that probe P-6 ran on the server; a seccomp filter that does the same was not tried."
- Spec, `SPEC.md:1616` (DEV-7): "The DEV-2 set, with every test of 19.4 that needs neither KVM nor the Claude Code login, the sandbox tests at level `process` included | On every event of VAL-6, on both architectures". DEV-5 pins `ubuntu-24.04` and `ubuntu-24.04-arm`, and DEV-6 (`SPEC.md:1611`) "installs `bubblewrap`".
- Evidence, `probes.md:28`: "`bubblewrap` | Not installed; `apt-get install` gives 0.9.0 | Installed, 0.11.1".
- Evidence, `probes.md:752` (P-6): "`bubblewrap`, `socat` | 0.12.0; not installed at first, then Debian's 1.8.0.3 | 0.12.0; 1.8.0.3".

P-16 ran five stand-in tests, none with `--disable-userns`, and it predates D-28. Its "Still open" (`probes.md:114`) leaves the factory's own sandbox to "the first CI run", but does not know that the sandbox now needs this option. If the runner's bubblewrap does not accept it, or it behaves differently under the runner's AppArmor setup, the required check of DEV-8 fails on every pull request (DEV-11: a step that cannot run fails by name).

**Proposed fix.** Add to SEC-6b: "The option was observed on bubblewrap 0.12.0. The hosted runners of DEV-5 install 0.9.0 (P-16), on which it was not run." Add one case to the P-16 workflow (it needs no App): `bwrap --unshare-user --disable-userns ... -- bwrap ...` on both pinned labels, inside the network namespace of DEV-6.

**Confidence.** High that it is unrecorded. I do not claim that 0.9.0 lacks the option.

---

### E6 [severity: minor] [milestone: M1a] §8.2 Deadlines, §14, §10.3 AG-8, BUD-10 rule 3; report 15; probe P-26

**Problem.** The two stall deadlines carry no `*(tunable)*` mark and have no measurement behind them, and the one measurement there is points the other way.

- Spec, `SPEC.md:581`: "Idle: no agent event for 300 s while no tool call is outstanding. Tool call: a single tool call outstanding for more than `tool_call_timeout_secs` (policy, default 1800)." `SPEC.md:1342`: "Idle and tool-call time are the stall rules of 8.2, not budgets."
- Source of the 300 s: prior art, not a measurement (`docs/research/02-orchestration-architecture.md:157`: "Stall | No events for N seconds | Kill process group, retry | Symphony 300 s; Kilroy example 600 s").
- Evidence, `docs/research/15-session-sizes.md:90`: "One model call, from the request to the end of its answer (n = 1,789) | 8 s | 71 s | 202 s | 552 s"; `:91`: "One tool call, from the call to its result (n = 2,494) | 1.4 s | 24 s | 99 s | 2,489 s"; `:41`: "long model calls and long tool calls, 9 and 5 of which ran over 5 minutes."
- Evidence, `probes.md:1726`: "The Agent call | Returns at once | Returns when the subagent has ended".

BUD-10 rule 3 keeps every subagent in the foreground, so the Agent tool call stays outstanding for the whole of a subagent's work, and the 1,800 s deadline of 8.2 then bounds a subagent. The spec does not say so, and D-22 set the turns, the wall clock and the spend from report 15 but not these two. The factory repository's policy has no `[limits]` (BOOT-6), so M1b runs on the defaults.

**Proposed fix.** Mark both numbers `*(tunable)*` in 8.2. Add: "A foreground subagent is one tool call of the main loop (P-26), so `tool_call_timeout_secs` also bounds a subagent's whole run. Report 15 timed a model call of 552 s and a tool call of 2,489 s in interactive sessions; neither deadline was measured on a headless stream, and the five issues of M1b are the first measurement."

**Confidence.** Medium. Report 15 times whole calls, not gaps between stream events, and with `--include-partial-messages` a long model call may still emit events; see also the first doubt below.

---

### E7 [severity: minor] [milestone: M0, M1a] §13.3 SEC-6, §10.6 AG-11, §10.3 AG-4; probes P-26, P-21

**Problem.** Two things the CLI reads or writes outside the directories the spec gives it were recorded and are not in the mount table.

- Spec, `SPEC.md:1259` (read-write): "`exec/<seq>/home/`, except the configuration in it that AG-4 names, and temp; the stage's target directory". `SPEC.md:857` (AG-11) passes `TMPDIR`. `SPEC.md:1260` (read-only): "a backend-defined OS baseline enumerated in code and covered by a snapshot test".
- Evidence, `probes.md:1774`: "The transcript of a subagent is written outside `TMPDIR`.** The `output_file` of a task was under `/private/tmp/claude-<uid>/`, with `TMPDIR` set to a directory of the session's own. `system/init` likewise names a socket under `/tmp/cc-socks/`. Both are on macOS; what the CLI does inside the sandbox, whose temporary directory SEC-6 gives it, was not observed."
- Evidence, `probes.md:393`: "Managed locations are read whatever the flags.** ... the CLI looking there for `managed-settings.json` and for managed agents and commands. That is the macOS path. Nothing was there, and the path on Linux was not observed."

SEC-6 denies by default. If "temp" means the directory `TMPDIR` names, a session with a subagent (AG-20) may fail to write where the CLI wants to, and the prototype of M0 would not see it, since its exit check is one agent turn. And AG-3a calls the config directory "the only user-level configuration" without saying that a managed location on the host, if the OS baseline shows it, is a second source that no flag switches off.

**Proposed fix.** In SEC-6, say what "temp" is: "a private, writable `/tmp` and the directory that `TMPDIR` names; the CLI wrote a subagent's transcript and a socket under `/tmp` whatever `TMPDIR` said (P-26, on macOS)". Add to the read-only row or to AG-4: "The OS baseline shows no managed-settings location of the agent CLI; the CLI reads one whatever the flags (P-21), and its path on Linux was not observed." Add both to the M0 sandbox prototype's checks.

**Confidence.** Medium. The record itself marks both as unobserved on Linux; the finding is that the spec is silent where its own evidence raised the question.

---

### E8 [severity: minor] [milestone: M0] §19.5 DEV-3, DEV-7; no probe

**Problem.** A platform fact that M0 builds on is stated as known and is recorded nowhere.

- Spec, `SPEC.md:1608`: "The VM is Lima running the latest Debian stable on the Mac's architecture, with `vmType: vz` and nested virtualisation so that `/dev/kvm` is available."
- Spec, `SPEC.md:1617` (DEV-7): "Sandbox tests at level `hardened` (from M2) | Never: not run (DEV-11) | With the other tests (DEV-3)".
- Where I looked: "Lima", "vz" and "nested virt" have no hit in reports 01 to 10, 14, 15 or `probes.md`. The Linux observations that exist are the server (P-24, P-6), hosted runners (P-16) and Debian 13 in Docker on the Mac (P-7, and P-6 with `--privileged`). None is a Lima guest.

The development VM is M0 scope (`SPEC.md:1725`), BOOT-3 cuts the first tag "when the rest of M0 passes in CI and in the development VM", and M1a's one real issue runs from a development build in that VM (DEV-4), at level `process`. Whether the guest gives unprivileged user namespaces to bubblewrap with `--disable-userns`, runs the agent CLI's Linux build with the login, and exposes `/dev/kvm` on the operator's Mac is unobserved. Only the last waits for M2.

**Proposed fix.** Add a probe to 23.2, answerable today without the App: "In the development VM of DEV-3 on the operator's Mac: do the five tests of P-16 and the `--disable-userns` case pass, does one session of the agent CLI complete under the stand-in of P-6, and is `/dev/kvm` present and does it answer `KVM_CREATE_VM`?" Until it is answered, word DEV-3 as intent: "with nested virtualisation, so that `/dev/kvm` is expected to be available (not observed)".

**Confidence.** High that no record exists. I make no claim about what Lima does.

---

### E9 [severity: minor] [milestone: M1a] §5.4 GH-5a, INV-9, §19.4; probe P-15

**Problem.** The encoder rests on behaviour that GitHub does not document. The record says so and asks for a live test, and the spec has neither the caveat nor the test.

- Spec, `SPEC.md:1591` (19.4): "Outbound text | The cases of probe P-15 as a table for the encoder of GH-5a, and the examples of GH-5c." That tests the encoder against the recorded answers, not against GitHub. The Contract row (`SPEC.md:1599`) is "A scheduled test against the installed agent CLI" and nothing else.
- Spec, `SPEC.md:259`, the only unobserved surface GH-5a names: "Probe P-15 wrote to no check run, as its record says, so whether a mention or a reference is live in that output was not observed. The rule does not wait for the answer."
- Evidence, `probes.md:1444`: "How long this holds.** None of it is documented behaviour. The encoder needs a contract test against GitHub that fails when a neutralised mention renders as a link." `probes.md:1409` says how: "A contract test of the encoder can read `bodyHTML` with one account."
- Evidence, `probes.md:1440`: "What GH-5a asks for beyond what was seen.** The break inside the path of a URL was tried in a comment and in a commit message, not in an issue body, a pull request body or an edit. A pull request URL was seen to be a reference; one with the break in it was not written. ... In each of these GH-5a goes by the surface next to it."

If GitHub changes how it reads a zero-width space, every encoded mention and reference goes live and no test of 19.4 fails.

**Proposed fix.** Add to the Contract row of 19.4: "and, against the scratch repository, that the encoder's output for a mention, for each form of a reference and for a character reference renders in `bodyHTML` without a `user-mention` or an `issue-link` (P-15)". Add one sentence to GH-5a: "None of this is documented by GitHub; the forms that P-15 did not write (its record lists them) are encoded on the strength of the surface next to them."

**Confidence.** High. I read 19.4 whole and GH-5a whole.

---

### E10 [severity: minor] [milestone: M0] §19.5 DEV-10; AGENTS.md; probe P-27

**Problem.** DEV-10 credits P-27 with a fact that its record does not hold, and drops a limit that the record states.

- Spec, `SPEC.md:1623`: "Two properties of the hook's command rest on P-27." The second: "And it turns every exit code of the task but 0 into 2, because Claude Code lets a call go on after any other failure of a hook".
- Evidence: the P-27 record (`probes.md:1513` to `1614`) has no observation of a hook that failed with another exit code. What it observed of a refusal is `probes.md:1586`: "A refusal reached the session as `PreToolUse:Bash hook error:` followed by the hook's command and the reason the task wrote to stderr, and the command did not run." Reports 01 to 08 do not mention `PreToolUse`. The fact is presumably from the documentation the record says was read, and it is not marked as such.
- Spec, `SPEC.md:1623`: "A Claude Code session on the factory repository is refused a command that makes git skip its hooks (D-23)." The limits DEV-10 lists end with "one started with the project's settings switched off".
- Evidence, `probes.md:1609`: "Permission modes.** The session ran in its default mode. Whether a refusal by a hook also holds when permissions are bypassed is not stated in the documentation that was read, and was not tried."

**Proposed fix.** Change "rest on P-27" to "the first rests on P-27 and the second on Claude Code's documentation of hook exit codes, which the probe did not exercise". Add to the limits: "The refusal was observed in the default permission mode only (P-27)."

**Confidence.** High. I searched the P-27 record for every use of "exit" and "refus".

---

### E11 [severity: minor] [milestone: M0] §4.4, §19.2 Git row; probe P-7; report 05

**Problem.** The git floor has no observation behind it, and the reason it was taken from does not apply to this design.

- Spec, `SPEC.md:134`: "`git` ≥ 2.42." `SPEC.md:1559`: "Git | `git` CLI ≥ 2.42".
- Evidence, `probes.md:310`: "git 2.42 to 2.46. The floor in 4.4 is 2.42 and `--attr-source` needs 2.40, but nothing older than 2.47.3 ran the suite."
- Where the number comes from, `docs/research/05-rust-implementation.md:112`: "jj 0.46.0 requires `git` >= 2.42 because `jj workspace add` calls `git worktree add --orphan`". The spec uses neither jj nor linked worktrees (`SPEC.md:694`: "Linked worktrees are not used").

Every machine in the design has 2.47.3 or later (the server, Debian 13, the Mac), so nothing breaks today. The floor is what `sf doctor` will enforce (CLI-1), and it would admit versions on which the capture commands of 9.4 were never run.

**Proposed fix.** Set the floor to what was observed: "`git` ≥ 2.47, the oldest version the hostile-tree suite ran on (P-7)", in 4.4 and 19.2. Or keep 2.42 and add "(not run below 2.47.3)".

**Confidence.** High.

---

### E12 [severity: minor] [milestone: M1a] §23.2 row P-22; probes P-22, P-26

**Problem.** The P-22 row states a result that the later record qualifies, and the driver runs in the case where it does not hold.

- Spec, `SPEC.md:1816`: "Both caps work and end the session with `error_max_turns` or `error_max_budget_usd` and exit code 1."
- Evidence, `probes.md:1764`: "A cap result is not the end of the process.** With stream-JSON input the CLI stays alive after `error_max_turns` and after `error_max_budget_usd`, and waits for input. P-22's caps ended the process with exit code 1 because those sessions had their prompt on the command line."

AG-1 always uses `--input-format stream-json`. AG-14 and BUD-6 are right, and the P-26 row says "neither cap ends the process" (`SPEC.md:1820`). Only the P-22 row still reads as if a cap ended a session of the driver's kind.

**Proposed fix.** In the P-22 row: "Both caps work: with the prompt on the command line, as in these sessions, each ends the process with `error_max_turns` or `error_max_budget_usd` and exit code 1; with stream-JSON input the process stays alive (P-26)."

**Confidence.** High.

---

### E13 [severity: minor] [milestone: M1a] §14 BUD-4; probe P-22

**Problem.** BUD-4 takes all of a call's token counts from an event that the record shows to hold one of them.

- Spec, `SPEC.md:1349`: "Where usage comes from the agent CLI, the token counts of a model call are those of its `message_delta` event".
- Evidence, `probes.md:439`: "`assistant` events.** Input and cache counts match the call. `output_tokens` does not"; `probes.md:440`: "the `message_delta` that closes each call has the real count: 123 and 11 for the same two calls". The record shows the output count in `message_delta` and the input, cache-read and cache-write counts in the `assistant` lines. It does not say that `message_delta` carries the other three. P-26 adds its sums the same way: input side from the `assistant` lines, output from `message_delta`.

**Proposed fix.** "... the output tokens of a model call are those of its `message_delta` event, and its input, cache-read and cache-write tokens those of its `assistant` lines, which P-22 found to match the call".

**Confidence.** Medium. The record's own "What changed in the spec" (item 2 under P-22) uses the short wording too; the tables are what I go by.

---

### E14 [severity: minor] [milestone: M1a] §11.3 VAL-5d; probe P-29

**Problem.** VAL-5d cites a definition of the Unicode Standard by number as the ground of its fold. The record says the definition was not read.

- Spec, `SPEC.md:968`: "Two strings get the same result exactly when one is a canonical caseless match for the other, as the Unicode Standard defines it (D145 in its section 3.13)."
- Evidence, `probes.md:1942`: "The definitions in the Unicode Standard.** D145 and the reason for the first toNFD were read in a summary of chapter 3 and not word for word."

The rule itself is backed by observation (21 pairs on APFS, 55 pairs in the sketch), so nothing is built on a guess. The citation is firmer than its source.

**Proposed fix.** Add "(read in a summary, P-29)" after the citation, or read section 3.13 and drop the item from the record's "Still open".

**Confidence.** High.

---

### Evidence: unanswered probes that M0 needs

23.2 says "All are M0 unless marked" and M0's exit criterion says "All M0 probes answered and recorded". The rows without "Answered" and without a later milestone are these eleven. M0 includes "GitHub App creation", so the App can exist before the suite runs, but the order is then fixed: App first, probes after.

| Probe | What in M0, M1a or M1b relies on it | Answerable without the factory's App? |
|---|---|---|
| P-2 (does an approval by an App or by `github-actions[bot]` count) | Nothing: PR-2 ignores such approvals; an advisory item of 20.1 | Half. The `github-actions[bot]` half needs only a workflow and a ruleset on the scratch repository. The App half needs an App that approves, and INV-5 forbids that of the factory's own App, so it needs the separate test App of 19.4 |
| P-3 (an App bot as assignee) | Nothing | No: it needs an App's bot account |
| P-5, the rest (is an App's push of a workflow change rejected; an App's push of a link at `.github`, of a nested or other-case workflow directory) | GH-11 states the rejection as a fact (`SPEC.md:333`). VAL-5e and VAL-5f let such paths through with a flag in M1a, and "a run meets the rejection at publish" (`SPEC.md:1031`) if GitHub rejects them | No: it needs an App without the Workflows permission |
| P-9 (blocked-by across repositories, the per-issue limit) | Nothing in M1: one repository, attended | Yes, with the operator's token |
| P-10 (do comment edits count toward the content limit) | GH-16, a tunable budget; M1b edits one status comment per run | Roughly. The limit that matters is the installation's, so the firm answer needs an App |
| P-11 (`merge-async` on a personal-account repository) | Nothing in M1: strategy `human` | Yes for the endpoint's behaviour, with the operator's token; the App's call needs the App |
| P-12 (does a base-merge push dismiss approvals) | Nothing in M1: base update is M2; 12.4 assumes yes until answered | No: the pusher must be someone other than the approver. A workflow's push could stand in |
| P-13 (cold against seeded build time) | Nothing: 9.3 only allows a later optimisation | Yes, but only once the factory repository has code to build, which is the end of M0 |
| P-14 (`current_user_can_bypass` for an installation token) | DEP-4, which is written as conditional on the answer; M1b's `enforced` detection works without it | No: it needs an installation token |
| P-17 (actor and time of issue type and field changes) | Nothing: SEC-1 uses labels until answered | Yes, with the operator's token |
| P-20 (is a base-update push rejected when the default branch changed a workflow) | Nothing in M1: base update and reset are M2 | No: it needs an App without the Workflows permission |

No rule of M0, M1a or M1b silently depends on one of these: where the spec uses one, it says so and names the fallback. The cost is to M0's exit criterion alone. Five of the eleven cannot be answered before an App exists (P-3, the rest of P-5, P-12, P-14, P-20), two more only in part (P-2, P-10), and one (P-13) cannot be answered before the end of M0. P-11, P-12 and P-20 serve only M2. If the criterion stays as written, M0 waits on probes that nothing before M2 uses. Marking P-11, P-12, P-13 and P-20 "(M2)" in 23.2 would match what the roadmap builds.

Two further rows are answered in part and should be read with the table:

- **P-26, third part** (which fields show usage credits). It is marked M1a and its row says it "is not provoked". BUD-8 rests on documented fields. M1a's exit criterion names no probe, so this blocks nothing, but the row can never be closed on purpose.
- **Rows answered "with the operator's token and not an installation token"**: P-1, P-5, P-8, P-15, P-25, P-32. See E4.

### Evidence: doubts about the evidence

1. **Report 15, section 3.4, tool calls.** The 2,489 s maximum (`docs/research/15-session-sizes.md:91`) comes from interactive sessions, where a tool call can wait for the operator to approve it. The report does not say whether such waits are in the figure. If they are, it overstates how long a headless tool call runs, and E6 should be read with that in mind.
2. **P-5, how long "no run" was watched.** The second run listed the repository's runs "the last time 2 minutes after the last push" (`probes.md:2066`). The first run looked again after five minutes. VAL-5f's flag-and-not-fail for a link at `.github` rests on the absence of a run, and two minutes is a short window for an absence. A workflow registered later, or one that only a `schedule` or `workflow_dispatch` event would start, would not have shown. The row does name the unobserved events.
3. **P-25, letter case.** One request, on one endpoint, under one API version. GH-3a builds a rule on it and says "in the one request that tried it", so the spec is honest about it. I list it because a rule of the claim path (a label in another letter case "counts as missing") rests on a single observation.

### Evidence: covered

Read in full, record against row against the spec passages the row names:

- P-1, P-5, P-6, P-7, P-16, P-21, P-22, P-24, P-25, P-26, P-27, P-29, P-30, P-34.
- P-8 and P-15: everything but the Method sections (`probes.md:1068` to `1106`, `1249` to `1294`).
- Reports 14 and 15, against 4.4, section 14 and D-22. Every number matched.

Sampled:

- P-4: answer, findings and "Still open"; its parser and lint are enforced from M3, so I did not check WI-1 line by line.
- P-28: all but the table of translated entries (`probes.md:2765` to `2797`); VAL-3a matched everything I read.
- P-31: tables, findings and "Still open", not the method.
- P-32: method, the results table, findings and "Still open", not the 71 sample texts (`probes.md:2247` to `2572`). SEC-3 and SEC-4 matched the table text for text where I compared them (texts 2, 10, 14 to 16, 29 to 31, 54, 56, 57, 65 to 69, 72 to 74, 77).
- Reports 01 to 08: by search only, to check a fact before reporting it (hook exit codes, the git floor, the stall figure, SIGTERM, App-token pushes, required-check conclusions). Reports 09 and 10: the first screen of 10, its C17, and searches, to avoid repeating a finding.
- Reports 11 to 13 are not in the tree at this commit; `probes.md` links to them on research branches. I searched 11 and 12 through the linked commits only to try to refute E1 and E7.

`SPEC.md` sections read whole: 3.1, 4, 5.2 to 5.9, 6, 7.3, 8, 9, 10, 11.1 to 11.4, 11.6, 12.1 to 12.3, 13.2 to 13.5, 14, 17, 18, 19.2 to 19.5, 20, 21.1, 22, 23.2, and the rows D-2, D-5, D-14, D-16, D-21, D-22 and D-27 of 23.1. Not read: 1, 2, 3 before 3.1, 5.1, 7.1, 7.2, 7.4 to 7.7, 11.5, 11.7, 12.4, 13.1, 13.6 to 13.8, 15, 16, 19.1, 21.2, the rest of 23.1, 23.3 beyond a skim, and the appendices. Most of those are M2 or later.

Checked and found consistent, so not reported: the 23.2 rows of P-6, P-7, P-16, P-21, P-24, P-25, P-27, P-28, P-29, P-30, P-31, P-32 and P-34 against their records; VAL-3a against P-28; VAL-5e and VAL-5f against P-5 and P-30; VAL-5g against P-34; SEC-3 and SEC-4 against P-32; AG-2b against P-31; AG-4 and AG-8 against P-21 and P-6; BUD-6 and BUD-10 against P-26; 8.2 step 3 against P-1; GH-3a against P-25; rule 6's tie rule against P-8; DEV-5 and DEV-6 against P-16; the table of 4.4 against report 14 and P-24.

## Lens: security

Scope: the pipeline as M0, M1a and M1b build it. Line numbers are those of `SPEC.md` and `docs/research/probes.md` at commit 9c55344. Twelve findings: none critical, six major, six minor. I found no sequence that breaks an invariant of 3.1 in M1 as the text stands; S5 comes closest (INV-11), and it turns on a sentence that can be read two ways.

Facts about tools that no probe of the repository records (which file names clippy, rustup, nextest and mise read, what GitHub collapses in a diff, where the CLI's WebSearch runs) are from my own knowledge of those tools. Each finding that rests on one says so and asks for a probe.

---

### S1 [severity: major] [milestone: M1b] §11.3 protected-path list, BOOT-4, AG-3a, probe P-21

**Problem.** A factory pull request can replace the instructions of the operator's interactive Claude Code sessions, and the flag listing does not name the file that does it.

The built-in list protects `AGENTS.md` and `CLAUDE.md` and not `CLAUDE.local.md`: `SPEC.md:919` "Change under a **protected path**: `.factory/**`, `AGENTS.md`, `CLAUDE.md`, `.claude/**`, `.mcp.json`, …". The pattern `CLAUDE.md` does not match the path `CLAUDE.local.md` (VAL-5b matches the whole path or the part after a `/`). BOOT-4's `protected_extra` (`SPEC.md:1665` and its table) does not name it either, and the repository's `.gitignore` holds `/target` alone, so capture stages the file.

Probe P-21 planted that file and found that it does more than add text: `probes.md:385` "**`AGENTS.md` is read only when no `CLAUDE.md` and no `CLAUDE.local.md` is in the tree.** With a `CLAUDE.local.md` next to it and no `CLAUDE.md`, the unlocked session had the local file in context and not `AGENTS.md`." The factory repository has an `AGENTS.md` and no `CLAUDE.md`.

Sequence: a subverted implementer writes `CLAUDE.local.md` at the root (or in any directory) beside its real change. No rule matches the path. On the factory repository nearly every pull request already carries `factory:protected-change` for `crates/**`, and the listing, which exists so that the operator can tell one protected change from another (`SPEC.md:931` "The label alone does not say what was touched"), does not name the file. Once merged, every interactive session of D-16 on the Mac, which holds the operator's `gh` login, reads the worker's text and no longer reads `AGENTS.md`, the file that tells a session that only the operator applies `state:ready` and that a session creates no label. A `labeled` event made by such a session has the operator as its actor and passes SEC-1. BOOT-4 protects `docs/agents/**` for exactly this reason: `SPEC.md:1682` "A worker that changed these files would change what a session with a GitHub credential is told to do." The lock-down of AG-3a keeps the file out of factory sessions only.

**Proposed fix.** Add `CLAUDE.local.md` to the built-in list of 11.3. State the rule behind the list so that it follows the CLI: every name that probe P-21 found an unlocked session to load unasked from the working directory is a protected path, and the contract test's fixture (19.4) is the list. Consider `fail` in place of `flag` for a file of that name that a candidate adds: no worker has a reason to write one.

**Confidence.** High that the path is unprotected and that P-21 records the displacement of `AGENTS.md`. I searched the spec for `CLAUDE.local`: it appears at `SPEC.md:798` (the lock-down and the reviewer's export) and in the P-21 row of 23.2, and in no list. 23.3 and D-24 do not accept it. Medium on how an operator reading the diff would miss a new root file; the finding is that no flag names it.

---

### S2 [severity: major] [milestone: M1a] §9.4 step 1 and step 3, 11.3, 13.1, probe P-7

**Problem.** A candidate can carry a directory with the layout of a bare git repository, with a `config` of the worker's choosing, and no layer refuses or flags it.

The scan refuses names, not layouts: `SPEC.md:706` "Fail if any entry other than the root `.git` has a name that git would treat as `.git`". Probe P-7 tried the other case and left it open: `probes.md:220` "A directory that holds `HEAD`, `config`, `objects/` and `refs/` but is not named `.git` | Nothing: candidate | Its files are staged like any others", and under "Still open", `probes.md:312` "A directory with the layout of a bare repository, under a name other than `.git`, passes capture. Git started inside that directory of a checkout would use its `config` unless `safe.bareRepository` is `explicit`. Whether 11.3 should flag it is not decided here." Draft 4 did not decide it: 11.3 has no rule for it, `index-pack --strict` objects only to an entry named like `.git`, and the threat row covers the factory's host alone (`SPEC.md:1193` "INV-2 and INV-3: the host does not open workspace paths; git on a workspace runs sandboxed; objects are verified at ingest").

Sequence: a subverted implementer adds `tests/fixtures/repo/` with `HEAD`, `config`, a file under `objects/` and one under `refs/`; the `config` names a command in a setting that git runs (a pager, an fsmonitor with `core.worktree` set, an alias). It looks like a test fixture, which this repository's git tests plausibly have. It is published in M1a and merged in M1b by an operator who reads it as a fixture. From then on any git command started with that directory as its working directory, on any checkout, runs the command as the user who started it: a shell prompt that asks git for a status, an editor's git integration, a test that changes into its fixture. The checkout that matters is the operator's Mac, which holds the `gh` login and the Claude login, and the server when the operator runs the check on a tagged commit outside any sandbox (DEV-7). The factory's own git commands are safe, since each names its `--git-dir`.

**Proposed fix.** Decide the open point of P-7. Smallest: the tree walk of 9.4 step 3 rejects, among the paths the round adds or changes, a directory that holds `HEAD`, `objects/` and `refs/` together, which is git's own test for a git directory; a `flag` in 11.3 is the weaker alternative. Because the walk reads only the round's paths, a base tree that already holds one does not fail every capture; list those at onboarding as the symbolic links are (20.1). Add the case to WS-9's suite, and have the operator's machines set `safe.bareRepository = explicit`.

**Confidence.** High that the spec has no rule: I searched for `bareRepository` and "bare repositor" and found only `host.git`. The behaviour of git is as P-7's record states it, not re-run by me. Medium on reachability: it needs a merge and a git command in that directory.

---

### S3 [severity: major] [milestone: M1b] §11.3 protected-path list, BOOT-4, D-24, DEV-2, 19.3

**Problem.** The lists protect one file name per check tool, and several of the tools read a second name. A candidate that adds the second name changes the check, in verify and CI for its own pull request and for every later one, and no pattern matches it.

`SPEC.md:919` names `clippy.toml`, `rust-toolchain.toml` and `deny.toml`. BOOT-4 adds `mise.toml`. For a configuration file that does not exist yet the spec has a rule of procedure and no check: `SPEC.md:1683` "A file that configures a check of DEV-2, the development VM (DEV-3) or the release build (19.2) joins the list in the change that adds the file." (D-24 says the same at `SPEC.md:1761`). That binds a session that adds such a file in good faith. A subverted worker does not add its own file to the list, and the structural test compares the policy with BOOT-4's table (`SPEC.md:1604` "the structural test of 19.1 also checks that the policy's `[paths] protected_extra` lists every path of BOOT-4"), not with the files the tools read.

Cases, none of which a pattern of 11.3 or BOOT-4 matches at the repository root:

- `.clippy.toml`. clippy reads that name as well as `clippy.toml`, looking from each crate's directory upward, and where one directory holds both it takes the dotted one and prints a warning that `-D warnings` does not turn into an error. The repository's `clippy.toml` holds the `disallowed-methods` of 19.3 (`SPEC.md:1578` "bare `tokio::spawn` outside the supervisor and `std::process::Command::new` outside `factory-proc`"), which is how the code is held to INV-3's spawning discipline. A root `.clippy.toml` with an empty list switches that off.
- `.config/nextest.toml`. nextest reads it unasked; a `default-filter` there leaves tests out of `cargo nextest run`, which is a verify command of BOOT-6 and a step of DEV-2.
- `rust-toolchain`, the name without the extension. rustup reads it and prefers it to `rust-toolchain.toml` where both exist. WS-7 keeps rustup off a worker's `PATH`, so verify is not affected; CI and the operator's machines are.
- `mise.local.toml`, `.mise.toml`, `.config/mise.toml`, `.tool-versions`. mise merges them over `mise.toml`. `SPEC.md:1604` "Each check tool of this set is pinned in `mise.toml` at one exact version … and interactive sessions and CI use that version"; a sibling file moves a pin or adds a tool, and the structural test still finds `[tools]` equal to `mise.toml`.

On the factory repository the consequence is the one item 6 of the brief asks about: a factory pull request weakens a check that gates later factory pull requests, and the listing of VAL-3 names nothing. The operator still has the diff.

**Proposed fix.** Three parts. (1) Built-in list: add `.clippy.toml` and `rust-toolchain`. (2) BOOT-4: add `/.*` and `/.*/**` (every dot file and dot directory at the root, which covers `.config/**`, `.mise.toml`, `.tool-versions` and what comes later) and `mise.*.toml`; both are valid in the grammar of VAL-5c and, as flags, safe to over-match (VAL-5b). (3) A probe, as P-30's record already asks (`probes.md:2002` "clippy with `clippy.toml`, rustup with `rust-toolchain.toml` and `cargo deny` with `deny.toml` were not run on a link"): for each tool of DEV-2, which file names it reads unasked and which wins. Where a tool takes its configuration as an argument, `cargo xtask check` can pass it, so that a sibling file is inert.

**Confidence.** High that none of these paths is matched and that nothing but procedure covers a new configuration file; I searched the spec for each name. Medium on the tool facts: they are from my knowledge of clippy's lookup, the rustup book, nextest's and mise's documentation, and no probe of this repository ran them. The precedence of `.clippy.toml` over `clippy.toml` in one directory is the one I would probe first.

---

### S4 [severity: major] [milestone: M1a] §13.2 SEC-2, 5.3, 8.2 step 1, 22 (M1a row), 13.1

**Problem.** SEC-2 is the control the threat model names against a stranger's text, and no step performs it.

`SPEC.md:1212` "The factory works only on issues written by the operator: the issue's author and every editor of its title and body must be authorised actors … Any other issue is never dispatched, whoever labels it and at any sandbox level." The threat table rests on it (`SPEC.md:1196` "SEC-2 (only the operator's issues)").

The places that say what a claim checks do not hold it:

- Readiness has six rules (5.3) and none is about the author or the editors. Rule 6 is about time, and it fixes what the claim reads: `SPEC.md:226` "The check and the snapshot come from one GraphQL read (title, body, `lastEditedAt`, rename events, the `labeled` event), so they cannot diverge." The author and the edit history are not in that read.
- The claim: `SPEC.md:567` "One GraphQL read of the issue: check SEC-1 on the `state:ready` label and rule 6 of 5.3; capture the snapshot; record tier, risk and type under SEC-1."
- The roadmap lists the parts of the M1a claim one by one and stops at comment authors: `SPEC.md:1726` "claim, with the label read of GH-3a, the check for the toolchain and the check tools (WS-7a), and the snapshot of SEC-3 and SEC-4, for which the claim checks the permission of each distinct comment author once".

Sequence: the scratch repository (M1a) and the factory repository (M1b) are public, so anyone can open an issue. The operator reads a useful report on the rendered page and applies `state:ready`. SEC-2 says this is never dispatched and that the operator restates the report instead; the operator may rely on that. A claim built from 5.3 and 8.2 dispatches it: SEC-1 holds (the operator applied the label), rule 6 holds, and the stranger's body becomes the task block of a session that holds the subscription token. SEC-4 removes HTML comments and invisible characters and says of itself that it is no boundary, and it leaves link destinations, image text and tag attributes in place. The same gap leaves undefined what a failed SEC-2 does: no run, a comment, or a state change.

**Proposed fix.** Make it rule 7 of 5.3 and name it in 8.2 step 1: the one GraphQL read also takes the issue's author, the editors of its edit history and the actors of its rename events; each distinct account is checked once, as SEC-3's comment authors are, and the App is known by its numeric id. On failure the claim ends with no run row and nothing written to GitHub, and `sf run` says which account failed. Add "SEC-2" to the M1a row of section 22.

**Confidence.** High. I searched for every citation of SEC-2 (lines 38, 1196, 1212, 1753, 1802) and for "author", "edit history" and "every editor" in 5.3, 6.4, 8.2 and 22; the only places that read editors are the resume of 7.5.1 and the reconciler of 7.7, both M2. Not accepted in 23.3; D-13 decides the rule, not its absence from the claim.

---

### S5 [severity: major] [milestone: M1a] §9.1, 9.4 Export, 8.4 step 3, INV-11, VAL-2

**Problem.** The export is named by the candidate's SHA and verify may write to it, and no rule says that a second use of the same candidate starts from a new one.

`SPEC.md:666` "export/<sha>/                  clean checkout of a candidate, for verify and review". `SPEC.md:715` has the checkout go "into that new, empty, host-created directory" and ends "The export is writable and disposable for `verify`, read-only for `review`." The directory is new and empty the first time. The text does not say what happens the second time the same `<sha>` is exported, and two things make a second time ordinary in M1a: the exit criterion (`SPEC.md:1726` "reaches a draft pull request and survives an interrupted run") and the recovery rule (`SPEC.md:611` "The run resumes at the start of its recorded stage, replaying journalled sub-steps (a journalled candidate SHA, a journalled push) instead of recomputing them"). An implementer who treats the export as one more journalled sub-step keeps the directory that is there. Every other per-execution directory of 9.1 is keyed by `<seq>` and said to be new each time (`exec/<seq>/target/` "new and empty each time"; `quarantine/` "the host creates anew for each capture").

Sequence: verify runs the candidate's own tests with the export writable. A test of a subverted candidate rewrites files of its export (the failing test, or the source under test) and then hangs. The run is attended, so the operator interrupts it and starts `sf run` again; or the supervising task dies and the stage restarts (8.2). The second verify runs on the directory the first one left and passes. `factory/verify` is then reported for a tree that never passed (`SPEC.md:909` "`factory/verify` records the tree SHA that was verified and is reported only on a head whose tree equals it"), against INV-11 (`SPEC.md:102` "They run on a clean export of the candidate commit, in a fresh sandbox instance with a fresh home directory"). In M1b CI is a second gate on the real tree; in M1a nothing is.

The same sentence gives one directory to verify and to review. That is M2 and out of this review's scope, but the fix is the same and INV-11's last sentence already forbids it.

**Proposed fix.** Key the export by stage execution: `exec/<seq>/export/`, made by the host at the start of each verify (and each review), removed at its end, never reused and never shared. A restarted stage has a new `<seq>` and so a new export. Add to the Git row of 19.4: a verify that is restarted after its commands changed the export runs on the candidate's bytes.

**Confidence.** Medium. "new, empty" can be read as a requirement on every use, in which case the gap is one of wording only; the path by SHA, the single line of 9.1 for both stages and 8.4's replay rule make the other reading natural. I searched for every mention of the export (9.1, 9.3, 9.4, WS-5, WS-11, 18.2) and found no statement about a second use. Drafts 1 and 2 raised the command (C18) and the shared target directory (C25), not this.

---

### S6 [severity: major] [milestone: M1a] §11.3 VAL-5a, VAL-5e to VAL-5g, probe P-30

**Problem.** Every path rule of 11.3 reads "a path of the diff", and the rule that fixes how the host makes a diff does not say whether renames are detected. With detection on, a protected file can be moved away and the rule sees only the place it went.

`SPEC.md:943` lists what is switched off: "runs against bare `host.git` with no work tree, with external diff and textconv disabled and with no in-tree attributes applied (attribute source set to the empty tree)". Rename and copy detection are not in the list. `git diff` detects renames by default; `git diff-tree` does not. Probe P-30 made its diffs with detection off and says so as an open point: `probes.md:2003` "**Rename detection.** The diff was made with `--no-renames`. What the host's diff does with a link that is renamed was not looked at".

Sequence, where the implementation leaves detection on and matches the path a diff entry ends at: the candidate moves `clippy.toml` to `notes/lints.txt`. The diff holds one entry, a rename, whose new path matches no pattern. clippy now runs with its defaults and no flag is raised. The same move takes a workflow out of `.github/workflows/`, a build script away from the name `build.rs`, or a locked test (M3) to another name. VAL-5g already assumes the other behaviour without requiring it (`SPEC.md:1038` "so a manifest or a lockfile that moved is new at its new path").

**Proposed fix.** One clause in VAL-5a: "with rename and copy detection off, so that a moved file is a deletion at its old path and an addition at its new one, and both are paths of the diff". Add a moved protected file to the path-pattern row of 19.4.

**Confidence.** Medium. The capture commands of P-7 use `--no-renames`, so an implementer may well carry it over, and then there is no gap. The spec does not require it, and the probe that the rule cites records the question as open. I searched for "rename" in the spec: nothing in 11.3 or 9.4 beyond the title's rename events.

---

### S7 [severity: minor] [milestone: M1a] §13.5, 11.3, GH-5a, 22

**Problem.** The literal-token scan is one of the four mitigations 23.3 counts on for `subscription` mode, and it exists as half a sentence in a table cell. It has no id, no statement of what bytes it reads, and its test arrives a milestone after the first runs it guards.

The rule: `SPEC.md:1282` "the host searches every candidate and every text written to GitHub for its literal value and fails the stage on a hit". The roadmap places it inside the integrity stage (`SPEC.md:1726` "integrity with path, structural and secret rules, including the matcher of VAL-5b to VAL-5d and the literal-token scan"), and the integrity table speaks of "added content" (line 926). The test is an exit criterion of M2 (`SPEC.md:1728` "a planted copy of the subscription token is caught before publish"), after M1a has published to the public scratch repository and M1b to the public factory repository with the real token.

Left open, each of them a place where the literal value can sit:

- a path, or the target of a symbolic link, which are text of a tree and not added content of a blob;
- a binary blob, which a line-based scan of added lines does not read;
- the commit subject, which capture validates (9.4) before integrity runs;
- the pull request body, the status comment and the check-run output, which GH-5a does not connect to the scan at all;
- whether those texts are searched before or after the encoder. The encoder inserts U+200B after the hyphen of `GH-<digits>`, compared without regard to case (`SPEC.md:252` onward). A token that holds `gh-` and a digit is no longer its literal value after encoding.

**Proposed fix.** Give the scan an id in 13.5 and say: it reads every blob the candidate adds or changes, whole and whatever its content; every path and link target of `base_sha..candidate`; the commit message; and every body handed to `factory-github`, before the encoder. Move the planted-token test from M2's exit criterion to M1a's.

**Confidence.** High that the text says no more than quoted (I searched for "literal", "token scan" and "secret"). The weakness of a literal scan against an encoded token is 23.3's and is not reported here; this finding is about where the literal scan looks and when it is tested.

---

### S8 [severity: minor] [milestone: M1a] §22, 19.4 (Contract, Sandbox), AG-3a, AG-4, SEC-6b, SEC-11

**Problem.** Two more of the mitigations are tests that no milestone requires before the first real session with the subscription token.

- The contract test. `SPEC.md:1728` puts "lock-down contract test and plan-limit pause" in M2. It is the only thing that asserts, on the server and inside the sandbox, the lock-down of AG-3a, that a worker cannot change the configuration of AG-4, and that a session needs no host but the model's. AG-3a says what it rests on until then: `SPEC.md:798` "This was observed on one CLI version, on macOS and outside the sandbox." M1a runs the Claude driver in `subscription` mode, and M1b runs it on the factory repository.
- The escape tests at level `process`. M2 has "sandbox escape tests pass at `hardened`" as an exit criterion (`SPEC.md:1728`). No row has the same for `process`, the level of every run in M1, where by the spec's own words `SPEC.md:1264` "At level `process` the host's kernel is all that stands between a worker and the host." DEV-7 puts those tests in the DEV-2 set (`SPEC.md:1616` "the sandbox tests at level `process` included"), and DEV-1 adds each check "when it is built", with no milestone. M0's exit check is one real turn "with only the proxy reachable" (`SPEC.md:1725`), which observes the egress rule once and none of the Sandbox row of 19.4.

23.3 accepts the residual risk of the token with these mitigations in place (`SPEC.md:1836` "The mitigations (model-requests-only scope, egress allowlist, literal-token scan, lock-down contract test) reduce this but do not remove it"). In M1 two of the four are not yet tested.

**Proposed fix.** M1a exit criterion: the contract test passes on the machine that runs the session, and the sandbox tests of 19.4 at level `process` pass in CI and on the server. If the contract test stays in M2, say in 23.3 that M1 runs without it.

**Confidence.** High on the rows as quoted. In M1 a failed lock-down lets an implementer configure only its own later sessions, since no reviewer exists yet, which is why this is minor.

---

### S9 [severity: minor] [milestone: M1b] §11.3 VAL-3, GH-5c, BOOT-4, D-24

**Problem.** A worker can choose which protected paths the flag listing names.

`SPEC.md:931` "For one flagged rule the listing names at most 50 paths *(tunable)*, the first in the byte order of the paths, and then says, outside the fence, how many more there are; the journal holds them all (D-33)." All protected-path patterns are one rule, so one cap covers them. On the factory repository `crates/**` is protected, and in byte order `crates/…` sorts before `deny.toml`, `docs/agents/…`, `mise.toml`, `rust-toolchain.toml` and `xtask/…`.

Sequence: a candidate that changes `xtask/src/check.rs` (what CI runs) or `mise.toml` also touches fifty files under `crates/`, one line each, well inside the size budget of 600 lines. The listing shows fifty `crates/` paths and "3 more". D-24 added the listing because on this repository the label alone says nothing; here the listing says nothing either, about the three paths that matter most. The diff and the journal still hold them.

**Proposed fix.** Before the paths, always print the distinct patterns that matched, each with its count, in the host's words outside the fence; that list is bounded by the policy and is never cut. Apply the cap of 50 for each pattern and not for the rule.

**Confidence.** High; the rule is as quoted and GH-5c's cut by length works the same way ("The lines it drops are counted among those the listing does not name").

---

### S10 [severity: minor] [milestone: M1b] §11.3 (attribute and binary rules), 11.6 item 5, 13.1

**Problem.** The threat table promises flags on "hidden diffs", and the rules follow git attributes and not what GitHub hides.

`SPEC.md:922` flags "A changed path carries an attribute that hides or alters its diff" and lists it "as \"hidden or collapsed in the GitHub diff\""; `SPEC.md:923` flags "A binary change to a path that was text at base"; item 5 of the evidence manifest names "files hidden or collapsed in the GitHub diff" (`SPEC.md:1121`). Three ways to have GitHub not show a change need no attribute:

- a new binary file. It was not text at base, so line 923 does not apply, and GitHub shows no content for it;
- a path that GitHub's own detection of generated files matches. It suppresses those in a diff as it does for `linguist-generated`, by name and by content, and `Cargo.lock` is on its list;
- a diff of one file over the size GitHub renders by default.

In each case the operator has to open the file by hand, and nothing tells them to. The worker chooses the path and the size.

**Proposed fix.** Flag every binary blob a candidate adds, not only a text file that became binary. Add a probe: which paths of a pull request GitHub collapses with no attribute set, and at what size. List those in item 5, or record in 23.3 that the listing covers attributes only.

**Confidence.** Medium. The rules are as quoted. What GitHub collapses is from my knowledge of its linguist library and its page, and no probe of the repository observed it.

---

### S11 [severity: minor] [milestone: M1b] §11.6 item 3, GH-5a, 10.5

**Problem.** The evidence manifest puts agent text in a table, and the encoder's protection for agent text is a fenced block, which a Markdown table cell cannot hold.

`SPEC.md:252` "renders the text inside a fenced block labelled as agent output (except sub-issue bodies, PLAN-5), and neutralises `<!--`, `-->`, @mentions and issue references". `SPEC.md:1119` "A table: each acceptance criterion → result → evidence → verification method." The evidence is the implementer's `criteria[].evidence` (10.5). A pipe table has one line of inline text for a cell. The implementer of M1b either leaves the fence out for that column or leaves the table out. Without the fence the neutralisations are all that stand, and they do not cover a `|` or a line break (which add cells and rows, or end the table and start a section that reads as the host's own "Gate results"), a link, an image, or raw HTML. The flags stay at the top of the body (VAL-3), so this forges text below them, in the part the operator reads to decide a merge.

**Proposed fix.** Say in 11.6 how item 3 is laid out under GH-5a: a table of host-written cells (criterion id, result, method) and, below it, one fenced block for each criterion's evidence under a host-written heading. Add a criterion's evidence that holds a `|`, a line break and a link to the outbound-text row of 19.4.

**Confidence.** Medium. It is a conflict between two rules and not an observed fault; GH-5a read strictly ("All text that originates from an agent …") already forbids the unfenced cell, and then item 3 cannot be built as written.

---

### S12 [severity: minor] [milestone: M1a] §10.1, 10.2, AG-1, SEC-11, 13.1, probe P-21

**Problem.** No rule says which tools a role's session has, and one of the CLI's built-in tools reaches the open web through the one host the proxy must allow.

AG-1 passes a "tool allowlist (`--tools` / `--allowedTools`)" (`SPEC.md:784`), `SessionSpec` has `tools: ToolPolicy`, and 10.2 gives each role its file access. Nothing lists the tools. Probe P-21 records what a non-bare session brings: `probes.md:391` "23 built-in tools, among them `WebFetch`, `WebSearch`, `CronCreate`, `RemoteTrigger` and `Workflow` … The tool allowlist of AG-1 removes the tools." It removes what the list leaves out, and the list is not written.

The CLI's web search is carried out by the model provider, so its traffic is a request to `api.anthropic.com`, which SEC-11 allows and whose paths the proxy cannot see (`SPEC.md:1272`). An implementer with that tool reads web pages (`SPEC.md:1187` counts "any web page an agent fetches" as attacker-controlled input) although egress is default-deny, and nothing appears in the denied-request log. That gives an agent that was not subverted a source of hostile text which the design otherwise closes for M1, where issues are the operator's own.

**Proposed fix.** List the tools of each role in 10.2 (for the implementer: the file tools, the shell and the Agent tool of AG-20), and state that no role has a tool that fetches or searches the web, schedules work or starts a remote session. Have the contract test assert that the tool list of `system/init` equals the role's list.

**Confidence.** Medium. High that the spec names no tool list (I searched for `ToolPolicy`, `allowedTools`, `WebSearch` and "tool allowlist"). That the search runs on the provider's side is from my knowledge of the CLI, not from a probe. A worker that is already subverted can call the model host with the token directly, and that is 23.3's; this finding is about how a session becomes subverted.

---

### Security: covered

**Read in full:** 3 and 3.1 (invariants); 4.1 to 4.4; 5.1 to 5.9 (with GH-3a, GH-5, GH-5a, GH-5c); 6.4 and 6.5; 7.3, 7.6 and 7.7; 8.2 and 8.4 to 8.7; 9.1 to 9.5; 10.1 to 10.6 (with AG-2a, AG-2b, AG-3a, AG-4, AG-11a, AG-21); 11.1 to 11.7 (all of 11.3: VAL-3, VAL-3a, VAL-5 to VAL-5g); 12.1 to 12.4; 13.1 to 13.8 (all of SEC-1 to SEC-12); 14 (BUD-1 to BUD-10); 16.1 to 16.3; 17; 19.1 to 19.5 (DEV-1 to DEV-11); 20.1 to 20.4; 21.1 and 21.2 (BOOT-1 to BOOT-6); 22; 23.2 (the table, each row's first lines); 23.3. `GLOSSARY.md` in full. The repository's own `.gitignore`, `.claude/settings.json`, `clippy.toml`, `mise.toml`, `.cargo/config.toml` and the start of `xtask/src/check.rs`, to check S1 and S3 against the tree.

**Probe records read in full:** P-5, P-7 (commands, layers, findings, open points), P-21, P-34. **In part:** P-22 (hosts and what follows), P-6 (findings and open points), P-30 (open points).

**Sampled only:** 23.1 (D-24, D-28 and D-31 in full, the first lines of the others); 6.1 to 6.3; 7.2, 7.4, 7.5 and 7.5.1 (M2 and later); 8.1 and 8.3; 15; 18; the two earlier reviews (the first screen and the security lens of `10-spec-recheck.md`, and a search of both for each finding above, none of which they hold). `docs/research/06-security-governance-ops.md` was not read. Probes P-8, P-15, P-25, P-28, P-29, P-31 and P-32 were not read, so the encoder (GH-5a), the fold (VAL-5d), the sanitisation (SEC-4) and `CODEOWNERS` (VAL-3a) were checked against the spec's own text and not against their records.

**Checked and found closed or accepted, so not reported:** the dependency rule inside a manifest and a lockfile (tight against P-34; a move to another version of a named crate is D-32's); a link at `.github` or `.github/workflows` (replacing the directory deletes its files, which the workflow rule fails); a new file that an instruction reference already names (the instruction directories are protected patterns); a worker's own git commits and `.git` (WS-3; never read); hard links out of a workspace (not possible across the sandbox's mounts); approval in M1 by anyone but an authorised actor (SEC-1 on `state:ready`, rule 6, P-25 through GH-3a); agent text to GitHub before the encoder exists (M1a builds it whole, D-33); removal of the token through the model host or encoded in a candidate (SEC-12 and 23.3); the M1a cut of the path rules (D-30).

## Lens: buildability

Snapshot: commit 9c55344. Every `SPEC.md:<line>` below is a line of that commit, and each quoted string was checked against its line by script before this file was written.

Fifteen findings: 1 critical, 11 major, 3 minor. None repeats a finding of `09-spec-review.md` or `10-spec-recheck.md`.

---

### B1 [severity: critical] [milestone: M0] §22 (M0 exit criterion), 21.1 BOOT-3, 17 CLI-1, 13.5, 20.1

**Problem.** The third clause of the exit criterion needs something that no M0 item produces. The criterion is "a cold build of the factory repository and one real agent turn succeed on the server under bubblewrap, started as SEC-6b says, with only the proxy reachable" (`SPEC.md:1725`), and BOOT-3 says who runs it and with what: "the operator installs it on the server to run M0's exit check (22)" (`SPEC.md:1664`). So the tagged binary `v0.1.0` has to run a build and an agent session inside a sandbox instance.

Section 17 has one command that does that, `sf run` ("same engine, one item, foreground, resumable", `SPEC.md:1482`), and it is M1a. The only M0 command is `sf doctor`, which "checks credentials, App permissions, CLI versions and flags, sandbox availability at the configured level, git version, the toolchain and the check tools for each repository (WS-7, WS-7a), enforcement mode and the capability probe" (`SPEC.md:1486`): availability, not a build and not a session. The M0 scope lists "prototypes of capture, ingest and export against the hostile-tree suite (WS-9), of the bubblewrap sandbox, and of the egress proxy" (`SPEC.md:1725`) and no item that joins the two prototypes to a command.

The agent turn needs more that M0 does not build: the subscription token reaches `sf` through a command no milestone row names (the operator "gives it to `sf init`", `SPEC.md:1282`), and a session needs the per-execution config directory and settings file of AG-4 and AG-8, which arrive with the driver in M1a. Nothing says which commit is built, from which checkout, or what "one turn" must return to count as a success.

The same missing entry point is needed twice more: step 2 of 20.1 asks for "a single-command setup and test entry point that passes on the default branch under the sandbox" (`SPEC.md:1633`) and BOOT-6 has that step done by hand at the start of M1b, before any `sf run` on the factory repository; and probe P-13 measures a cold build.

If the M0 row is cut into tickets as it stands, every ticket can be closed and the exit check still cannot be run.

**Proposed fix.** A decision, then one sentence in 22 and one in CLI-1. Name the entry point of the exit check and put it in the M0 scope: for example a command that creates one sandbox instance at the configured level with the per-run proxy, runs a given command line in a given directory, and reports the exit status and the proxy's denied-request log (`sf doctor --exec -- <command>`), plus what the two runs are (`cargo build --locked` of the tagged commit with an empty target directory and an empty `CARGO_HOME`; one `claude -p` prompt that must end in a `result` line). Say how the token reaches that command before `sf init` exists. The alternative decision is that the exit check is a script the operator runs against a development build, in which case BOOT-3's sentence changes.

**Confidence.** High. Searched for "exit check" (four hits: `SPEC.md:1264`, `1664`, `1765`, `1818`; none says how it is run), read section 17 in full, and read DEV-7, whose third row places "any real agent turn" on the server only as "The contract test, on a schedule" (`SPEC.md:1618`). The size of the fix is small; the severity is for what happens if the tickets are cut without it.

---

### B2 [severity: major] [milestone: M0] §13.3 (level `process`), 13.4 SEC-10, SEC-6, 19.1, 19.2

**Problem.** Two parts of the bubblewrap backend are named and not specified, and the M0 items "prototype of the bubblewrap sandbox" and "prototype of the egress proxy" meet exactly there, so two sessions would build halves that do not fit.

*The forwarder.* SEC-10 makes the only reachable endpoint "a Unix socket bound into the namespace with an in-sandbox forwarder (bubblewrap)" (`SPEC.md:1271`), and AG-11 gives a worker `HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY` (`SPEC.md:857`). Nothing says what program the forwarder is, who starts it inside the namespace beside the command (bubblewrap runs one command), how it relates to the process group that AG-11 makes the command the leader of, or how a worker reads it: SEC-6's read-only list ends with "a backend-defined OS baseline enumerated in code and covered by a snapshot test" (`SPEC.md:1260`) and names no forwarder and no `sf` binary. `socat` is on the server with "None stated" as its requirement (`SPEC.md:153`). The probe record says the question is open: "Nor is it known whether the in-sandbox forwarder of SEC-10 will need `socat` itself" (`docs/research/probes.md:837`). The proxy's protocol is not stated either (a CONNECT proxy is implied by `HTTPS_PROXY` and by the harness of P-22, and nothing says which ports, or what a plain HTTP request gets), and 19.2 has no row for it: "`reqwest`; `axum` 0.8 for the Unix-socket API and the gateway" (`SPEC.md:1561`).

*The seccomp filter.* The backend is "bubblewrap with PID, IPC, network and user namespaces and a seccomp filter" (`SPEC.md:1252`). Which system calls the filter refuses is said nowhere (the same probe record: "its seccomp filter ... 13.3 does not say"), and 19.2 names no crate that compiles one. The 19.4 Sandbox row and its snapshot test cannot be written against a filter that has no content.

*Ownership.* The forwarder has no crate. `factory-net` is "Egress proxy, model gateway | core" (`SPEC.md:1543`) and `factory-sandbox` may depend on `proc` and `core` only, so neither can hold code that needs the other; the supervisor shim is "`sf __supervise`, same binary" (`SPEC.md:866`), which suggests a second hidden subcommand, but nothing says so.

**Proposed fix.** Three decisions, each a paragraph under 13.3 or 13.4. (1) The forwarder: either a hidden subcommand of `sf` that is the first process in the namespace, starts the listener on loopback and then the command (then SEC-6 gains the `sf` binary read-only, and 19.1 says which crate holds it), or `socat` as part of the OS baseline (then 4.4 gains it as a requirement). (2) The proxy's protocol: CONNECT to an allowlisted host on port 443, everything else refused and logged; a 19.2 row for the crate. (3) The filter: the list of refused system calls, or the rule it is derived from, and the crate.

**Confidence.** High. Searched `SPEC.md` for "forwarder", "proxy", "seccomp", "socat" and "OS baseline", and the research records for the same words.

---

### B3 [severity: major] [milestone: M0] §9.2 WS-7, 9.5, 16.1, CFG-4, 4.4, 19.1

**Problem.** Where the host finds a toolchain is not specified, and the crate that looks is one that may not look.

WS-7 resolves `rust-toolchain.toml` "to an installed toolchain directory and puts that directory's `bin` first on the worker `PATH`" (`SPEC.md:686`). It does not say which directory, how its name is derived from channel, components and targets, or how presence is tested. WS-7a answers each of these for a check tool: "The host looks for a check tool at `<tools directory>/<name>/<version>/bin`" (`SPEC.md:689`), with a key in operator configuration. The operator layer lists "agent CLI path and version range, the tools directory (WS-7a)" (`SPEC.md:1404`) and no toolchain location, and CFG-4 rules out the place rustup would use by default: "The factory reads and writes no other path under a home directory unless the operator configuration names it." (`SPEC.md:1411`).

Four M0 things depend on the answer: the `rust` implementation of `Ecosystem`, the `sf doctor` check of "the toolchain", the server provisioning ("still includes an agent CLI and a toolchain (WS-7) that the factory's user can read", `SPEC.md:158`), which the operator does by hand and has to do to a layout, and the cold build of the exit criterion. M1a's claim check is the fifth.

9.5 puts "toolchain resolution and the worker `PATH` (WS-7)" (`SPEC.md:727`) behind `Ecosystem`, which "sits behind one interface, `Ecosystem`, in `factory-core`" (`SPEC.md:725`). `factory-core` is "(no I/O crates)" (`SPEC.md:1536`) and "Core defines domain data and effects only" (`SPEC.md:1549`). Resolving to an installed directory reads the disk. AG-2b solves the same problem for the object database by handing the pure function a lookup; nothing says that for the file system, and no crate of 19.1 is given the lookup. The same holds for the presence test of a check tool and for the "ecosystem-specific readiness checks at onboarding" of 9.5, which run commands.

**Proposed fix.** Text. Give WS-7 what WS-7a has: a key of operator configuration for a toolchains directory with no default, the directory name as a function of the parsed file (for the `rust` implementation, rustup's `<channel>-<host triple>`), and the presence test for the toolchain and each component. Say that the `Ecosystem` function is pure, from the parsed file to a directory name and a list of what must be in it, and name the crate that looks on disk (`factory-daemon`, or `factory-sandbox` since it mounts the result).

**Confidence.** High. Read every line of `SPEC.md` that holds "toolchain" (27 lines); D-27 (`SPEC.md:1764`) extended the claim rule to the toolchain and did not give it a location.

---

### B4 [severity: major] [milestone: M0, M1a] §16.1, 16.2, CFG-2, BUD-3, 9.4

**Problem.** The operator configuration has no keys. 16.1 describes what the file "Holds" in one table cell, 16.2 gives an example of the repository policy only, and CFG-2 makes the names binding: "Files are TOML, parsed with unknown fields rejected, and each has a `schema_version`." (`SPEC.md:1409`). The only keys of that file that the spec spells are `required_checks = [{ name, app_id }]` (`SPEC.md:1090`) and `min_free_bytes`.

M0's `sf doctor` reads the file (credential mode and files, the App's id and key, "sandbox availability at the configured level", the CLI path and version range, the tools directory, the repository allowlist), and M1a's `sf run` reads the same keys plus the data directory and the egress allowlist. `sf init`, which would write the file, is in no roadmap row, so through M1 the operator writes it by hand on the server from whatever the code turned out to accept. Each of these readers is a separate ticket.

One thing M1a needs has no home in either layer. BUD-3: "Model id, effort and a price table are per-stage configuration, loaded at runtime." (`SPEC.md:1348`), and the running spend figure that the session limit is compared with "is computed from the calls' token counts and the price table of BUD-3" (`SPEC.md:1349`). Neither 16.1's list nor the `[models]` table of 16.2 holds prices.

The policy has the same problem in two places. 16.2 is headed "Example policy", and BOOT-6 says of it only "What the factory repository's own policy holds is in BOOT-6." (`SPEC.md:1467`). Capture limits are policy keys by 9.4 ("Repository policy may lower them and cannot raise them above operator configuration.", `SPEC.md:711`) and session limits by 16.1 ("per-stage model and limits", `SPEC.md:1405`), and the example names neither. M1a builds the parser that rejects unknown fields, so M1a fixes schema version 1.

**Proposed fix.** Text. Add an example operator configuration to section 16 with every key M0 and M1 read, the price table among them, and say that it and 16.2 are the complete key sets of `schema_version = 1`. Add the capture and session-limit keys to 16.2 or say that they do not exist in version 1.

**Confidence.** High. Read section 16 in full and every line that holds "operator configuration" or "(operator)"; searched for "price".

---

### B5 [severity: major] [milestone: M0] §22 (M0 scope), 17 CLI-1, 10.3 AG-1, 23.2, 5.7, 21.1 BOOT-6

**Problem.** Three M0 items, "GitHub App creation; `sf doctor`; the probe suite (23.2)" (`SPEC.md:1725`), are each specified by text that reaches into M1a or M2, and nothing says which cut M0 builds.

*`sf doctor`.* CLI-1's list (`SPEC.md:1486`, quoted in B1) includes "the toolchain and the check tools for each repository", which needs the policy of each repository, and BOOT-6 says "until M1a has built the code that parses a policy nothing can tell whether it is valid (CFG-2)" (`SPEC.md:1687`). It includes "the capability probe", of which GH-3a says "From M2 the read is part of the capability probe (4.3)." (`SPEC.md:215`). It includes "CLI versions and flags", and AG-1 says how: "`sf doctor` verifies each flag by invoking the CLI with it against a stub model endpoint" (`SPEC.md:784`); the flag list is the driver's (M1a) and no crate or milestone is given the stub. It includes "App permissions" and "enforcement mode", which need App authentication and a GitHub client.

*The GitHub client.* No row of M0, M1a or M1b names the `factory-github` adapter or the gateway of 5.8. M0 needs one for `sf doctor`, for `sf app create` (the manifest flow of `SPEC.md:321` ends in an API call) and for the open probes.

*The probe suite.* 23.2 says "`sf doctor --probe` answers each against a scratch repository, except P-4 (WI-1b) and the part of P-28 that is read in the web interface" (`SPEC.md:1791`). Every probe answered so far was answered by a script or by hand, and many are not questions about a repository at all (P-6, P-13, P-21, P-22, P-24, P-27, P-29, P-30, P-31, P-34). Eleven M0 probes are open (P-2, P-3, the App part of P-5, P-9, P-10, P-11, P-12, P-13, P-14, P-17, P-20); most need an installation token, and P-2, P-11 and P-12 need a ruleset, a review and a merge on the scratch repository. A builder cannot tell whether "the probe suite" is thirty-odd probes re-implemented in Rust behind `--probe`, the eleven open ones, or the rows answered by scripts as before. The first reading is several weeks of work with no specification beyond each row's question.

**Proposed fix.** Decisions, recorded in 22 and CLI-1. (1) Which checks of CLI-1 the M0 `sf doctor` has (a natural cut: credential files and their modes, git and bubblewrap versions, user namespaces, the agent CLI's version and flags, sandbox availability), with the per-repository checks, the mode and the capability probe arriving with what they read. (2) Whether M0 builds App authentication and the minimal client in `factory-github`, and say so in the row. (3) What "the probe suite" is in M0: which probes `--probe` runs as code and which stay scripts, and that the exit clause "All M0 probes answered and recorded" is met by rows of 23.2 however they were answered. (4) Where the stub model endpoint lives.

**Confidence.** High for `sf doctor` and the client; medium for the probe suite, where the spec may intend the lighter reading and simply does not say. Searched the three rows for "github", "gateway" and "adapter", and read 23.2 row by row for which probes carry an answer.

---

### B6 [severity: major] [milestone: M0, M1b] §19.5 DEV-2, DEV-3, DEV-6, DEV-7, DEV-11, SEC-6, BOOT-6

**Problem.** Two steps of the check command have no defined outcome in a place where the command runs.

*(a) The sandbox step on the operator's Mac.* Every commit of M0 and M1 is made there ("Claude Code sessions on the operator's Mac write the code", `SPEC.md:1662`), through the pre-commit hook. On macOS the workspace compiles "with the sandbox backends compiled out" (`SPEC.md:132`), so the sandbox step at level `process` cannot run on the host. DEV-11 allows the third outcome in listed places only: "A step is not run only where the table of DEV-7 says that it does not run in that place, which it says of the sandbox steps alone (DEV-2a)." and "In every other place a step that cannot run fails by name" (`SPEC.md:1624`). The table's places are "| Checks | Hosted runners | Development VM | Server | A worker's sandbox |" (`SPEC.md:1614`): the Mac is not one of them. A step that runs no test may not pass either ("a test that was not run is never reported as passed", `SPEC.md:1620`). Read as written, the first commit after the sandbox step exists fails on the Mac. DEV-3 says the command "also runs clippy and the tests inside the development VM" (`SPEC.md:1608`), which is probably where the step is meant to run, but "also" leaves the host's own set whole.

*(b) `cargo deny check` without network.* DEV-2 lists "`cargo deny check`" (`SPEC.md:1604`). On a hosted runner the job "fetches the toolchain, the check tools, the dependencies, the advisory database" and "then runs the check in a network namespace that holds only a loopback interface." (`SPEC.md:1611`), so the step must run without fetching against a database at a known place. In a worker the implementer "runs every step but the two sandbox steps" (`SPEC.md:1705`), but the database is mounted for one stage only, "the host-fetched advisory database (verify only)" (`SPEC.md:1260`), and "`github.com` and `api.github.com` are never on a worker's list" (`SPEC.md:1272`). There the step can neither fetch nor read, and by DEV-11 it fails by name, on every run of the command that D-28 decided an implementer must be able to pass. The factory repository's verify list leaves `cargo deny` out (`SPEC.md:1696`), so nothing else brings the database to that sandbox. On the Mac and in the VM nothing says who fetches the database before a step that does not fetch. DEV-1's promise that "the local set and the CI set cannot drift apart" (`SPEC.md:1603`) needs one form of the step that works in all five places.

**Proposed fix.** Text for (a): add the Mac as a place in DEV-7, with the sandbox steps "not run on the host; run in the development VM (DEV-3)", so DEV-11 covers it. A decision for (b): the step's exact form (`--disable-fetch` and where the database lies), who fetches in an interactive session, and either SEC-6 gives the implementer's sandbox the database as it gives verify's, or DEV-7 and DEV-11 name the advisory part of the step as not run in a worker.

**Confidence.** High for both. For (a), read DEV-2a, DEV-3, DEV-7 and DEV-11 for a sentence that places the step in the VM alone, and found none. For (b), read every line that holds "advisor".

---

### B7 [severity: major] [milestone: M1a] §22 (M1a), 18.1, 8.1 ORCH-2 and ORCH-3, 8.4, 8.5, AG-15, BUD-4

**Problem.** M1a's store is "Store with `runs` and `events`" (`SPEC.md:1726`) and its exit criterion is that a run "reaches a draft pull request and survives an interrupted run" (`SPEC.md:1726`). M2 brings "outbox, activities and recovery" (`SPEC.md:1728`). What M1a records in between is not said, and resume rests on it.

- A killed `sf run` leaves a sandbox and processes. Recovery "Reaps every activity recorded as live: destroys its sandbox instance and sweeps for surviving processes (AG-18)." (`SPEC.md:610`), from what AG-15 wrote: "The sandbox record, pid, process group id and process start time are journalled before the first exec." (`SPEC.md:865`). ORCH-3 puts that in "an activity row (id, sandbox record, process data, state)" (`SPEC.md:557`). M1a has no `activities` table.
- Resume replays "journalled sub-steps (a journalled candidate SHA, a journalled push)" (`SPEC.md:611`), and 8.5 journals "Tree, parents, dates and message ... with the candidate SHA before any push" (`SPEC.md:623`). The `runs` row is "One row per run: repository, issue, stage, attempt, rounds, spend, deadlines, ids of branch, pull request and status comment." (`SPEC.md:1502`), with no candidate, and "the `events` table is an audit record and is never replayed to derive state" (`SPEC.md:551`).
- An external write is "Stored in the outbox with an effect id; delivered at least once, in FIFO order per run." (`SPEC.md:556`). M1a creates a ref, changes labels, pushes and opens a pull request with no outbox.
- "spend is rolled up per issue from M1a all the same (BUD-4)" (`SPEC.md:1352`) and "Spend is recorded per model call" (`SPEC.md:1349`); the `spend` table is not in M1a's store. 23.2 also imports probe answers "once the store exists (M1a)" into a `probes` table (`SPEC.md:1791`) that M1a's store does not have.

One builder adds `activities`, `spend` and a small outbox in M1a; another keeps two tables, runs writes inline under the keys of 8.5 and widens the `runs` row. M2's recovery, M1b's status comment and check runs, and the `state_version` conversion of 18.1 are built on whichever was chosen.

**Proposed fix.** A decision, written as an M1a cut of ORCH-3 and 8.4 (as VAL-14a is the M1a cut of 11.6): which tables M1a creates; where the activity record of AG-15, the journalled candidate and the per-call spend live until M2; whether external writes run inline under 8.5 in M1; and what `sf run` does at its start with a run whose last activity is recorded as live.

**Confidence.** High. Read 8.1, 8.4, 8.5 and 18.1 in full and BUD-7; CLI-3 (`SPEC.md:1488`) says only that `sf run` "resumes from the journal".

---

### B8 [severity: major] [milestone: M1a] §7.3, 8.6, BUD-7, 22 (M1a, M2)

**Problem.** What `sf run` does when a stage fails is specified for one failure class. In M1 there are no rounds and no escalation: the M2 row holds "escalation and resume; plan stage and gate; reviewer stage; in-place rounds" (`SPEC.md:1728`), and BUD-7 confirms that rounds and escalations "arrive with M2" (`SPEC.md:1352`). For a session limit BUD-7 then says exactly what happens: "`sf run` then interrupts the session, records which limit it was, reports it and exits with a failure." (`SPEC.md:1352`), with the restart and the handoff note that follow. BUD-2 points there: "Before escalation exists (M2), BUD-7 takes the place of the escalation." (`SPEC.md:1347`).

Every other outcome of M1a's stages still has only its full-pipeline handling:

- capture rejects a path: "Rejected paths → `implement`, verify round +1" (`SPEC.md:448`);
- integrity fails, or verify fails: "→ `implement`, verify round +1" (`SPEC.md:449`, `SPEC.md:450`);
- an agent failure: "Restart the stage once with a fresh session and the failure summary in the handoff; then escalate with `verification`" (`SPEC.md:640`);
- a result with status `needs_human` (`SPEC.md:495`), a stall, a detected loop, a Deterministic failure: each ends in an escalation, which sets a label and posts a comment that M1a does not have.

A builder cannot tell whether M1a returns to `implement` once (rounds without the name), exits and leaves the run at `implement` or at the failed gate, what the run row and the handoff hold for the next `sf run`, or what is written to the issue. The one real issue of the exit criterion will very likely fail verify at least once.

**Proposed fix.** A decision, as one item beside BUD-7: through M1, each failure class of 8.6 other than Transient ends `sf run` with a report; the item says the stage the run row keeps, what the host writes to `handoff/feedback/` so that the next `sf run` starts `implement` with the gate feedback of 10.4, that no round counter moves, and that nothing is written to GitHub. Or the decision that verify rounds are built in M1a, with the M2 row changed.

**Confidence.** High. Read every line of `SPEC.md` that holds "sf run" (16 lines) and every sentence that defers something to M2; only BUD-7 to BUD-9, the body cap of SEC-4 and the merge conflict of 12.3 say what `sf run` does at a failure.

---

### B9 [severity: major] [milestone: M1b] §22 (M1b, M2), 5.9, 7.3, 7.7, 8.2, 12.2, 12.3

**Problem.** M1b adds three waits, "CI wait; ready and evidence manifest; observation of human review and merge (strategy `human`)" (`SPEC.md:1727`), to a foreground command, and the loop that serves waits is M2: "poll and reconcile loop" (`SPEC.md:1728`). The waits are long: CI has "`ci.timeout_secs` from the push of the head SHA (default 3600)." (`SPEC.md:583`) and human review "None; one nudge (PR-3)." (`SPEC.md:584`).

Nothing says whether `sf run` stays in the foreground polling its one run until the operator merges, perhaps days later, or exits at a wait and is run again. 12.3 assumes the first without saying it: the test-merge runs "on every tick while the run is in `human_review` or `merge`" and "Before base update exists (M2), `sf run` reports the conflict in the status comment and stops; the operator cancels the run and approves the issue again." (`SPEC.md:1152`). Nothing says which rows of 7.7 M1b's command evaluates while it waits (the merged pull request it must; issue closed, `state:triage`, hold, a foreign commit?).

The two outcomes of those waits that are not the happy path lead to rounds, which M1b does not have (B8): "Red check → `implement` with the failing log excerpt, CI round +1." (`SPEC.md:452`) and "Changes requested → `implement`, human round +1" (`SPEC.md:455`). On the factory repository CI runs `cargo xtask check` whole, mutation step included, while verify runs four commands, so a red check on a head that passed verify is the expected case, as BOOT-6 itself says. The exit criterion, five issues merged from factory pull requests, depends on what the operator can do then, and only the conflict case is written down.

**Proposed fix.** A decision in 12.3 or CLI-3: the lifetime of `sf run` in M1b (stays and polls as GH-18 for its one run, or exits and resumes), the rows of 7.7 it evaluates, and what a red required check and a "changes requested" review do before rounds exist (most simply the rule already given for a conflict: report in the status comment, stop, cancel and approve again, or the M1 rule of B8).

**Confidence.** High. Read 5.9, 7.7, 12.1 to 12.3 and the deadline table of 8.2.

---

### B10 [severity: major] [milestone: M1a, M1b] §6.2, 6.4, 10.5, 11.6, 19.1, 22

**Problem.** Acceptance criteria are used from M1a on, and neither their written form nor the milestone that builds their parser is specified.

The form is one table cell: "A list `AC-1 … AC-n`. Each has a statement in the form *WHEN ⟨condition⟩ THE SYSTEM SHALL ⟨behaviour⟩* and a verification method: `test`, `property`, `review` or `manual`." (`SPEC.md:375`). WI-1 specifies sections with care ("a tolerant section parser keyed on heading text", `SPEC.md:381`) and stops at the section's text. How an id, a method and a statement stand in a line of that text, whether a criterion may span lines, and what an unparsable line is, are not said. The one sample is in a probe record, not in the spec (`docs/research/probes.md:926`: "- AC-1 (review): WHEN ...").

M1a needs the parsed list. "Every agent stage ends with a schema-validated JSON result." (`SPEC.md:820`) and "A result with `status: success` and any non-`manual` criterion `not_met` is schema-invalid." (`SPEC.md:836`): the host has to know the ids and which are `manual`. M1b needs it again for the manifest, "A table: each acceptance criterion → result → evidence → verification method." (`SPEC.md:1119`), and for VAL-14.

No row of M0, M1a or M1b names the issue parser. The only mention is M3, "Issue forms and readiness lint enforced" (`SPEC.md:1729`), and the claim says "Run the readiness lint on the snapshot. Until the lint is enforced (M3, 22)" (`SPEC.md:567`), which leaves open whether M1a builds the parser and the lint and ignores the result, or builds neither. If neither, the result schema of M1a cannot be checked as 10.5 says.

**Proposed fix.** Text and a decision. Text: the grammar of a criterion in 6.2 (for example one list item per criterion, `AC-<n> (<method>): <statement>`, ids unique and in order, what a line that does not parse is), with the P-4 bodies as fixtures as WI-1b already has them. Decision: M1a builds the section parser and the criterion parser of `factory-core` and checks the result against them, with the lint's other rules unenforced until M3; or M1's result check leaves criteria out and the M1b manifest shows them as the agent returned them, said in 10.5 and VAL-14a.

**Confidence.** High. Searched for "verification method", "AC-" and "issue parser"; read 6.2, 6.4, 10.5 and 11.6.

---

### B11 [severity: major] [milestone: M1a] §19.4 (GitHub, End to end), 21.1 BOOT-3, 21.2, 19.1, 22

**Problem.** The test layers that M1's GitHub code is to be built against need a fake that no milestone builds. 19.4: "`fake-github`: a stateful HTTP fake implementing the REST and GraphQL subset the gateway uses" (`SPEC.md:1596`) and "With the fake agent against `fake-github` on every pull request." (`SPEC.md:1598`). 21.2 gates every pull request on "Harness tests | Section 19.4 with the fake agent" (`SPEC.md:1712`), and BOOT-3 gates every install of M1 on them: "the gate is the harness tests of 19.4 passing on the tagged commit" (`SPEC.md:1664`). The crate exists for it ("`factory-testkit` | Fake agent binary, `fake-github`, fixtures", `SPEC.md:1546`).

The M1a row names the fake agent and not `fake-github`; no row of M0, M1a, M1b or M2 does. For the claim of 8.2 alone the fake has to hold issues with labels and their `labeled` events, edit history and rename events, comments with `isMinimized`, the permission endpoint, the label list, ref creation with the 422 behaviour of P-1, and a push target; publish adds pull requests, M1b adds check runs, comments and reviews. That is weeks of work, and the builder of M1a's claim or publish ticket cannot tell whether it is theirs, someone's before them, or not due until M2's process-crash test.

The same is true in smaller form of which 19.4 layers exist at the end of each milestone. Engine, Git and Sandbox are placed by the M0 row and D-28; Path patterns, Instruction set, Snapshot and Outbound text by sentences such as "M1a builds both"; the GitHub, End-to-end and Contract layers are placed nowhere, though M1a's session limits rest on stream shapes that the contract test is to assert (BUD-10).

**Proposed fix.** A decision in 22: put `fake-github` in a row, with the subset it must serve at that point (the calls of M1a's claim and publish), and the end-to-end test with it; or say that through M1 the adapter is tested with `wiremock` and the scratch repository, that `fake-github` is M2 work, and what BOOT-3's "harness tests of 19.4" consist of during M1. Add a "Since" column to the table of 19.4, as 11.1 has.

**Confidence.** High. Searched for "fake-github", "fake agent", "testkit" and "harness tests".

---

### B12 [severity: major] [milestone: M1a] §11.3 (rule table), 22 (M1a), 13.5

**Problem.** M1a builds "integrity with path, structural and secret rules, including the matcher of VAL-5b to VAL-5d and the literal-token scan" (`SPEC.md:1726`). The path rules have several pages (VAL-5b to VAL-5g). The secret rule has one table row: "| Secret pattern in added content | `fail` |" (`SPEC.md:926`). No section says what a secret pattern is: which secrets, which expressions, whether the set is built in or configured, what "added content" is for a binary file or a renamed one, how a hit is reported without writing the secret to GitHub, and which crate matches (19.2 names none, and it lists the crates that are not used). The literal-token scan is a different thing and is specified ("the host searches every candidate and every text written to GitHub for its literal value and fails the stage on a hit", `SPEC.md:1282`). A `fail` returns to `implement` (B8), and 15.2 and 15.3 count "secret-scan hits", so the rule's content is not a detail of one function.

Three other rows of the same table are placed in no milestone. "Structural rules (file type, file size, nested repositories, submodule entries) are enforced earlier, in capture (9.4)." (`SPEC.md:929`), so "path, structural and secret" does not cover the attribute rule ("A changed path carries an attribute that hides or alters its diff", `SPEC.md:922`), the binary rule ("| A binary change to a path that was text at base | `flag` |", `SPEC.md:923`) or the size budget ("First time: `fail` with a request to split (`size_strikes` +1). Second time: `flag`.", `SPEC.md:924`). The size budget is read by M1b at the latest, since the factory repository's policy anchors `lockfiles` "because a match exempts a file from the size budget (VAL-5b)" (`SPEC.md:1701`), and its first outcome is a round.

**Proposed fix.** A decision and a line of text. Decision: what the secret rule is in M1a: either the literal-token scan alone, with the pattern rule moved to a named later milestone, or a stated set (the forms, where they are listed, the matcher). Text: add the attribute rule, the binary rule and the size budget to the M1a or the M1b row by name.

**Confidence.** High for the secret rule (every line holding "secret" was read; `09-spec-review.md:464` asked for the rule to be in scope and did not ask what it is). Medium for the three unplaced rows: a reader could take "path rules" to mean the whole table but for the secret row, and the M1a row's care in naming "the two that match a path alone" argues against that.

---

### B13 [severity: minor] [milestone: M1b] §19.1, 19.5 DEV-2, VAL-3a, BOOT-4, BOOT-6, 9.5

**Problem.** The onboarding pull request gives the structural test a part that the crate table does not let it have. "On the factory repository the structural test makes that comparison (DEV-2)." (`SPEC.md:939`): the last lines of `CODEOWNERS` against the translation of three lists, one of which is the built-in protected-path list of 11.3. The structural test lives in `xtask` ("and the structural test (19.1)", `SPEC.md:1671`; the file is `xtask/tests/structure.rs`). The table gives `xtask` no workspace dependency, "| `xtask` | Developer automation | — |" (`SPEC.md:1547`), the test file holds `("xtask", &[])` at its line 52, and "dev-dependencies may additionally include `factory-testkit`" (`SPEC.md:1549`) and nothing else. The built-in list and the rule of translation are code of `factory-core`. So the session that writes the onboarding pull request either copies both into `xtask`, which is a second copy of a list the test exists to keep in step, or adds a dependency that the test it is editing forbids. BOOT-6 already puts one policy test elsewhere ("a test that parses the policy with the policy types of `factory-core` (19.1)", `SPEC.md:1689`), which shows the way out but is not applied to the comparison.

A second structural test is asserted and not described: "The pipeline, sandbox, GitHub model and gates contain no Cargo-specific logic outside this interface, and a structural test enforces that." (`SPEC.md:735`). What it inspects, and in which milestone it arrives, is not said; M0 builds the interface.

**Proposed fix.** Text. Either allow `xtask` a dev-dependency on `factory-core` (table, `structure.rs` and DEV-2 change together), or say that the comparison of VAL-3a is a test of `factory-core` that reads the repository's own policy and `CODEOWNERS`, as the parse test does. For 9.5, say what the test checks (for example: no crate but `factory-core`'s `rust` module names a Cargo file or command) or drop the sentence.

**Confidence.** High. Compared the table with `xtask/tests/structure.rs` line by line; they agree, which is what makes the collision certain.

---

### B14 [severity: minor] [milestone: M0] §19.2, 19.3, `Cargo.toml`, `clippy.toml`

**Problem.** Two places where 19.2 and 19.3 do not carry a session that has only the spec.

*Pins and missing rows.* "Pre-1.0 and fast-moving crates are pinned exactly, and the pinned set MUST resolve (`cargo generate-lockfile`) as an M0 task." (`SPEC.md:1553`). Several pre-1.0 rows give no exact version: "`octocrab` 0.54.x, exact pin" (`SPEC.md:1560`) leaves the patch open, "`rusqlite` 0.43 (`bundled`)" (`SPEC.md:1562`) and `axum` 0.8 give two components, and `reqwest`, `toml`, `wiremock` and `insta` give none (`SPEC.md:1561`, `1563`, `1570`). `toml` reads the files of a gate (VAL-5g), where the argument 19.2 makes for the `icu` pins applies. `AGENTS.md:39` tells a session that 19.2 "names the crate chosen for each concern", and concerns of M0 and M1a have no row: SHA-256 (snapshot hashes, the chain of `events.jsonl`), UUIDv7 run ids, system calls (the `lstat` walk of 9.4, process groups, the sweep of AG-18), the seccomp filter and the proxy (B2), JSON Schema validation of a stage result, and time. Nothing says what a session does then. The resolve task itself is in no roadmap row.

*Lints in test code.* "Workspace lints deny `unwrap_used`, `expect_used`, `await_holding_lock`" and "`xtask` is exempt." (`SPEC.md:1578`). The check runs clippy with `--all-targets`, and `clippy.toml` sets neither `allow-unwrap-in-tests` nor `allow-expect-in-tests`, so both lints bind in the tests of every `factory-*` crate. The only tests that exist are `xtask`'s, which use `expect` freely under the exemption (`xtask/tests/structure.rs:91`). The first M0 ticket that writes a test in `factory-core` has to choose between tests that return `Result`, a module-level `allow`, and a change to a protected configuration file, and each session will choose again.

**Proposed fix.** Text. Give each pre-1.0 row its exact version at the change that first adds the crate, or say that the lockfile is the pin and reword the sentence. Add the missing rows or a rule ("a concern without a row is decided in the pull request that first needs it and added to this table there"). Add the resolve task to the M0 row. In 19.3, say whether the two lints cover test code and set `clippy.toml` to match.

**Confidence.** High on the facts; the blast radius of each is small.

---

### B15 [severity: minor] [milestone: M0] §22 (M0), 21.1 BOOT-3, 19.2 (Release), VAL-6, VAL-7

**Problem.** "the first tagged release build (BOOT-3)" is an M0 item whose whole specification is one stack row, "| Release | `cargo-dist` 0.34 with attestations; `cargo-auditable` | |" (`SPEC.md:1572`), and BOOT-3's sentence about when the tag is cut. Not said: what builds it (a workflow on a tag, or the operator's machine), for which targets (the server is x86-64; the development VM is arm64 on the operator's Mac), where the artifact is published, how the operator fetches it, verifies the attestation and installs it, and where an installed binary and its predecessor lie, although "Previous binaries and store backups are kept for rollback (18.1)." (`SPEC.md:1664`) from the first upgrade in M1 and M2's `sf self-update` reads the same layout. A release workflow cannot follow the rules given for the CI workflow, which has "top-level `permissions: contents: read`, no secrets" (`SPEC.md:1082`), since attestations and a release need write scopes; VAL-7 asks that "Release and deployment workflows MUST live behind protected environments" (`SPEC.md:1089`), which is a setting the operator creates and DEV-8's ruleset for `main` does not cover tags. DEV-7 adds a step nobody has provisioned for: the check set is "Run by the operator on a tagged commit before that build is installed" (`SPEC.md:1616`) on the server, where 4.4 records no toolchain and no check tools for the operator's account.

**Proposed fix.** A decision, as a DEV item beside DEV-5: the trigger and the runner of the release build, its targets, its permissions and environment, where the operator installs it (a path outside any home directory, with the version in it), and whether the server run of DEV-7 applies from `v0.1.0` (and then what the server needs for it).

**Confidence.** Medium. `cargo-dist` generates a workflow with sensible defaults, so a builder is not stuck; the open points are those that later work reads. Searched for "cargo-dist", "release", "tagged" and "attestation".

---

### Buildability: roadmap items and where they are specified

Exit criteria first, then each scope item in the order of its row.

**M0**

- Exit, "All M0 probes answered and recorded": 23.2. Testable (a row carries its answer). Eleven M0 probes are open; most need the App. `partly` (B5)
- Exit, "the hostile-tree suite passes": WS-9, 9.4, 19.4 Git. Testable. `specified`
- Exit, cold build and one agent turn on the server: 22, BOOT-3, SEC-6b, SEC-10, SEC-11. No entry point, no procedure. `missing` (B1, B2, B3)
- Workspace: 19.1; built, and `xtask/tests/structure.rs` agrees with the table. `specified`
- The check command with its pre-commit hook: DEV-1, DEV-2, DEV-2a, DEV-9, DEV-10, DEV-11; `xtask/src/check.rs` holds the three steps DEV-1 says exist. The programs of the unused-dependency check and the link check are left to the builder on purpose. `partly` (B6)
- The development VM: DEV-3, DEV-4, DEV-7. How the VM gets the toolchain and the check tools is left to the builder; nothing else depends on it. `specified`
- CI with its ruleset: DEV-5, DEV-6, DEV-7, DEV-8, VAL-6, P-16. `specified` (B6b for one step)
- The first tagged release build: BOOT-3, 19.2 Release row. `partly` (B15)
- The Linux server provisioned: 4.4, WS-7, WS-7a, D-16. `partly` (B3)
- `factory-core` types: 19.1, 8.1, 5.4, 16. No list of states, events or effects; the builder's to draw. `partly`
- The `Ecosystem` interface with its `rust` implementation: 9.5, WS-7, VAL-5e. No signature, and no word on which of its seven parts M0 builds (three belong to M1b, M4 and M5). `partly` (B3)
- The engine skeleton with property tests: ORCH-1 to ORCH-5, 19.4 Engine. "Skeleton" is not bounded. `partly`
- `factory-proc` with the supervisor shim: AG-11 to AG-16, AG-18, 19.3. `specified`
- Git wrapper: WS-8, WS-1, WS-10, WS-11. `specified`
- GitHub App creation: 5.7, GH-11, GH-12, `sf app create`. Whether M0 builds the command or the operator creates the App by hand, and where the key lands, is not said. `partly` (B4, B5)
- `sf doctor`: CLI-1, AG-1, AG-5. `partly` (B5)
- The probe suite: 23.2, CLI-1. `partly` (B5)
- Prototypes of capture, ingest and export: 9.4, WS-9, WS-11, P-7. Whether the M0 prototype is the code of `factory-git` that M1a's "capture" then uses, and whether the suite runs the staging git inside the sandbox, is not said; P-7 already was a prototype "without the factory's sandbox" (`SPEC.md:720`). `partly`
- Prototype of the bubblewrap sandbox: 13.3, SEC-6 to SEC-8, 19.4 Sandbox, DEV-2a. `partly` (B2)
- Prototype of the egress proxy: SEC-10 to SEC-12, AG-11, AG-19. `partly` (B2)
- Not in the row and due in M0: the pinned set resolves (19.2). `missing` from the row (B14)

**M1a**

- Exit, one real issue on the scratch repository to a draft pull request, surviving an interrupted run: testable. `partly` (B7, B8)
- Store with `runs` and `events`: 18.1, ORCH-2, ORCH-3. `partly` (B7)
- Claim, with the label read of GH-3a: 8.2, 5.3, GH-3a, SEC-1, SEC-2. That M1a's claim ends before the status comment follows from the M1b row only. `specified`
- The check for the toolchain and the check tools: WS-7, WS-7a, 8.2. `partly` (B3)
- The snapshot of SEC-3 and SEC-4: 13.2, D-35, 19.4 Snapshot. `specified`
- Implement at level `process` with the egress proxy: 13.3, 13.4, 10.6. `partly` (B2)
- Fake agent: 19.4 Drivers. `specified`
- The Claude driver in `subscription` mode: 10.1, AG-1 to AG-8, 13.5, P-21, P-22, P-26. The tool allowlist of each role and the value of `--permission-mode` are not given (`SPEC.md:756`, `SPEC.md:784`); the probes used `dontAsk`. `partly`
- The instruction set of AG-2a and AG-2b: 10.3, 19.4 Instruction set. `specified`
- The variables of AG-11a: 10.6. `specified`
- Per-session limits: BUD-6 to BUD-10, AG-14, ORCH-4. `specified` (the price table's home: B4)
- Capture: 9.4, WS-11. `specified`
- Integrity: 11.3, VAL-3 to VAL-5f. `partly` (B12)
- Verify on the export: 11.2, VAL-1, 9.4 Export, WS-4. `specified` (its failure: B8)
- The outbound encoder and the form of a path: GH-5a, GH-5c, 19.4 Outbound text. `specified`
- Publish a draft pull request with the body of VAL-14a: 9.4 Publish, 5.6, 8.5, VAL-3. `specified`
- "then exit" and resume from the journal: CLI-3, 8.4, 8.5, BUD-7. `partly` (B7, B8)
- Not in the row and needed: the `factory-github` adapter and as much of the gateway of 5.8 as one run needs; policy and operator-configuration parsing (16); the issue and criterion parser (B10); `fake-github` (B11); preparing the scratch repository and the VM (BOOT-6 says it is M1a work). `missing` from the row

**M1b**

- Exit, five real issues on the factory repository merged by the operator from a tagged build: testable. `partly` (B9)
- The factory repository onboarded by hand: BOOT-6, BOOT-4, 20.1, VAL-3a, DEV-2, D-37. `specified` (B13 for one test; step 2 of 20.1 by hand needs the entry point of B1)
- The dependency rule and the reading of `package.build`: VAL-5e, VAL-5g, 19.4 Dependency rule. `specified`
- Status comment and marker: 5.4, GH-4 to GH-7. Its human-readable part and DEP-1 need the mode detection of 4.1 and DEP-4, which no row names. `partly`
- Factory check runs: 5.5, GH-8 to GH-10, VAL-2, 8.5. `factory/review` is `skipped` as "not yet implemented" (`SPEC.md:303`). `specified`
- CI wait: VAL-8, VAL-9, 8.2 deadlines, GH-18. `partly` (B9)
- Ready and evidence manifest: 7.3 `ready`, 11.6, VAL-14. `partly` (B10)
- Observation of human review and merge, strategy `human`: 12.2, 12.3, 7.7. `partly` (B9)

### Buildability: covered

Read in full: `SPEC.md` sections 3, 4, 5.2 to 5.9, 6, 7.1 to 7.3, 7.5 to 7.7, 8, 9, 10, 11.1, 11.2, the rule table of 11.3 with VAL-3 and the start of VAL-3a, VAL-14a, 12.1 to 12.3, 13.2 to 13.5, 13.8, 14, 16, 17, 18, 19, 20, 21, 22, 23.3; of 23.1 the rows D-22, D-28 and D-36. Code and configuration: `xtask/src/check.rs`, `xtask/src/main.rs`, `xtask/tests/structure.rs`, `xtask/Cargo.toml`, the root `Cargo.toml`, `clippy.toml`, `mise.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, `.githooks/pre-commit`, `.claude/settings.json`, and the manifests of `factory-core`, `factory-daemon` and `factory-testkit`. `AGENTS.md`. The first screen and the rust-scope lens of `docs/research/10-spec-recheck.md`.

Sampled: 11.3 from VAL-3a to VAL-5g (the start of each item and every table), 11.4 to 11.7, 13.1, the other rows of 23.1 (first 500 characters each), 23.2 (each row for whether it carries an answer; P-5 and P-13 to the end), `GLOSSARY.md` (each entry's first lines), `docs/research/probes.md` (the open points of P-6 and the lines found by search), `docs/research/09-spec-review.md` (by search, to check that no finding repeats one).

Not read: 1, 5.1, 7.4, 12.4, 13.6, 13.7, 15 and the appendices beyond search hits; `xtask/src/hook_skip.rs`, `xtask/src/pre_tool_use.rs` and the two hook test files; `docs/research/05-rust-implementation.md` beyond one search.

Compared and found consistent: the crate table of 19.1 with `xtask/tests/structure.rs`; DEV-1's account of what exists with `xtask/src/check.rs`; DEV-9 with `.githooks/pre-commit`; DEV-10 with `.claude/settings.json`; 19.1's toolchain pins with `mise.toml` and `rust-toolchain.toml`.
