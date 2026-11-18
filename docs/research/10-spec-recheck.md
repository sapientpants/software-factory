# Recheck of SPEC.md draft 2

One checker per review lens compared draft 2 with the findings in `09-spec-review.md` and looked for problems the rewrite introduced. Draft 3 addresses each item below.

## Lens: consistency (13 earlier findings judged fully resolved)

### C1 [new_contradiction] [major] new §7.5 (last paragraph) vs 7.7 rows 1 and 3, Glossary "Run", 12.3

**Problem.** Introduced by the K0 fix, breaks the K4 fix. 7.5: "While a run is in `needs-human`, the reconciler rows of 7.7 are not evaluated for it, except issue closure, hold and stop. They are evaluated once at resume." The excepted closure row is "Issue closed and the pull request is not merged". So when a human merges the pull request of an escalated run, row 1 ("The run's pull request is merged | Enter `done`, whatever the merge strategy and whoever merged") is never evaluated, and the closure row is false because the pull request is merged. The run stays non-terminal for ever (no `done`, branch and run directory never cleaned up), although the Glossary says `done` = pull request merged. Likewise a human moving an escalated issue to `state:triage` is not evaluated, although the Glossary lists it as a cancellation.

**Proposed fix.** Replace the paragraph in 7.5 with: "While a run is in `needs-human`, only these rows of 7.7 are evaluated on every tick: the run's pull request is merged (the run enters `done`); issue closed and the pull request is not merged; `state:triage` applied by a human (the run is cancelled); `factory:hold`. The stop of 13.7 also applies. All other rows are evaluated once, at resume (7.5.1)."

### C2 [partly_resolved] [major] K0 §7.5.1 steps 2 to 6, 5.3 rule 6

**Problem.** The resume procedure cannot be executed in the order written. (a) Step 3 "Adds `state:in-progress` ... and removes the other state labels" and step 5 "grants one more issue-budget increment" run before step 6, whose third case says "no resume. `state:ready` is removed and the run stays in `needs-human`". By then the labels are already swapped, the counter is zeroed and the budget grant is spent. (b) Step 2 "Re-evaluates readiness rules 2–4 and 6 of 5.3" gives no failure action for a resume. The only stated action for rule 6 is 5.3's "removes `state:ready`, sets `state:triage`", which puts a non-terminal run with an open pull request on a triage issue; 7.2 has no `needs-human` -> `triage` row and 7.7 cancels only on a human-applied `state:triage`. The action for a lint failure (6.4 says "A lint failure creates no run") is also undefined when the run already exists.

**Proposed fix.** Replace steps 2 to 6 of 7.5.1 with:
"2. Re-evaluates readiness rules 2–4 and 6 of 5.3. Rule 2 (hold) or rule 3 (open blocker) not met: the issue keeps `state:ready` and the resume waits. Rule 4 (lint) fails: the factory removes `state:ready`, leaves `state:needs-human`, sets the category to `clarification` and posts the lint comment of 6.4. Rule 6 fails (title or body edited after the `labeled` event): the factory removes `state:ready`, leaves `state:needs-human` and says so once. On a resume the factory never sets `state:triage`.
3. Chooses the re-entry stage:
   - Title and body unchanged (`title_body_sha256` matches): the recorded stage. A new trusted comment alone never forces a re-plan.
   - Title or body changed, and every edit since the last snapshot is by an authorised actor: `plan`, and any plan approval is void. If the recorded stage is `human_review` or `merge`, the change is noted and the run resumes at the recorded stage.
   - An edit by anyone else, or by an editor that cannot be identified: no resume. `state:ready` is removed, the run stays in `needs-human` with category `spec-changed`, and steps 4 to 7 are not performed.
4. Waits for a slot unless the re-entry stage is `human_review` or `merge`. The issue keeps `state:ready` while it waits.
5. Adds `state:in-progress` (or `state:in-review` if the re-entry stage is `human_review` or `merge`) and removes the other state labels.
6. Takes a new snapshot.
7. Sets the round counter of the gate that escalated to 0. For category `budget` it grants one more issue-budget increment (section 14) and counts it in `budget_grants`."

### C3 [new_contradiction] [major] new §13.2 SEC-1 and SEC-1a vs 7.4 PLAN-1, PLAN-2 and `decompose`, 6.3

**Problem.** Introduced by the K11 and K12 fixes together. SEC-1 now covers `risk:*` and `type:spec-delta` and says: "A tier, risk or type label that fails the check is read as its most restrictive value (`risk:high`, plan gate required, not `type:ci`)", where the check requires "an actor of type User". SEC-1a exempts only "A `state:ready` applied by the factory App". The `risk` value a human approved in the plan (PLAN-1: "key, title, risk, blocked-by edges, full body") and the `type:spec-delta` label of 6.3 are applied by the App, so they fail SEC-1: every sub-issue is read as `risk:high` and stops at its own plan gate, and the spec-delta issue is not treated as one. The `decompose` row also never says which labels it applies. Separately, SEC-1a requires that the "parent's plan approval is valid (PLAN-2)" at the sub-issue's claim, but PLAN-2 says "The factory removes the label when it consumes an approval" and SEC-1 honours a label only "with no later `unlabeled` event", so re-running the check at that point fails for every sub-issue.

**Proposed fix.** Append to SEC-1a: "On such a sub-issue the `risk:*` label and, for the spec-delta issue, the `type:spec-delta` label that the factory applies at creation are honoured on the same conditions, with the values shown in the approved plan comment. A later change to either is judged by SEC-1. The parent's approval is checked from the parent's marker, not from the `factory:plan-approved` label (which the factory removed when it consumed the approval): `plan_approved_by` is set, the plan comment's current SHA-256 equals `plan_sha256`, and the parent's title and body are unchanged since that approval's `labeled` event."
In SEC-1 change "Each is honoured only if" to "Except as SEC-1a provides, each is honoured only if".
In the 7.4 `decompose` row, after "create the sub-issues", insert: "(labels: `state:ready`, `factory:origin`, the plan's `risk:*`, and `type:spec-delta` on the issue marked `spec_delta`)".

### C4 [new_contradiction] [major] new §5.4 GH-5a and INV-9 vs 7.4 PLAN-1, PLAN-4 and `decompose`, 6.2 WI-1, 5.3 rule 4

**Problem.** The K9 fix (plan comment carries every sub-issue body; `decompose` creates issues "by parsing it, never from agent output or a local copy") collides with GH-5a: "All text that originates from an agent ... and is written to GitHub passes through one outbound encoder. It caps length, renders the text inside a fenced block labelled as agent output, and neutralises `<!--`, `-->`, @mentions, issue references and closing keywords. Agent text never starts a body." A sub-issue body is planner text. If it is written through the encoder, the issue body is a fenced block, and the section parser "keyed on heading text" (WI-1) and the readiness lint at dispatch (5.3 rule 4) no longer see the form's level-3 headings, although PLAN-4 linted the raw text. If it is written unfenced, GH-5a and INV-9 are violated. "caps length" also conflicts with PLAN-1's "full body". The spec does not say which form is linted, posted, hashed into `children[].body_sha256` or created.

**Proposed fix.** Add after PLAN-1: "**PLAN-1a.** Sub-issue titles and bodies are the one place where agent-proposed text becomes an unfenced issue body, because a human approves exactly that text. The host normalises each proposed title and body once, before linting and posting: it applies the neutralisation of GH-5a (not the fencing) and the 32 KiB cap of 6.4; a body over the cap is a planner failure. PLAN-4 lints the normalised form. The plan comment shows each normalised body verbatim inside a fence longer than any backtick run it contains. `decompose` extracts that text byte for byte and creates the issue with the host-written marker line (8.5) followed by the text, unfenced, so the section parser (WI-1) and the readiness lint apply as they do to a human-written issue. `children[].body_sha256` is the hash of the body as created."
Append to GH-5a: "Sub-issue titles and bodies created by `decompose` follow PLAN-1a instead of the fenced rendering."

### C5 [new_gap] [major] new §7.7, 7.6, 7.5 / 7.5.1, 5.2 `factory:restart`

**Problem.** `factory:restart` has no trigger rule. 5.2 says "Resets the run (7.6). Removed by the factory once acted on" and 7.6 says "A reset happens on `factory:restart`", but the reconciler table in 7.7 has no row for it, and 7.5 says the 7.7 rows "are not evaluated" for a run in `needs-human`, which is where a human most often wants a restart (after a `merge-conflict`, `verification` or closed-pull-request escalation). It is undefined whether `factory:restart` alone leaves `needs-human`, whether `state:ready` is also needed, what the public state becomes for a run that was `in-review`, and what happens to an unauthorised label.

**Proposed fix.** Add a row to 7.7 after the `factory:hold` row: "| `factory:restart` applied | If valid under SEC-1: reset (7.6) and remove the label; if the attempt cap is reached, escalate instead (7.6). If the public state was `in-review`, set `state:in-progress`. If invalid: remove the label and ignore it. |"
Add to the end of 7.5.1: "`factory:restart` on a run in `needs-human` is acted on at resume: the human applies `factory:restart` and `state:ready`, and after the label swap and new snapshot the factory performs the reset of 7.6 instead of re-entering the recorded stage. Without `state:ready` the label has no effect."

### C6 [new_contradiction] [minor] new §7.7 (title or body rows) vs 7.4 tier 2 parent pipeline

**Problem.** The K12 fix added the parent pipeline but 7.7 was not adjusted for it. "Title or body changed by an authorised actor before the `ready` stage | Interrupt, take a new snapshot, restart from `plan` in place." A tier 2 parent has no `ready` stage, so an edit to a parent in `await_children` restarts it from `plan`; the new plan has a new `plan_sha256`, so the sub-issue keys of 8.5 (`<run>/<plan sha256>/<key>`) do not match and a second set of sub-issues is created beside the first. 7.4 forbids exactly this for the label route: "`factory:restart` on a parent in `await_children` is rejected with a comment."

**Proposed fix.** In 7.7 change the first cell of the row to "Title or body changed by an authorised actor before the `ready` stage (tier 2 parent: before `decompose`)" and change the row "Title or body changed after the pull request is ready" to "Title or body changed after the pull request is ready, or on a tier 2 parent in `decompose` or `await_children` | Note it in the status comment; no automatic action."

### C7 [partly_resolved] [minor] K5 §11.4 VAL-8, 7.3 `ci` row, 8.6

**Problem.** The failure path for a required check that never reports is still ambiguous. VAL-8: "it fails on any other conclusion (CI round +1); a check still missing at `ci.timeout_secs` counts as failed with class `infrastructure`." 8.6 has no class called `infrastructure` (it is an escalation category), and the only `ci` failure in 7.3 is "-> `implement` with the failing log excerpt, CI round +1". Read literally, a path-filtered or disabled workflow sends the run back to the implementer twice with no log and then escalates. On resume the recorded stage is `ci`, whose deadline ("from the push of the head SHA") has already passed, so it would fail again at once.

**Proposed fix.** In VAL-8 replace the last clause with: "a check still missing at `ci.timeout_secs` is not a CI round: the run escalates with category `infrastructure` and does not return to `implement`. When such a run resumes, the `ci` deadline restarts from the resume."
In 7.3 change the `ci` "On failure" cell to: "Red check -> `implement` with the failing log excerpt, CI round +1. Required check missing at the timeout -> escalate with `infrastructure` (VAL-8)."

### C8 [new_gap] [minor] new §8.4 step 4 (rebuild)

**Problem.** The rebuild rule added for K7 does not cover every stage. "A run whose marker stage is `ci`, `ready`, `human_review` or `merge` keeps that stage. A run rebuilt in an agent or deterministic stage restarts at `plan`". By the kinds in 7.3 and 7.4, the plan gate and `await_children` (wait), `decompose`, `publish` and `claim` (host) and `capture` ("host + sandboxed git") are in neither list, and a run whose marker has a non-null `escalation` is not mentioned. A tier 2 parent rebuilt in `await_children` has no rule; if it is restarted at `plan` it decomposes a second time, although `children` was put in the marker to avoid that. "The approver is re-read from the latest `labeled` event for `state:ready`" also yields the App for a sub-issue (SEC-1a).

**Proposed fix.** Replace the third bullet of 8.4 step 4 with: "A run whose marker stage is `ci`, `ready`, `human_review` or `merge` keeps that stage. A tier 2 parent whose marker stage is `decompose` or `await_children` keeps that stage: `children` is in the marker and sub-issue creation is keyed (8.5). A run rebuilt in any other stage restarts at `plan` with a workspace created from `head_sha` (or `base_sha` if nothing was published), because its handoff files are gone. A marker with a non-null `escalation` is rebuilt in `needs-human`; this rule is applied to its recorded stage when it resumes."
Change the first bullet to: "The approver is re-read from the latest `labeled` event for `state:ready`; for a sub-issue claimed under SEC-1a it is the parent marker's `plan_approved_by`."

### C9 [new_contradiction] [minor] new §16.1 CFG-1 vs 6.1, 7.3 plan gate, SEC-1

**Problem.** CFG-1: "Repository policy can only tighten operator settings: ... and disable `plan` for tier 0–1". 6.1 and 7.3 require the plan gate for a tier 1 issue with `risk:high` ("Required for tier 2 and `risk:high`"), and SEC-1 relies on it as the restrictive fallback ("read as its most restrictive value (`risk:high`, plan gate required"). With `plan` disabled for tier 1 there is no plan to post at the gate, so the gate either cannot run or is silently dropped by a policy pull request, which is a loosening, not a tightening.

**Proposed fix.** In CFG-1 replace "disable `plan` for tier 0–1" with "disable `plan` for tier 0 and for tier 1 issues that are not `risk:high` (where the plan gate of 7.4 applies, `plan` always runs)". In 16.2 change the comment on `plan = "auto"` to "# auto = by tier and risk; false never removes the plan gate".

### C10 [new_contradiction] [minor] new §7.5.1 step 5 vs 7.4 PLAN-3, 7.3 round caps

**Problem.** The plan cap can never be reached. PLAN-3: "`state:ready` without a valid approval is a revision request: `plan` runs again with the updated snapshot (plan round +1)" and 7.3 caps plan rounds at 2. But every revision request is a resume from `needs-human` with category `plan-approval`, and 7.5.1 step 5 "Sets the round counter of the gate that escalated to 0" first, so the counter is always 1 after a revision.

**Proposed fix.** Append to the counter step of 7.5.1: "For category `plan-approval` the plan counter is not zeroed: it counts revision requests (PLAN-3). When it exceeds the plan cap the factory does not re-plan; it escalates with `clarification`, and resuming from that escalation sets the plan counter to 0."

### C11 [new_gap] [minor] new §4.2 vs 7.3 `ready` row

**Problem.** From the K13 fix. 4.2: "In `team`, an empty reviewer list after removing the approver is a configuration error." The approver is known only per issue, so this cannot be checked when configuration loads; it first shows at the `ready` stage ("review requested (4.2)"), which 7.3 says "Cannot fail". With `reviewers = [A]` and A approving an issue, the run reaches `ready` with nobody to request review from and no defined transition.

**Proposed fix.** In 4.2 replace the sentence with: "In `team`, `reviewers` MUST list at least two accounts; this is validated when the configuration is loaded, so removing the issue's approver never leaves the list empty."

### C12 [new_gap] [minor] new §8.2 step 4 vs 5.2, 6.4

**Problem.** The reordered claim (K7) leaves two state labels. Step 4: "Add `state:in-progress`, then remove `state:ready`." At claim the issue still carries the label it had before the human added `state:ready` (5.2: "A human need only add it; the factory removes the previous state label"): `state:triage`, or `state:needs-human` after a lint failure (6.4). Nothing in the claim removes it, unlike 7.5.1 ("removes the other state labels"), so every claimed issue has two `state:*` labels, is not a work item by 5.2 ("exactly one"), and is rewritten by GH-2 normalisation with a correction note in the status comment.

**Proposed fix.** Change 8.2 step 4 to: "4. Add `state:in-progress`, then remove `state:ready` and every other `state:*` label (`state:triage`, or `state:needs-human` left by a lint failure), and remove `needs:clarification` if present."

## Lens: platform-facts (10 earlier findings judged fully resolved)

### C13 [partly_resolved] [major] K15 §9.4 steps 2 and 4; 12.4 steps 1-2; 19.4 Git row

**Problem.** The merge-commit base update replaces the rebase soundly, but capture still uses the pre-update rules. 9.4 step 2: "It builds a fresh index from the previous candidate's tree (or `base_sha`)". 9.4 step 4: "`git commit-tree` on the previous published head (or `base_sha`)". 12.4 step 2 only says "The resulting candidate has the same two parents" for the first conflict round. Two failures follow. (a) In a conflict round the previous candidate is the pre-update head, so the index lacks upstream files. Reproduced on git 2.56.1: an upstream file that is tracked but matches an ignore rule is untracked-and-ignored against that index, is not staged, and the candidate deletes it (`git diff B1 C1` shows `D big.gen`). That is the K15 revert again. The 5 MiB and total-bytes caps are also applied to everything that landed upstream. (b) If a gate fails on an unpublished update commit (clean update under `always`, or a second round after a conflict), the next candidate gets the single parent "previous published head". The default-branch head is then not an ancestor, GitHub's pull request diff shows upstream changes as part of the pull request, and under `always` the branch is "behind" again with an identical merge tree.

**Proposed fix.** 9.4 step 2, replace the sentence with: "It builds a fresh index from the tree the workspace content was last taken from: the previous candidate's tree, or `base_sha`, or, in the first round after a conflicting base update, the conflicted merge result the workspace was re-created from (12.4 step 2). Path and size limits therefore apply only to what the implementer changed."

9.4 step 4, replace the first sentence with: "`git commit-tree` on the previous published head (or `base_sha`). From a base update until its first push, every candidate instead has the update commit's two parents (previous published head, new default-branch head), so the default-branch head stays an ancestor of the work branch when a gate fails on an unpublished update commit."

12.4 step 2, replace the last sentence with: "The resulting candidate, and every later candidate until one is published, has the same two parents (9.4 step 4)."

19.4 Git row, append: "; a base update where the default branch added a tracked file matching an ignore rule keeps that file; a candidate built after a gate failure on an unpublished update commit has the new default-branch head as an ancestor".

### C14 [new_contradiction] [major] new §12.3 (paragraph after step 4), 15.3, VAL-6, GH-18

**Problem.** The K20 resolution (decision D-9, `on_conflict`) relies on: "A red default-branch CI run after a factory merge pauses merges in that repository, files a `state:triage` issue linking the merged pull request, and counts as a tripwire (15.3)." But VAL-6 says the workflow template "runs on `pull_request` and `merge_group` only", so no CI run exists on the default branch after a squash merge. Nothing polls default-branch checks either: GH-18 polls issues and "Open pull requests of active runs". The merge pause also has no exit condition. As written the only safeguard for semantic conflicts can never fire, and if it did, merges would never resume.

**Proposed fix.** VAL-6: replace "It runs on `pull_request` and `merge_group` only" with "It runs on `pull_request`, `merge_group` and `push` to the default branch, and on nothing else".

12.3, replace the last sentence of the paragraph after step 4 with: "After each factory merge the reconciler reads the VAL-8 checks on the pull request's `merge_commit_sha` on every tick until all have concluded or `ci.timeout_secs` has passed. A conclusion other than `success`, `neutral` or `skipped`, or a check still missing at the timeout, is a red run: it pauses merges in that repository, files a `state:triage` issue linking the merged pull request, and counts as a tripwire (15.3). The pause ends when the VAL-8 checks are satisfied on the current default-branch head, or when the operator runs `sf resume`. Runs waiting in `merge` keep their approval."

GH-18, append: "The default-branch commit produced by a factory merge is polled for its check results on the same tick (12.3)."

### C15 [new_gap] [major] new §9.1, 9.2 (WS-1, WS-6), 9.4 step 2, SEC-6, 8.4 step 4

**Problem.** `host.git` carries the whole K15 fix (test-merge, update commit, candidates, export) but is defined only as "host-owned bare repository for candidates and publishing". Nothing says how it is created or how the base and each new default-branch head get into it. 9.4 step 2 gives sandboxed capture "`host.git` objects as a read-only alternate", yet SEC-6 lists for capture only the workspace (read-only) and `quarantine/`, and denies "the mirror and the rest of the data directory". The sandboxed git therefore cannot read the previous candidate's tree it must build its index from. WS-6 ("re-creates it from the update commit"), 12.4 step 2 ("from the merge result including conflict markers") and 8.4 step 4 ("a workspace created from `head_sha`") cannot use WS-1's command: it clones only the mirror's default branch, and update commits and unpublished candidates exist only in `host.git`.

**Proposed fix.** Add to 9.2: "**WS-10.** `host.git` is created at claim with `git clone --bare --no-local --single-branch --branch <default> <mirror> host.git`, and its remote is removed. Before every test-merge, base update or rebuild the host fetches the default branch (and, on rebuild, this run's work branch) from the mirror into it. It holds only objects reachable from the default branch and from this run's candidates. A workspace made from a commit that is not on the default branch (WS-6, 12.4 step 2, 8.4 step 4) is cloned with `git clone --no-local --no-checkout` from `host.git` at a temporary ref on that commit, checked out detached, and then has its remote removed; for a conflicted merge result the host first wraps the tree in a commit that is never pushed. The temporary ref is deleted afterwards."

WS-1, change "holding only objects reachable from the base" to "holding only objects reachable from the base and from this run's own candidates".

SEC-6, Read-only row, add: "`host.git/objects` (capture only)".

### C16 [partly_resolved] [major] K17 §20.3 (`team`), 5.5 GH-8, GH-9, GH-10, 5.9 GH-18, 8.5

**Problem.** 20.3 `team`: "The factory reports its three checks as `neutral` ("not a factory pull request") on every open same-repository pull request that is not its own". This exists only as prose in the onboarding section and contradicts section 5. 5.5 says the factory reports checks "on every head SHA it publishes, with `external_id` set to the run id". GH-9 says "A factory check is reported for a SHA only when the gate ran on exactly that commit's tree". GH-8 defines `neutral` only as an integrity flag. Nothing discovers non-factory pull requests or their new head SHAs (GH-18 polls only "Open pull requests of active runs"), and the check-run key in 8.5 needs a run id. An implementer following section 5 builds none of it, and in `team` + `enforced` every human pull request then waits forever on three required checks.

**Proposed fix.** Add to 5.5 after GH-10: "**GH-10a.** In `enforced` mode with the `team` profile, the factory reports the three factory checks with conclusion `neutral`, title 'not a factory pull request' and no `external_id` on the head SHA of every open pull request that targets the default branch, has its head branch in the same repository and is not the pull request of a run. This is the only exception to GH-9. Such pull requests are listed on every tick (`GET /repos/{o}/{r}/pulls?state=open&base=<default>` with an ETag); the checks are reported once per head SHA, found again by name and App id on that SHA. They are never reported on a SHA that a run has journalled as a candidate or a pushed head. 12.3 step 1 and 8.4 step 4 accept a factory check only when its `external_id` is the run id. Fork pull requests get no checks (SEC-5)."

GH-8, add to the list of conclusions: "not a factory pull request (GH-10a) → `neutral`".

20.3 `team`, append: "(GH-10a)".

### C17 [new_gap] [major] new §12.4 step 1, 7.6, GH-11, 23.2 P-5

**Problem.** 12.4 step 1 pushes "an update commit in `host.git` with parents (previous published head, new default-branch head)" using an App that, per GH-11, has no Workflows permission: "Without Workflows, GitHub rejects pushes that change `.github/workflows/`". When the default branch changed a workflow file since `base_sha` (the operator-committed workflow of 20.1, or any human workflow change), the pushed range contains those workflow changes. GitHub is reported to reject such pushes from tokens without the workflow permission even though the commits already exist in the repository; this is not verified here. P-5 asks only the defensive direction ("rejected ... on every push path?", impact "Defence in depth for 11.3"). If the rejection applies, every in-flight run needs a base update it can never push; 8.6 classes that as Deterministic, and no exit is defined. The same question applies to the reset ref update of 7.6.

**Proposed fix.** Add to 23.2: "| P-20 | Is the push of a base-update merge commit (12.4), or the ref update of a reset (7.6), rejected for an App without the Workflows permission when the default branch changed a file under `.github/workflows/` between the old and the new `base_sha`? | 12.4, 7.6, GH-11 |"

Add to 12.4 after step 4: "5. If the push of an update commit is rejected because the range carries default-branch changes under `.github/workflows/` (GH-11, probe P-20), the failure is not retried. The run does an automatic reset (7.6) if one is still available, and otherwise escalates with `policy-denial`, naming the workflow files that changed on the default branch."

22, M0 exit criterion is unchanged ("All M0 probes answered" now includes P-20).

### C18 [new_contradiction] [minor] new §9.4 Export

**Problem.** "`verify` and `review` use a clean checkout of the candidate made from `host.git` with a temporary index and `git checkout-index -a --prefix=runs/<run_id>/export/<sha>/`". `host.git` is bare and `git checkout-index` requires a work tree. Run as written on git 2.56.1 it exits 128 with "fatal: this operation must be run in a work tree". This is the same class of error K15 raised for `git rebase`.

**Proposed fix.** Replace the first sentence of the Export paragraph with: "`verify` and `review` use a clean checkout of the candidate made from `host.git`: `git read-tree <candidate>` into a temporary index, then `git --git-dir=host.git --work-tree=runs/<run_id>/export/<sha> checkout-index -a` into that new, empty, host-created directory (bare `host.git` has no work tree, and `checkout-index` refuses to run without one), with no filters or attributes applied (attribute source set to the empty tree, as in VAL-5a)."

### C19 [partly_resolved] [minor] K23 §12.3 step 3, 8.5 Merge row

**Problem.** `merge_action`, `commit_title` and `commit_message` are now sent, but the result handling is inconsistent: "Journal the returned id and poll the pull request's merged state. A `failed` result re-runs step 1". The pull request's merged state can never report `failed`; only the request status read by id can. 400 and 422 (not mergeable) are unhandled, so 8.6 treats 422 as Deterministic and escalates. 8.5 says "A 409 means a request is pending: poll ... for up to 10 minutes", which is wrong for the synchronous endpoint used in `factory-enforced` mode, where 409 means the head SHA no longer matches and 405 means not mergeable.

**Proposed fix.** 12.3 step 3, `enforced` bullet, replace the last two sentences with: "A 202 returns a request id: journal it and poll `GET /repos/{o}/{r}/pulls/{n}/merge-async/{id}` (results are kept 24 hours) together with the pull request's merged state. Merged with the journalled head SHA is success. A 200 means already merged: confirm the merged head SHA. A `failed` status, or a 400 or 422 from the call, re-runs step 1 and is not a Deterministic failure under 8.6; twice in a row escalates with `infrastructure`. A 409 is handled as in 8.5."

`factory-enforced` bullet, append: "A 405 (not mergeable) or 409 (the head no longer matches `sha`) re-runs step 1 at once."

8.5 Merge row, replace "A 409 means a request is pending" with "A 409 from `merge-async` means a request is pending".

### C20 [partly_resolved] [minor] K19 §4.1 DEP-4, 20.3 last sentence, 23.2 P-14

**Problem.** 20.3 says "The mode detection of 4.1 accepts exactly these bypass configurations", but the 4.1 table has no bypass condition, and DEP-4 says only that `sf repo check` "records the result" and "repeats when the ruleset's `updated_at` changes". Three things are undefined: the mode when the list read is not the accepted one; how a daemon that holds no operator credential "repeats" the check; and how it sees `updated_at`, which the branch-rules call does not return. P-14 lists DEP-4 as its design impact, but DEP-4 never uses `current_user_can_bypass`, and P-14 looks for it "on the branch-rules endpoint", whereas the field is documented on the ruleset object.

**Proposed fix.** DEP-4, append: "If the list read is not the one 20.3 fixes for the repository's profile, or contains the factory App, the mode is `factory-enforced` until a later `sf repo check` passes. At every mode detection the daemon reads `updated_at` of each contributing ruleset (`GET /repos/{o}/{r}/rulesets/{ruleset_id}`, ids taken from the branch-rules response); when it changes, the mode reverts to `enforced (bypass list unverified)` until the operator runs `sf repo check` again. If probe P-14 shows `current_user_can_bypass` is populated for installation tokens, any value other than `never` on a contributing ruleset makes the mode `factory-enforced`; an absent value is treated as unverified."

P-14, replace the question with: "Is `current_user_can_bypass` populated for installation tokens on the ruleset objects (`GET /repos/{o}/{r}/rulesets/{id}`) that contribute to the default branch's rules?"

### C21 [new_gap] [minor] new §20.1 step 5, GH-11, 13.5

**Problem.** The ordering rule added for K18 reads as a `sf repo add` action: "After the merge: create the label set through the API, write the workflow's check names into `required_checks`, and only then activate the ruleset of 20.3." The App has no Administration permission (GH-11) and cannot create or activate a ruleset. The only other credential, the operator's, is per 13.5 "Used once by `sf repo check` to read the bypass list". The step has no actor that can perform it.

**Proposed fix.** Replace 20.1 step 5 with: "5. After the merge: create the label set through the API and write the workflow's check names into `required_checks`. Only then is the ruleset of 20.3 activated, by the operator: the App has no Administration permission (GH-11) and `sf` holds no credential that can write rulesets (13.5). `sf repo add` prints the ruleset for the repository's profile as importable JSON; `sf repo check` then re-detects the mode (4.1) and verifies the bypass list (DEP-4)."

### C22 [new_contradiction] [minor] new §12.4 step 3 vs 16.3, 5.4 marker

**Problem.** 16.3: "A run uses the policy commit recorded at claim for its whole life, except that a reset or a base update picks up the current one." 12.4 step 3 changes only the base: "`base_sha` in the marker becomes the new default-branch head; integrity and reviewer diffs use it." `policy_sha` is not updated, so the marker, GH-5b and INV-10 keep pointing at the claim-time policy while 16.3 says the stages after a base update run under the new one (verify commands, required checks, protected paths).

**Proposed fix.** Replace 12.4 step 3 with: "3. `base_sha` and `policy_sha` in the marker become the new default-branch head (16.3). Integrity and reviewer diffs use the new `base_sha`; every stage from `integrity` on uses the policy, prompts and repository instructions of the new `policy_sha`. If that policy is invalid, the base update waits as in 16.3."

### C23 [new_gap] [minor] new §11.3 (`.github/workflows/` row), 5.2 (`type:ci`), 7.5.1

**Problem.** The K18 change makes the row: "`fail`. For a `type:ci` issue the run escalates with `policy-denial` and attaches the proposed workflow as a patch for a human to commit". That escalation has no exit. After the human commits the patch and applies `state:ready`, 7.5.1 resumes at "the recorded stage", where integrity runs on the same candidate against the claim-time `base_sha`. The diff still contains the workflow change, so the run escalates again. 5.2 also still describes `type:ci` as one of the "Change classes that relax specific integrity rules (11.3)", which it no longer does.

**Proposed fix.** 11.3 row, append: "A `state:ready` resume from this escalation is handled as `factory:restart` (7.6): the new attempt starts from the current default-branch head, which then contains the committed workflow. If nothing else remains to do, the human closes the issue instead. The escalation comment states both options."

5.2, replace the `type:ci`, `type:spec-delta` effect cell with: "Change classes with their own integrity handling (11.3): `type:spec-delta` permits changes under `docs/specs/**`; `type:ci` turns a workflow change into a patch for a human to commit."

### C24 [new_gap] [minor] new §12.3 (strategy `human`), 7.2, 12.4

**Problem.** Base update is reachable only from the coordinator: "For `factory_serial`, the merge coordinator handles one pull request at a time", with step 2 as the sole trigger of 12.4. Under `human` (the M1b strategy) "the factory observes and finishes the run when it sees the merge". A pull request that comes to conflict with the default branch cannot be merged by the human, cannot be fixed by a human push (7.7: foreign commit, "never push over it"), and is never updated by the factory. The run sits in `human_review` or `merge` with no exit other than cancelling the issue, although 7.2 lists "merge conflict" as an `in-review` → `in-progress` trigger.

**Proposed fix.** 12.3, add after the strategy sentence: "Under `human` the factory still runs step 2 on every tick while the run is in `human_review` or `merge`: a conflicting test-merge starts a base update (12.4). Before base update is implemented (M2), `sf run` reports the conflict in the status comment and stops; the operator cancels the run and approves the issue again."

## Lens: security (12 earlier findings judged fully resolved)

### C25 [partly_resolved] [major] K33 §9.3, 9.1, INV-11, SEC-6

**Problem.** 9.3: "`CARGO_TARGET_DIR` is outside the workspace: `runs/<run_id>/target` for agent stages. `verify` gets a new, empty target directory on every execution" and "No cache writable by a worker is shared between runs or between an agent stage and `verify`." `review` is an agent stage (7.3: "agent, read-only"), so the reviewer gets the target directory the implementer wrote (SEC-6 makes "the stage's target directory" read-write). That contradicts INV-11: "Nothing one stage execution writes is visible to a later one except the workspace (implementer rounds only), the candidate commit and the structured result." INV-11 as written also forbids implementer round 2 from seeing round 1's target, which 9.3 intends. The implementer can still plant build artifacts that run when the reviewer builds or tests.

**Proposed fix.** 9.3 first bullet, replace with: "`CARGO_TARGET_DIR` is outside the workspace. Implementer stage executions of one attempt share `runs/<run_id>/target`. `plan`, `author_tests`, `verify` and `review` each get a new, empty target directory on every execution, so a build there is cold each time. That cost is accepted; CI is the second clean-environment gate."
9.3 third bullet, replace the first sentence with: "No cache writable by a worker is shared between runs, or between the implementer and any other stage."
9.1 layout line: "target/                        CARGO_TARGET_DIR for implementer stage executions only"
INV-11 last sentence, replace with: "Nothing one stage execution writes is visible to a later one except the workspace and the implementer target directory (both visible to implementer rounds only), the candidate commit and the structured result."

### C26 [partly_resolved] [major] K36 §13.2 SEC-1a, 7.4 PLAN-2, SEC-1

**Problem.** SEC-1a honours a factory-applied `state:ready` only on a sub-issue "whose parent's plan approval is valid (PLAN-2) with an approver who still has write permission at the sub-issue's claim". But PLAN-2 says "The factory removes the label when it consumes an approval", and SEC-1 honours `factory:plan-approved` only for "the most recent `labeled` event for that exact label, with no later `unlabeled` event". By the time any sub-issue is claimed the label has been removed by the factory, so the approval can never be "valid (PLAN-2)" when re-evaluated. Read literally, plan sub-issues never run, which is gap (1) of K36 again.

**Proposed fix.** SEC-1a, replace "and whose parent's plan approval is valid (PLAN-2) with an approver who still has write permission at the sub-issue's claim" with: "and whose parent's marker records a consumed plan approval (`plan_sha256` and `plan_approved_by`), where the plan comment's current SHA-256 still equals `plan_sha256` and `plan_approved_by` still has write permission at the sub-issue's claim. The factory's own removal of `factory:plan-approved` (PLAN-2) does not void a consumed approval."
PLAN-2, replace the last sentence with: "When it consumes a valid approval the factory writes `plan_approved_by` to the marker and then removes the label. Validity under SEC-1 and this rule is evaluated once, at consumption."

### C27 [new_gap] [major] new §13.2 SEC-1 / SEC-1a, 7.4 PLAN-1, 5.4, 6.3

**Problem.** SEC-1: "A tier, risk or type label that fails the check is read as its most restrictive value (`risk:high`, plan gate required, not `type:ci`)", and the check requires "an actor of type User". SEC-1a exempts only "A `state:ready` applied by the factory App". The `risk:*` and `type:spec-delta` labels on plan sub-issues are applied by the App, so every sub-issue is read as `risk:high` (plan gate on each, 6.1) and the spec-delta sub-issue of 6.3 is never treated as `type:spec-delta`. PLAN-1's layout "(key, title, risk, blocked-by edges, full body)" also omits the planner's `spec_delta` field (10.5), which `decompose` cannot recover because it creates sub-issues "by parsing it, never from agent output or a local copy".

**Proposed fix.** SEC-1a, append: "On such a sub-issue the `risk:*` and `type:spec-delta` labels applied by the factory App at creation are honoured as if applied by that approver, as long as they equal the values recorded for the sub-issue in the parent's `children`. Any other label change on a sub-issue falls under SEC-1. Plan sub-issues carry no tier label (tier 1)."
PLAN-1, replace "(key, title, risk, blocked-by edges, full body)" with "(key, title, risk, spec-delta flag, blocked-by edges, full body)".
5.4, replace "`children` (tier 2 parents only) is a list of `{"issue","body_sha256"}`" with "`children` (tier 2 parents only) is a list of `{"issue","body_sha256","risk","spec_delta"}`".

### C28 [new_gap] [major] new §7.4 `decompose` row, 6.5, SEC-1a

**Problem.** 7.4 `decompose`: "create the sub-issues and blocked-by links by parsing it ... Record `children` in the marker." 6.5: "the only issues the factory creates in `state:ready` are sub-issues". SEC-1a honours that label only if the sub-issue "is listed with its body hash in the `children` of its parent's status marker". The issue numbers are not known until creation and writes are queued at least 1 s apart (GH-13), so for many seconds the sub-issues exist in `state:ready` with no `children` entry and no blocked-by links. A poll tick in that window either rejects them as an unauthorised `state:ready` (7.2: back to `triage`) or dispatches one before its blockers are linked.

**Proposed fix.** 7.4 `decompose` row, replace the Summary with: "Re-read the approved plan comment from GitHub, check its hash, and by parsing it (never from agent output or a local copy): (1) create every sub-issue in `state:triage`; (2) create the blocked-by links; (3) record `children` in the marker; (4) only after the marker write is acknowledged, add `state:ready` to each child and remove `state:triage` (GH-1)."
6.5, replace "the only issues the factory creates in `state:ready` are sub-issues of a human-approved tier 2 plan, under SEC-1a" with "the only issues to which the factory applies `state:ready` are sub-issues of a human-approved tier 2 plan, under SEC-1a and in the order given in 7.4".

### C29 [new_gap] [major] new §5.4 GH-5a, 7.4 PLAN-1 / PLAN-4, 6.4, 8.2 step 1

**Problem.** GH-5a: "All text that originates from an agent ... and is written to GitHub passes through one outbound encoder. It caps length, renders the text inside a fenced block labelled as agent output, and neutralises ...". Sub-issue bodies originate from the planner and are created "by parsing" the plan comment, but the spec never says what bytes a sub-issue body consists of. If the encoder applies, the body is a fenced block, while PLAN-4 lints the proposed body "before the plan is posted" (un-encoded) and the dispatcher lints again at claim (8.2 step 1) with a parser "keyed on heading text" (WI-1) on a body that no longer has headings. `body_sha256` in `children` is likewise undefined (raw or encoded).

**Proposed fix.** PLAN-1, append: "Sub-issue bodies are built once by the host, before the plan is posted: the planner's `body` with the neutralisations of GH-5a applied, but not wrapped in a fenced block and capped only by the 32 KiB lint limit, so the section headings of 6.2 render and parse. The lint of PLAN-4 runs on exactly these bytes. The plan comment carries each body inside a fenced block whose fence is longer than any backtick run in the body. `decompose` extracts the bytes between the fences unchanged and creates the issue with body = the first-line `factory:subissue` marker (8.5), a newline, then those bytes. `body_sha256` in `children` is the SHA-256 of the body as created."
GH-5a, after "renders the text inside a fenced block labelled as agent output" insert "(except plan sub-issue bodies, PLAN-1)".

### C30 [new_gap] [major] new §5.4 GH-5a, 9.4 Subject validation

**Problem.** GH-5a: "a subject containing an issue reference, an issue URL, a closing keyword or a skip token (9.4) is rejected instead", and 9.4 rejects "anything GH-5a forbids" and says "The pull request title and the squash title and message are generated by the host under the same rule." GitHub's closing keywords are close/closes/closed, fix/fixes/fixed, resolve/resolves/resolved. As written every conventional subject such as `fix: handle empty input` is sent back to `implement` until the verify round cap, and a host-generated title derived from an issue titled "Fix ..." can never be produced (the 6.4 lint only rejects skip tokens in titles). A keyword closes nothing without a reference.

**Proposed fix.** GH-5a, replace the last sentence with: "Commit subjects have no markup to escape, so a subject containing an issue reference (`#<digits>`, `GH-<digits>`, `owner/repo#<digits>`), an issue URL or a skip token (9.4) is rejected instead. A closing keyword with no reference closes nothing and is allowed, so subjects such as `fix: handle empty input` pass."
9.4 Subject validation, replace the last sentence with: "The pull request title and the squash title are generated by the host from the issue title with any issue reference or issue URL removed (skip tokens are already excluded by the lint, 6.4); the squash message is host-written (5.6)."

### C31 [new_contradiction] [major] new §9.4 step 2, 13.3 SEC-6

**Problem.** 9.4 step 2 runs "the host's git binary" in a sandbox with "Its object directory is `quarantine/`, with `host.git` objects as a read-only alternate. It builds a fresh index ...". SEC-6 is deny-by-default and its Read-only row lists only "The resolved toolchain; the agent CLI; the role's handoff view; the workspace (planner, capture); the candidate export (reviewer); the host-written settings file; a backend-defined OS baseline"; its Denied row covers "the mirror and the rest of the data directory". Neither `host.git/objects` nor the git installation is mounted, and with the workspace read-only there is no stated place for the index. Capture cannot run under SEC-6 as written; WS-3 ("The agent MAY run git in its workspace") has the same problem for the git binary.

**Proposed fix.** SEC-6 table, Read-write row: replace "`quarantine/` (capture only)" with "`quarantine/` (capture only; it also holds the capture index and the host-written git configuration is read from outside it)".
SEC-6 table, Read-only row: after "the agent CLI;" insert "the host's `git` installation; `host.git/objects` (capture only);".
9.4 step 2, after "with `host.git` objects as a read-only alternate." insert "The index file lives in `quarantine/`; nothing in `workspace/.git` is read (WS-3)."

### C32 [new_gap] [major] new §5.4 GH-5, 7.7 (cancel rows), 8.4 step 4

**Problem.** GH-5: "on rebuild, the earliest App-authored comment whose body starts with the marker block". 5.4 says "Each run has exactly one status comment on its issue", so an issue that was cancelled (7.7) and later re-approved has two status comments, and rebuild picks the cancelled run's one: stale `policy_sha`, `plan_sha256`, `plan_approved_by`, counters and branch data. Nothing writes a terminal stage to the marker on cancellation (GH-4 allows writes only "on stage transitions and escalations"), so `sf rebuild` also cannot tell that an open `state:triage` issue with a marker at stage `implement` has no live run, and may resurrect work a human cancelled.

**Proposed fix.** GH-5, replace the first sentence with: "The status comment is the comment whose id is in the journal or, on rebuild, the most recently created App-authored comment whose body starts with the marker block; earlier ones belong to terminal runs of the same issue. A marker whose `stage` is `done` or `cancelled` reconstructs no run."
GH-4, replace the first sentence with: "The marker is written only on stage transitions, escalations and when the run reaches a terminal state."
7.7, in the row "Issue closed and the pull request is not merged", replace the action with: "Cancel: run → `cancelled`; write `"stage":"cancelled"` to the marker; close the pull request; delete the branch per 18.2."

### C33 [new_gap] [major] new §8.4 step 4, GH-5b, 7.4

**Problem.** 8.4 step 4: "A run whose marker stage is `ci`, `ready`, `human_review` or `merge` keeps that stage. A run rebuilt in an agent or deterministic stage restarts at `plan`". The plan gate (wait), `decompose` (host), `await_children` (wait), `publish` (host) and a run with a non-null `escalation` fit neither sentence, so their resume stage after `sf rebuild` is undefined. If a tier 2 parent in `await_children` is treated as "restarts at `plan`", it produces a new plan hash and, because the sub-issue key is `<run>/<plan sha256>/<key>` (8.5), a second set of sub-issues.

**Proposed fix.** 8.4 step 4, replace the third bullet with: "A run whose marker stage is `ci`, `ready`, `human_review`, `merge`, the plan gate or `await_children` keeps that stage; at the plan gate `handoff/plan.md` is rewritten from the plan comment after its hash is checked against `plan_sha256`. A run rebuilt in `decompose` re-runs it; the sub-issue markers of 8.5 make that idempotent. A run rebuilt in any other stage (`plan`, `author_tests`, `implement`, `capture`, `integrity`, `verify`, `publish`, `review`) restarts at `plan` with a workspace created from `head_sha`, because its handoff files are gone. A run whose marker carries an `escalation` stays in `needs-human`, and these rules apply to its recorded stage at resume (7.5.1)."

### C34 [partly_resolved] [minor] K37 §13.2 SEC-1, 5.2, 7.7

**Problem.** SEC-1 authorises only `labeled` events ("the most recent `labeled` event for that exact label, with no later `unlabeled` event"). 5.2: "No tier label means tier 1" and "No risk label means `risk:medium`". An actor without write permission (the triage role on an organisation repository) can therefore remove `risk:high` or `tier:2` before claim and the issue is read as medium / tier 1, skipping the plan gate; 7.7 only catches changes "after claim". K37's "Removing a control is not covered" was fixed for `factory:hold` and `factory:stop` only. Separately, 7.7 honours "`state:triage` applied by a human" (cancel) and "`state:needs-human` applied by a human", which are not in SEC-1's list and not stated as exempt.

**Proposed fix.** SEC-1, append: "Removal is covered too: an `unlabeled` event for a `tier:*`, `risk:*` or `type:*` label whose actor is not an authorised actor is ignored; the value of the last authorised `labeled` event stays in force and the factory re-applies the label once. `state:triage`, `state:needs-human` and closing an issue are deliberately honoured from any human actor, because they can only stop work (7.7)."

### C35 [new_contradiction] [minor] new §7.5.1 step 2, 5.3 rule 6

**Problem.** 7.5.1 step 2: "Re-evaluates readiness rules 2–4 and 6 of 5.3". Rule 6's failure action is "the factory removes `state:ready`, sets `state:triage` and says so once." Applied on resume, that puts `state:triage` on an issue whose run is still non-terminal in `needs-human`: a factory-applied `state:triage` is not a cancellation (glossary: "moved to `state:triage` by a human"), and 7.5.1 step 1 and step 6 both leave such an issue in `state:needs-human`. The resulting label/run combination is defined nowhere.

**Proposed fix.** 7.5.1 step 2, replace with: "Re-evaluates readiness rules 2–4 and 6 of 5.3 and waits for a slot. The issue keeps `state:ready` while it waits. If rule 6 fails (the title or body was edited after the resuming `labeled` event), the factory removes `state:ready`, leaves `state:needs-human` and says so once; it does not set `state:triage`."

### C36 [new_gap] [minor] new §13.3 SEC-6a, 10.6 AG-11 / AG-19, 13.5

**Problem.** SEC-6a: "Argument and environment visibility of same-user host processes is not prevented at level `process` on macOS, which is why INV-1 forbids secrets there." INV-1 covers only GitHub credentials, while AG-11 puts "the model gateway address and per-run token" in every worker's environment. With up to 4 concurrent runs (8.3), a subverted worker can read another run's gateway token and, since nothing binds a token to its run's endpoint (13.5 says only "a per-run token valid only there"), spend against that run's budget. That defeats per-session and per-issue limits for the attacker's own run (section 14, principle 7).

**Proposed fix.** AG-19, append: "The gateway accepts a per-run token only on connections that arrive through that run's own proxy endpoint (SEC-10); a token presented on any other endpoint is refused and logged with both run ids. A worker's sandbox can connect to no endpoint but its own run's."
SEC-6a, replace "which is why INV-1 forbids secrets there" with "which is why INV-1 keeps every GitHub credential out of child processes, and why the only secret in a worker's environment, the per-run gateway token, is useless outside its own run's endpoint (AG-19)".

## Lens: rust-scope (12 earlier findings judged fully resolved)

### C37 [new_contradiction] [major] new §2 Glossary (Stage execution); 8.1 ORCH-3; 9.1; 9.2 WS-4; 11.2 VAL-1; 13.3

**Problem.** Glossary: "Stage execution: ... one agent session, or one setup, capture or verify command sequence. Each gets its own sandbox instance." ORCH-3 lists "Agent session, setup, capture, verify" as separate activities, and 9.1 gives each stage execution a fresh `exec/<seq>/home/` ("fresh HOME, CARGO_HOME"). But WS-4 says setup runs "at the start of every stage execution that builds" and VAL-1 says "`setup` commands run first" in verify's instance. Read as the glossary and ORCH-3 state it, setup is its own stage execution with its own instance and home, so `cargo fetch --locked` fills a CARGO_HOME that is discarded before the session or verify it was meant to prepare (INV-11 also forbids carrying it over). The per-stage-execution instance (K54) and fresh home (K49) were added without reconciling setup.

**Proposed fix.** Glossary, Stage execution: "One execution of a stage: one agent session, one capture, or one verify command sequence, together with the `setup` commands that precede it (WS-4). Each gets its own sandbox instance."

ORCH-3 Activity row, Examples: "Agent session, capture, verify (each including its setup commands)".

WS-4, append: "Setup is not a stage execution of its own: it runs as the first step of the activity it prepares, in the same sandbox instance with the same `HOME`, `CARGO_HOME` and target directory, under `setup.timeout_secs`. A setup failure fails that activity with class `infrastructure`."

8.2 deadline table, Deterministic row: "Deterministic (capture, verify, and the setup step of any activity) | The command timeout only."

### C38 [new_contradiction] [major] new §13.3 SEC-6 vs 9.4 step 2

**Problem.** 9.4 step 2 runs capture in a sandbox where "the host's git binary runs with a host-written configuration ... Its object directory is `quarantine/`, with `host.git` objects as a read-only alternate." SEC-6 grants capture only "`quarantine/` (capture only)" read-write and "the workspace (planner, capture)" read-only, and denies "Everything else, including ... the rest of the data directory". `host.git/objects`, the host git binary and the host-written git configuration are therefore denied, so the index cannot be built from the previous candidate's tree and the step cannot run under the profile SEC-6 specifies (and its snapshot test would lock that in).

**Proposed fix.** SEC-6, Read-only row, add: "`runs/<run_id>/host.git/objects`, the host's `git` binary and its libexec directory, and the host-written git configuration file (capture only); the `git` binary (any stage that may run git, WS-3)".

SEC-6, Read-write row, change "`quarantine/` (capture only)" to "`quarantine/` and a temporary index file under the stage's temp directory (capture only)".

### C39 [partly_resolved] [major] K45 §9.4 steps 2 and 4; 8.5 Candidate commit; 12.4 steps 1-2

**Problem.** 12.4 step 2 defines the conflict round ("re-creates the workspace from the merge result including conflict markers ... The resulting candidate has the same two parents"), but capture was not updated to match. 9.4 step 2 still "builds a fresh index from the previous candidate's tree (or `base_sha`)": in a conflict round that makes everything that landed upstream part of the round's change set, so one upstream file over 5 MiB or an upstream delta over the total-bytes cap fails capture on every round until the cap triggers a reset. 9.4 step 4 still says "`git commit-tree` on the previous published head (or `base_sha`)" (one parent) and 8.5 journals "Tree, parent". 12.4 also never says how the new default-branch head reaches `host.git` or what object the workspace is re-created from when no update commit exists yet (WS-6 says "from the update commit"; WS-1 clones from the mirror, which does not hold the merge result).

**Proposed fix.** 12.4 step 1, replace with: "The host fetches the new default-branch head from the mirror into `host.git` and creates an update commit there with parents (previous published head, new default-branch head). If the test-merge was clean, its tree is the merge result and the commit is the new candidate. If it conflicted, its tree is the `merge-tree` result including conflict markers; this commit is a work-in-progress base W: it is journalled, never pushed and never a candidate."

12.4 step 2, replace the first sentence with: "If it conflicted, the host re-creates the workspace from W (a WS-1 clone with `host.git` as the source instead of the mirror), writes the conflicting paths to the handoff, and runs an implementer round (merge-conflict round +1)."

9.4 step 2, replace "It builds a fresh index from the previous candidate's tree (or `base_sha`)" with: "It builds a fresh index from the tree the workspace was last created or captured from: the previous candidate's tree, `base_sha` on the first round, or W's tree in a merge-conflict round (12.4), so upstream content is never counted as the round's change set".

9.4 step 4, replace the first clause with: "`git commit-tree` with the previous published head (or `base_sha`) as parent; in a merge-conflict round the parents are W's two parents (previous published head, new default-branch head), never W itself".

8.5 Candidate commit row: change "Tree, parent, dates and message" to "Tree, parents, dates and message".

### C40 [new_gap] [major] new §8.1 ORCH-4 vs 14 and 13.5

**Problem.** ORCH-4 says the engine receives "`UsageBatch` (all per-model-call usage records since the last batch ...)" and that "per-session limits are evaluated in the supervisor", but never says where the usage records come from. Section 14 says "Limits are enforced by the control plane from gateway-metered usage. Agent-side caps are a second line only", while the only usage source ORCH-4 mentions is the agent event stream ("Raw and normalised agent events", `AgentEvent::Usage`), which the untrusted CLI produces. M1a also requires "per-session budgets" before the gateway exists (M2). As written an implementer can fill `spend` and enforce `limits.usd` from CLI-reported usage under the daemon, which is exactly what moving the gateway to M2 (K52) was meant to prevent.

**Proposed fix.** ORCH-4, add after the event list: "In `gateway` mode the usage records in `UsageBatch`, and the spend the supervisor compares with `limits.usd`, come from the model gateway's meter for the run's token; the gateway reports each completed model call to the supervisor of the live activity. The CLI's own `Usage` events are written to `events.jsonl` and used only to report a discrepancy. In `direct_key` mode (attended `sf run`, M1) the CLI's `Usage` events are the source and the limits are advisory (13.5)."

### C41 [new_gap] [major] new §8.4 step 4 (third bullet)

**Problem.** "A run whose marker stage is `ci`, `ready`, `human_review` or `merge` keeps that stage. A run rebuilt in an agent or deterministic stage restarts at `plan`". The rule adopted for K58 does not cover the wait and host stages of 7.4: a run waiting at the plan gate, and a tier 2 parent in `decompose` or `await_children`, are neither in the keep list nor "agent or deterministic". If they fall under restart-at-`plan`, a parent whose children are already open is re-planned, and because the sub-issue dedupe key is `<run>/<plan sha256>/<key>` (8.5) the new plan creates a second set of sub-issues. A rebuilt run with no published head also has a null `head_sha` to create its workspace from.

**Proposed fix.** Replace the third bullet of 8.4 step 4 with: "A run whose marker stage is `ci`, `ready`, `human_review`, `merge`, `decompose` or `await_children`, or that is waiting at the plan gate (escalation category `plan-approval`), keeps that stage. `decompose` re-reads the approved plan comment and is replay-safe through the sub-issue markers of 8.5; a run approved at the plan gate after a rebuild re-creates `handoff/plan.md` from the plan comment after checking `plan_sha256`. A run rebuilt in any other stage restarts at `plan` with a workspace created from `head_sha` (from `base_sha` when `head_sha` is null), because its handoff files are gone. A marker that carries `escalation` rebuilds the run in `needs-human`, and this rule is applied at resume."

### C42 [new_contradiction] [minor] new §3.1 INV-11 vs 9.3 and 9.1

**Problem.** INV-11: "Nothing one stage execution writes is visible to a later one except the workspace (implementer rounds only), the candidate commit and the structured result." 9.3 and 9.1 give agent stages one shared `runs/<run_id>/target` that is deleted only "when the run enters `human_review` or a terminal state", so it is written by one agent stage execution and read by the next. Section 3.1 says a change that breaks an invariant is a defect, so an implementer testing INV-11 literally must wipe `target/` between implementer rounds, making every round a cold build, which 9.3 accepts only for `verify`.

**Proposed fix.** INV-11, replace the last sentence with: "Nothing one stage execution writes is visible to a later one except the workspace and the agent-stage target directory `runs/<run_id>/target` (both visible to agent stages only, never to `verify` or `review`), the candidate commit and the structured result."

### C43 [partly_resolved] [minor] K49 §13.1 (row "Malicious dependency or build script"); 13.4 SEC-11; 13.3 SEC-6; 11.2 VAL-1

**Problem.** 13.1 still lists "`cargo deny` in verify" as a control, and SEC-11 still says "`github.com` and `api.github.com` are never on a worker's list". `cargo deny check advisories` fetches the advisory database from github.com, and each verify has a fresh `CARGO_HOME`, so the check cannot run in verify as specified. Draft 2 addressed Cargo git dependencies (SEC-12, 20.1) but neither provides the advisory database nor withdraws the control.

**Proposed fix.** VAL-1, append: "The host fetches the RustSec advisory database into `<data_dir>/cache/advisory-db` (daily, host-side, never by a worker) and exposes it read-only to verify at the path cargo-deny reads (`$CARGO_HOME/advisory-dbs`); `cargo deny` in verify runs with `--disable-fetch`."

SEC-6, Read-only row, add: "the host-fetched advisory database (verify only)".

20.1 step 2, Blocking, add: "if `verify.commands` contains `cargo deny`, it passes with `--disable-fetch` against the host-fetched advisory database".

### C44 [new_gap] [minor] new §7.3 Slots vs 12.4 step 4 and 8.3

**Problem.** "It releases the slot in `human_review` ... and re-acquires one before any return to an agent stage." A base update (12.4 step 4) "re-enters at `integrity`" from `merge`, where the run holds no slot, and the next stage is `verify`, a full cold build in a sandbox that is not an agent stage. As written that build runs outside the 8.3 limits "Concurrent runs holding a slot" (4 global, 2 per repository).

**Proposed fix.** 7.3 Slots, replace the last clause with: "and re-acquires one before any return to a stage that runs in a sandbox (an agent stage, `capture` or `verify`), including re-entry at `integrity` after a base update (12.4). The run waits in its current stage until a slot is free."

### C45 [partly_resolved] [minor] K53 §21.1 BOOT-1 and BOOT-3; 21.2 Smoke eval row; 22 M1b and M2 rows

**Problem.** BOOT-1 now starts self-hosting in M1 ("From M1 the factory is run attended on its own repository from an installed, tagged build"), but BOOT-3 allows an upgrade only through "`sf self-update`, allowed only after the eval gate passes on the candidate: a smoke eval until M3". The smoke eval is "The five issues merged in M1" (it exists only at M1b exit) and both it and `sf self-update` are M2 scope. Every replacement of the installed build during M1 therefore breaks BOOT-3: the same defect K53 raised for M2 to M3, moved one milestone earlier.

**Proposed fix.** BOOT-3, replace with: "From M2, upgrading the operating binary is a human action (`sf self-update`), allowed only after the eval gate passes on the candidate: the smoke eval until M3, the eval suite afterwards. During M1 the operator installs a newer tagged build by hand; the gate is the harness tests of 19.4 passing on the tagged commit. Previous binaries and store backups are kept for rollback (18.1)."

### C46 [new_gap] [minor] new §22 M0 row; 23.2

**Problem.** M0 exit: "a cold `cargo build` of the factory repository and one real agent turn succeed under the Seatbelt profile with only the proxy reachable", but the M0 scope contains no egress proxy; it first appears in M1a ("implement at sandbox level `process` with the egress proxy"). 23.2 says each probe answer "is recorded in the store" and "All are M0", but the store is M1a scope ("Store with `runs` and `events`"). Both make the M0 exit depend on later work.

**Proposed fix.** 22 M0 scope, change the last item to: "prototypes of capture, ingest and export against the hostile-tree suite (WS-9), of the Seatbelt profile, and of the egress proxy (loopback CONNECT proxy with the SEC-11 allowlist, enough for the exit criterion)".

23.2, replace "the answer is recorded in the store and in `docs/research/probes.md`" with: "the answer is recorded in `docs/research/probes.md` and, once the store exists (M1a), imported into its `probes` table".

### C47 [new_gap] [minor] new §9.4 step 2; 8.2 Deadlines; 9.1; 9.3; 16.2; 18.2

**Problem.** Three values the rewrite relies on are never defined. (1) 9.4 step 2 rejects "any change set over the total-bytes cap": no value or key exists anywhere. (2) 8.2 gives capture "The command timeout only", but only `setup` and `verify` have a `timeout_secs`; capture has none, so the stage has no exit if sandboxed git hangs. (3) 9.3 gives verify "a new, empty target directory on every execution" and SEC-6 grants "the stage's target directory", but 9.1 lists only `target/` "for agent stages" and 18.2 removes only "`target/`, exports", so verify target directories have no location and no removal rule.

**Proposed fix.** 9.4, add after step 4: "Capture limits *(tunable)*: 5 MiB per file, 50 MiB of new or changed bytes per candidate, and a command timeout of 300 s for the sandboxed stage step. Repository policy may lower them (`[capture] max_file_bytes`, `max_total_bytes`, `timeout_secs`) and cannot raise them above operator configuration. A capture timeout is an abandoned activity (8.6 Poison)."

9.1 layout, add the line: "exec/<seq>/target/   CARGO_TARGET_DIR for a verify execution (new and empty each time)".

9.3 second bullet and 18.2 first row, change "`target/` and exports" to "`target/`, verify target directories and exports".
