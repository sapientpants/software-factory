# Software Factory — Specification

| | |
|---|---|
| Status | Draft 2. Draft 1 was reviewed by four adversarial reviewers; this draft addresses their 59 findings (`docs/research/09-spec-review.md`). |
| Date | 2026-11-18 |
| Implementation language | Rust (toolchain 1.100, edition 2024) |
| System of record | git on GitHub for code, GitHub Issues for work items |
| Research basis | `docs/research/01`–`08` (sourced reports and a fact-check table) |

The key words MUST, MUST NOT, SHOULD, SHOULD NOT and MAY are used as in RFC 2119. Numbers marked *(tunable)* are starting values with no empirical basis; they are expected to change after measurement.

---

## 1. Purpose

The software factory (binary name `sf`) is a long-running Rust program that turns approved GitHub issues into merged pull requests. For each issue it prepares an isolated workspace, runs a coding agent to plan and implement the change, validates the result with checks the agent does not control, publishes a pull request, and merges it once the required human and automated gates have passed.

Humans steer the factory by writing issues and specs, approving work, and reviewing pull requests. They do not write the code.

### 1.1 Goals

- **G1.** Take an approved issue to a mergeable pull request without human involvement between approval and review.
- **G2.** Keep GitHub as the only database of record. Losing the factory host loses no work item, decision or approval.
- **G3.** Make wrong or malicious output hard to merge: the agent that builds a change never judges it, never holds a GitHub credential, and cannot alter the checks that gate it.
- **G4.** Make cost and quality visible per issue and per merged pull request.
- **G5.** Build the factory with itself from milestone M1 (attended) and M2 (unattended) onward (section 21).

### 1.2 Non-goals

- A second issue tracker, or any store that holds work items GitHub does not.
- A general workflow engine. The pipeline is fixed in code; only stage settings are data.
- A multi-tenant service. One operator (or one small team) runs one daemon on one host.
- Several agents writing to one branch. Parallelism comes from independent issues.
- Removing human review by default. Auto-merge is a narrow, measured opt-in (section 13.6).
- Opening issues or pull requests on repositories the operator has not added to the factory.

### 1.3 Design principles

These come from the practice of teams that run such systems (report 01) and each one constrains the design below.

1. **The control plane is deterministic code.** An LLM never chooses the next lifecycle stage. Agents run inside stages; code sequences the stages and enforces the gates.
2. **One issue, one workspace, one writer, one small pull request.**
3. **A session is a cache, never state.** Every stage is restartable from the issue, the branch and handoff files alone.
4. **The builder is not the judge.** Verification runs on a clean copy of the committed tree; review is done by a separate agent with a clean context; acceptance is a human decision by default.
5. **Propose, validate, apply.** The agent produces files and a structured result. The control plane validates them and performs every external write.
6. **Assume prompt injection succeeds.** Safety comes from what the agent process cannot reach, not from what the model is told.
7. **Budgets are enforced outside the model.**
8. **Every stage is switchable and measured**, so a stage that stops catching problems can be removed.

---

## 2. Glossary

| Term | Meaning |
|---|---|
| **Operator** | The person who installs and runs the factory and owns its credentials. |
| **Authorised actor** | A GitHub account of type User whose permission on the repository is write, maintain or admin when checked (SEC-1). |
| **Approver** | The authorised actor who applied `state:ready` to an issue (for a plan sub-issue: the plan approver, SEC-1a). |
| **Control plane** | The trusted `sf` daemon process. Holds all credentials and performs all external writes. Also called the host. |
| **Worker** | The untrusted side: an agent process or build command and everything it spawns, inside a sandbox. |
| **Work item** | An open GitHub issue carrying exactly one `state:*` label. |
| **Run** | The factory's handling of one work item from claim to a terminal state. Identified by a run id (UUIDv7). Terminal states: `done` (pull request merged, or for a tier 2 parent all children closed as completed) and `cancelled` (issue closed without a merged pull request, issue moved to `state:triage` by a human, or `sf cancel`). A run in `needs-human` is not terminal. |
| **Attempt** | One pass of a run that starts from a fresh branch off the default branch. A run has one attempt unless it is reset (7.6). |
| **Stage** | One step of the internal pipeline (7.3). |
| **Stage execution** | One execution of a stage: one agent session, or one setup, capture or verify command sequence. Each gets its own sandbox instance. |
| **Round** | One trip back to the implement stage after a failed gate. |
| **Workspace** | The directory tree in which the implementer works. It lives for one attempt, or until a base update rebuilds it (12.4). |
| **Candidate** | A commit the host builds from the workspace (9.4). Gates run on candidates; only candidates are pushed. |
| **Policy** | The repository-owned configuration in `.factory/` on the default branch. |
| **Snapshot** | The issue title, body and trusted comments captured at claim or resume and given to agents. |

---

## 3. System overview

```
                         GitHub (system of record)
        issues · labels · dependencies · branches · PRs · check runs
                 ▲                                   ▲
   read (poll, ETag)                      every write (rate-governed)
                 │                                   │
┌────────────────┴───────────────────────────────────┴─────────────────┐
│ CONTROL PLANE (trusted)                                    sf daemon │
│                                                                      │
│  GitHub gateway ── Reconciler ── Engine (pure state machine)         │
│                                    │                                 │
│  Scheduler · Budgets · Journal (SQLite) · Policy loader              │
│  Git publisher · Integrity checker · Merge coordinator               │
│  Egress proxy · Model gateway · Control API (Unix socket)            │
└───────────────┬──────────────────────────────────────────────────────┘
                │ spawns and supervises; reads the event stream and results
┌───────────────▼──────────────────────────────────────────────────────┐
│ WORKER (untrusted, one sandbox instance per stage execution)         │
│  agent CLI ─ shell, build, tests ─ workspace or candidate export     │
│  no GitHub credential · no model key · default-deny network          │
└──────────────────────────────────────────────────────────────────────┘
```

### 3.1 Invariants

These hold in every mode and milestone. A change that breaks one is a defect.

- **INV-1.** No GitHub credential ever enters a worker, and none appears in the arguments, environment or files of any child process.
- **INV-2.** The control plane never executes anything from a workspace and never reads configuration from one: no hooks, no workspace `.git/config`, no agent settings, no `.cargo/config.toml` or `rust-toolchain.toml`. It does not open paths inside a workspace. Its only access is an `lstat`-only walk and git commands run inside a sandbox (9.4).
- **INV-3.** Every command that builds or runs repository code, and every git command whose work tree is a workspace, runs inside a sandbox.
- **INV-4.** A run starts only on an action by an authorised actor whose permission was checked through the GitHub API at that moment, or on a plan sub-issue that inherits a human plan approval under SEC-1a.
- **INV-5.** The factory identity never approves a pull request and never bypasses a repository rule.
- **INV-6.** At most one non-terminal run exists per (repository, issue), and at most one stage execution per run at a time.
- **INV-7.** All external side effects are idempotent and keyed (8.5). Repeating any step after a crash produces no duplicate branch, pull request, comment, check run or issue.
- **INV-8.** The local store is a rebuildable cache. If it disagrees with GitHub, GitHub wins.
- **INV-9.** Text from issues, pull requests, comments, logs and agents is data. It is never interpolated into a shell command or a control-plane decision, and it is encoded before being written back to GitHub (GH-5a).
- **INV-10.** The policy and agent instructions that govern a run are read from a default-branch commit, never from the workspace or the work branch.
- **INV-11.** `verify` and `review` never see the workspace. They run on a clean export of the candidate commit, in a fresh sandbox instance with a fresh home directory. Nothing one stage execution writes is visible to a later one except the workspace (implementer rounds only), the candidate commit and the structured result.

---

## 4. Deployment prerequisites and modes

### 4.1 Enforcement mode

GitHub rulesets are what stop a faulty or compromised control plane from pushing straight to the default branch. They are not available on private repositories owned by a Free personal account (verified on the operator's account; report 07). The factory detects one of two modes per repository at startup, daily, and when a ruleset changes:

| Mode | Condition | Meaning |
|---|---|---|
| `enforced` | `GET /repos/{o}/{r}/rules/branches/{default}` returns active `pull_request`, `required_status_checks`, `non_fast_forward` and `deletion` rules whose parameters match section 20.3, with the factory checks bound to the factory App | GitHub enforces pull requests, approvals and required checks. |
| `factory-enforced` | The call returns 403, or the rules are missing or weaker | Only the factory enforces the gates. This is a weaker guarantee. |

- **DEP-1.** The mode MUST be shown in `sf status`, in the status comment and in every pull request body.
- **DEP-2.** In `factory-enforced` mode the factory MUST itself verify, immediately before merging, every condition the ruleset would have enforced (12.3), and autonomy is capped at A2 (13.6).
- **DEP-3.** `enforced` mode requires a public repository, or a private repository on GitHub Pro, Team or Enterprise. The spec recommends `enforced` for any repository the factory runs on unattended.
- **DEP-4.** The App has no Administration permission, so it cannot read a ruleset's bypass list. `sf repo check` reads it once with the operator's own credential (not stored) and records the result; it repeats when the ruleset's `updated_at` changes. Until then the mode is shown as `enforced (bypass list unverified)`. The accepted bypass list is fixed by 20.3; the factory App is never on it.

### 4.2 Profile

- `solo`: one human. The approver of an issue MAY also approve its pull request.
- `team`: the pull request MUST be approved by an authorised actor other than the issue's approver.

Profile and `reviewers` (the accounts review is requested from; each must have write permission) are operator configuration per repository. Default profile: `solo`, with the approver as reviewer. In `team`, an empty reviewer list after removing the approver is a configuration error.

### 4.3 Capability probe

At startup and daily, the factory probes each repository and records what is available: rulesets, native merge queue, issue types, issue fields, sub-issues, issue dependencies. Features are used only when the probe finds them; each has a fallback (section 5).

### 4.4 Host requirements

- macOS on Apple silicon through M3. Linux is added in M4 (decision D-3).
- `git` ≥ 2.42 on the host. A case-sensitive volume for the data directory is recommended, on its own volume if possible.
- An agent CLI inside a tested version range (v1: Claude Code; 10.3).
- A sandbox backend that satisfies the repository's required isolation level (13.3). A run does not start when the backend is unavailable.

---

## 5. GitHub data model

### 5.1 What lives where

| Information | Location | Written by |
|---|---|---|
| Work item, acceptance criteria | Issue title and body | Humans (the factory never rewrites an issue body) |
| Lifecycle state | One `state:*` label | Humans and the factory per 7.2 |
| Classification | Issue type; fallback `type:*` label | Humans |
| Priority | `priority:p0`–`p3` label; the `Priority` issue field once probe P-17 shows field changes are attributable | Humans |
| Decomposition | Native sub-issues | The factory from an approved plan, or humans |
| Ordering | Native blocked-by dependencies | The factory from an approved plan, or humans |
| Run summary and durable machine state | One status comment per issue (5.4) | Factory |
| Code | Branch `factory/issue-<n>` and its pull request | Factory |
| Gate verdicts | Check runs on the pull request head SHA (5.5) | Factory and CI |
| Provenance | Commit trailers, squash message and pull request body (5.6) | Factory |
| Journal, deadlines, transcripts | Local store and run directories (section 18) | Factory |

Projects v2 and GitHub's native granular issue status are not used. Four label states correspond to native status values (triage→Backlog, ready→Todo, in-progress→In progress, in-review→In review); `state:needs-human` has no native equivalent and would remain a label if native status were adopted.

### 5.2 Labels

**State labels.** An issue is a work item if and only if it is open and carries exactly one of these:

| Label | Meaning |
|---|---|
| `state:triage` | Not yet approved for work. |
| `state:ready` | Approved (or resumed). The factory may claim or resume it. |
| `state:in-progress` | Claimed by a run. |
| `state:in-review` | A pull request is ready for human review or is merging. |
| `state:needs-human` | The run stopped and waits for a human. |

Any closed issue is terminal: closed as completed, as not planned, or as duplicate. Every close other than through a merged factory pull request is a cancellation (7.7).

**Human inputs** (honoured only under SEC-1):

| Label | Effect |
|---|---|
| `state:ready` | Start (5.3) or resume (7.5.1). A human need only add it; the factory removes the previous state label. |
| `factory:hold` | Freezes the item in any state. No new stage starts; a running stage execution is interrupted at the next tick. A held run releases its concurrency slot, and time on hold counts toward no timer or failure counter. It stays in force until an authorised actor removes it. |
| `factory:restart` | Resets the run (7.6). Removed by the factory once acted on. |
| `factory:plan-approved` | Approves the plan posted for this issue (7.4). |
| `factory:stop` | On a designated open issue: stops the repository (13.7). |
| `tier:0|1|2` | Work tier (6.1). No tier label means tier 1. |
| `risk:low|medium|high` | Risk. No risk label means `risk:medium`. |
| `priority:p0`–`p3` | Dispatch order. |
| `type:ci`, `type:spec-delta` | Change classes that relax specific integrity rules (11.3). |

**Factory-set labels:** `needs:clarification`, `needs:decision`, `factory:origin` (on factory pull requests and factory-filed issues), `factory:protected-change` (11.3). These are indicators only and are never read back as authority (SEC-1b).

- **GH-1.** Labels MUST be changed with the add-label and remove-label endpoints, never by replacing the whole label set. A swap adds the new label before removing the old one.
- **GH-2.** If the reconciler finds zero or several `state:*` labels on an issue it knows, it MUST normalise to a single state derived from GitHub evidence and the journal, and record the correction in the status comment. Normalisation never produces `state:ready`. A human-applied `state:ready` next to another state label is not a conflict: it is the start or resume signal.
- **GH-3.** `state:ready` MUST NOT appear in the `labels` key of any issue form, so that opening an issue can never approve it.

### 5.3 Readiness

An issue is dispatchable when all of these hold:

1. It is open and carries `state:ready`, valid under SEC-1.
2. It has no `factory:hold` label.
3. Every issue it is blocked by is closed. (Without native dependencies, the fallback is a `Blocked by: #n` line in the issue body.)
4. It passes the readiness lint (6.4).
5. The repository's review queue is below its limit (8.3).
6. Its title and body have not been edited since the `labeled` event that applied the current `state:ready`. The check and the snapshot come from one GraphQL read (title, body, `lastEditedAt`, rename events, the `labeled` event), so they cannot diverge. On failure the factory removes `state:ready`, sets `state:triage` and says so once.

Dispatch order is priority, then creation date, oldest first.

### 5.4 Status comment

Each run has exactly one status comment on its issue, authored by the factory's App identity, created at claim and edited in place afterwards. It starts with a hidden marker block, followed by a human-readable part (current stage, attempt, round counts, spend, links to the pull request and checks, last error, enforcement mode).

```
<!-- factory:state v1
{"run_id":"…","attempt":1,"stage":"implement","branch":"factory/issue-42","pr":57,
 "base_sha":"…","head_sha":"…","policy_sha":"…",
 "snapshot_sha256":"…","title_body_sha256":"…","tier":1,"risk":"medium",
 "plan_sha256":null,"plan_approved_by":null,"locked_paths_sha256":null,
 "escalation":null,
 "spend_usd":3.41,"budget_grants":0,
 "rounds":{"plan":0,"verify":1,"ci":0,"review":0,"human":0,"merge_conflict":0},
 "size_strikes":0,"last_failure":"…fingerprint…","children":[],
 "factory_version":"0.3.1"}
-->
```

This field list is normative for `v1`. `escalation` is `{"category","seq"}` while the run is in `needs-human`. `children` (tier 2 parents only) is a list of `{"issue","body_sha256"}`. `head_sha` is written on the transition from `publish` to `ci`.

- **GH-4.** The marker is written only on stage transitions and escalations. Heartbeats and deadlines MUST NOT be written to GitHub.
- **GH-5.** The status comment is the comment whose id is in the journal or, on rebuild, the earliest App-authored comment whose body starts with the marker block. A marker anywhere else, including other App-authored comments, issue bodies and pull request bodies, is ignored. Dedupe markers (8.5) are recognised only as the first line of a comment or issue body authored by the App.
- **GH-5a.** All text that originates from an agent, an issue, a review or a log and is written to GitHub passes through one outbound encoder. It caps length, renders the text inside a fenced block labelled as agent output, and neutralises `<!--`, `-->`, @mentions, issue references and closing keywords. Agent text never starts a body. Commit subjects have no markup to escape, so a subject containing an issue reference, an issue URL, a closing keyword or a skip token (9.4) is rejected instead.
- **GH-5b.** On recovery and rebuild, `policy_sha` and `base_sha` must be ancestors of the default-branch head; counters and spend from a marker are lower bounds; the resumed stage is the earlier of the marker's stage and what GitHub evidence supports (8.4).
- **GH-6.** Readers MUST preserve unknown fields. The `v1` version tag changes only on incompatible changes.
- **GH-7.** Apart from the status comment, a run may create a bounded number of comments (default 6 *(tunable)*): the plan, one per escalation, and one when work completes. Progress never goes into new comments.

### 5.5 Check runs

The factory reports three check runs on every head SHA it publishes, with `external_id` set to the run id:

| Check | Content |
|---|---|
| `factory/integrity` | Protected-path and tampering policy (11.3). |
| `factory/verify` | Result of the repository's verification commands on the candidate export (11.2). |
| `factory/review` | Per-criterion verdict of the reviewer agent (11.5). |

- **GH-8.** Conclusions: gate passed → `success`; integrity `flag` → `neutral` with the flagged rules in the summary; stage disabled by policy, not applicable to the tier, or not yet implemented → `skipped` with the reason; gate failed → `failure`. GitHub treats `success`, `neutral` and `skipped` as satisfying a required check, and so does 12.3.
- **GH-9.** A factory check is reported for a SHA only when the gate ran on exactly that commit's tree.
- **GH-10.** In `enforced` mode the ruleset requires the three factory checks, bound to the factory App as their source, plus the repository's CI checks. Because required checks apply to every pull request, the handling of human-authored pull requests is defined in 20.3.

Repository CI reports its own checks through GitHub Actions (11.4).

### 5.6 Branch, pull request and commits

- Branch: `factory/issue-<n>`, one per issue, created by the factory and owned by it. It only ever fast-forwards, except on a reset (7.6), which force-updates it with a lease.
- Pull request: at most one open pull request per branch, opened as a draft at first publish, targeting the default branch, with `Closes #<n>` in the body and the label `factory:origin`. A reset closes the old pull request and opens a new one that links it.
- Commit author and committer: the App's bot identity (`<id>+<slug>[bot]@users.noreply.github.com`). Dates come from the engine's injected clock and are journalled.
- Commit message: a one-line subject from the agent's result, validated (9.4); the body and trailers are written by the host. Trailers: `Factory-Issue: <n>`, `Factory-Run: <run id>`, `Factory-Agent: <cli>/<version> <model>`, and `Co-authored-by:` naming the approver.
- Merge method: squash. The factory passes an explicit, host-generated squash title and message (with the trailers) in the merge call, so the default squash text never pulls in pull request body text. With merge strategy `human` or `native_queue` the trailers on the default branch are not guaranteed and the pull request body is the provenance record.

v1 publishes with `git push` over HTTPS, so commits are unsigned. A ruleset that requires signed commits is incompatible with v1. Publishing through GraphQL `createCommitOnBranch` (GitHub-signed) is a later option limited to change sets it can express.

### 5.7 Identity and permissions

The factory authenticates as a GitHub App created by `sf app create` (manifest flow).

| Permission | Level | Use |
|---|---|---|
| Metadata | read | Required |
| Contents | write | Push the work branch |
| Issues | write | Labels, status comment, follow-up issues, dependencies |
| Pull requests | write | Open, update, mark ready, merge |
| Checks | write | Report factory checks |
| Actions | read | Read CI results and logs |
| Commit statuses | read | Read legacy statuses |

- **GH-11.** The App MUST NOT hold the Workflows or Administration permission and MUST NOT be on any ruleset bypass list. Without Workflows, GitHub rejects pushes that change `.github/workflows/`; the integrity check (11.3) does not rely on that alone, and onboarding accounts for it (20.1).
- **GH-12.** Installation tokens are minted in the control plane, restricted to the one repository in use, cached until shortly before expiry, and treated as opaque strings of unknown length.

### 5.8 GitHub gateway

All GitHub traffic passes through one in-process gateway.

- **GH-13.** Mutations go through a single serial queue with at least 1 s between requests. Nothing in the queue is dropped. A pending write to a comment or check run may only be superseded by a newer write to the same object.
- **GH-14.** Reads run with bounded concurrency (default 8), use an ETag cache with `If-None-Match`, and follow `x-ratelimit-*` and `retry-after` headers with exponential backoff and jitter. The HTTP client's own retry is disabled; every retry goes through the gateway.
- **GH-15.** Priority applies between runs, never within one: claim and merge first, then state transitions (which include marker writes), then check runs.
- **GH-16.** The gateway keeps content-creating requests under 250 per hour per installation *(tunable; GitHub's documented secondary limit is about 500 per hour and 80 per minute)* and pauses dispatch when the budget is nearly used.
- **GH-17.** REST is used for writes and simple reads, with a pinned `X-GitHub-Api-Version` (currently `2026-03-10`). GraphQL is used for composite reads (issue with labels, events, blockers, sub-issues, linked pull request and check rollup) and GraphQL-only mutations.

### 5.9 Event intake

- **GH-18.** The source of truth for change detection is a level-triggered poll: `GET /repos/{o}/{r}/issues?state=all&sort=updated&since=<watermark>` with an ETag, every 30 s *(tunable)*, refreshing only the items that changed. Open pull requests of active runs are polled for reviews and check results on the same tick.
- **GH-19.** A full resync of all open work items runs every hour to catch anything the watermark missed.
- **GH-20.** The factory needs no inbound network endpoint. Webhooks are out of scope for v1; if added later they only enqueue "reconcile issue n now".

---

## 6. Work items and specs

### 6.1 Tiers

| Tier | Use | Required before `state:ready` | Plan gate |
|---|---|---|---|
| 0 | Chore: dependency bump, typo, formatting | Outcome and at least one acceptance criterion | No |
| 1 | Task or bug with one verifiable outcome | Full issue form (6.2) | Only if `risk:high` |
| 2 | Feature spanning several issues | A parent issue describing the feature | Always |

A tier 2 parent is never implemented directly. It follows the parent pipeline (7.4): its plan proposes sub-issues, and after plan approval the factory creates them.

### 6.2 Issue form

Task issues are created from an issue form (`.github/ISSUE_TEMPLATE/factory-task.yml`) that renders these sections, in this order, as level-3 headings:

| Section | Content |
|---|---|
| Outcome | One or two sentences: what is true when this is done. |
| Spec refs | Requirement ids and the spec file path, or "none". |
| Context | Crates, modules or files likely involved; links. |
| Acceptance criteria | A list `AC-1 … AC-n`. Each has a statement in the form *WHEN ⟨condition⟩ THE SYSTEM SHALL ⟨behaviour⟩* and a verification method: `test`, `property`, `review` or `manual`. |
| Out of scope | What must not change. |
| Stop conditions | Situations in which the agent must stop and ask. |

- **WI-1.** The factory parses the body with a tolerant section parser keyed on heading text. A round-trip test against real form output is part of the M0 probe suite, because GitHub does not document the rendered layout.
- **WI-2.** A `[NEEDS CLARIFICATION: …]` marker anywhere in the body blocks dispatch.

### 6.3 Specs in a target repository

- Living specs live at `docs/specs/<capability>/spec.md`. Each requirement has a stable id `R-<CAP>-<NNN>` and acceptance scenarios.
- A tier 2 change proposes a delta under `docs/changes/<parent issue>-<slug>/` with `ADDED`, `MODIFIED` and `REMOVED` requirement sections. Exactly one sub-issue of a tier 2 plan is the spec-delta issue (`type:spec-delta`); it is blocked by all its siblings and merges the delta into the living spec.
- Spec files are protected paths (11.3): changing them outside a spec-delta issue trips a flag.

The format is implemented natively. The factory does not shell out to spec-kit, OpenSpec or Kiro.

### 6.4 Readiness lint

`sf issue lint <n>` and the dispatcher both run the same check. An issue is ready when: required sections for its tier are present and non-empty; every acceptance criterion has a verification method; no clarification marker exists; spec refs resolve at the default-branch head or in the parent's delta; the title contains no skip token (9.4); the body is under 32 KiB after sanitisation (13.2).

A lint failure creates no run. The factory moves the issue to `state:needs-human` with `needs:clarification` and posts one comment listing the failed rules, keyed `<!-- factory:lint <sha256 of title and body> -->`. `state:ready` on a `needs-human` issue that has no run is an ordinary dispatch candidate.

### 6.5 Discovered work

An agent MUST NOT widen its issue. It reports out-of-scope findings in the `follow_ups` field of its result (10.5). The control plane files each as a new issue in `state:triage` with `factory:origin`, linked to the source issue, up to 3 per run *(tunable)*. Factory-filed issues are never auto-approved; the only issues the factory creates in `state:ready` are sub-issues of a human-approved tier 2 plan, under SEC-1a.

---

## 7. Lifecycle

### 7.1 Two levels

The **public state** is the `state:*` label: five values that humans read and set. The **internal stage** is the run's position in the pipeline; it appears in the status comment and in check runs, never as a label.

### 7.2 Public state transitions

| From | To | Trigger | Who |
|---|---|---|---|
| (new) | `triage` | Issue opened from a form, or filed by the factory | Human or factory |
| `triage` | `ready` | Label applied | Authorised actor only |
| `ready` | `in-progress` | Claim (8.2) | Factory |
| `ready` | `needs-human` | Readiness lint fails (6.4) | Factory |
| `ready` | `triage` | Edited after approval (5.3 rule 6), or unauthorised `state:ready` | Factory |
| `in-progress` | `in-review` | All automated gates green; pull request marked ready | Factory |
| `in-progress` or `in-review` | `needs-human` | Escalation (7.5), or a human applies the label | Factory or human |
| `in-review` | `in-progress` | Human review requests changes; a required check turns red; merge conflict | Factory |
| `in-review` | closed (completed) | Pull request merged | GitHub through `Closes #n`, or the factory if auto-close is off |
| `in-progress` | closed (completed) | Tier 2 parent only: all children closed as completed | Factory |
| `needs-human` | `ready` | Label applied (resume signal) | Authorised actor only |
| `needs-human` (via `ready`) | `in-progress` or `in-review` | Resume (7.5.1) | Factory |
| any | closed | Issue closed without a merged pull request | Human; the run is cancelled (7.7) |

`factory:hold` suspends any of these.

### 7.3 Internal pipeline

```
claim → plan → [plan gate] → [author_tests] → implement → capture → integrity → verify
      → publish → ci → review → ready → human_review → merge → done
```

| Stage | Kind | Runs in | Summary | On failure |
|---|---|---|---|---|
| `claim` | host | — | Snapshot, run row, branch ref, label, status comment (8.2) | No run is created |
| `plan` | agent, read-only | sandbox | Return a plan: approach, files, test strategy, risks, questions (10.5). Skipped for tier 0. | Agent failure (8.6) |
| plan gate | wait | GitHub | Required for tier 2 and `risk:high`; otherwise skipped (7.4) | — |
| `author_tests` | agent | sandbox | Optional (M3). A separate session writes failing acceptance tests from the criteria; their paths are then locked for `implement`. | Escalate |
| `implement` | agent, sole writer | sandbox | Change the workspace to satisfy the criteria; emit a result (10.5). | Agent failure (8.6) |
| `capture` | host + sandboxed git | sandbox | Build the candidate commit from the workspace and journal its SHA (9.4). | Rejected paths → `implement`, verify round +1 |
| `integrity` | deterministic | host | Diff policy on the candidate (11.3). | `fail` → `implement`, verify round +1; `flag` → continue, mark the pull request |
| `verify` | deterministic | sandbox | Run the policy's commands on a clean export of the candidate (11.2). | → `implement`, verify round +1 |
| `publish` | host | — | Push the journalled candidate; upsert the draft pull request; report the factory checks. | Transient retry; foreign commit → escalate |
| `ci` | wait | GitHub | Wait for the required CI checks on the head SHA (11.4). | → `implement` with the failing log excerpt, CI round +1 |
| `review` | agent, read-only | sandbox | Clean-context reviewer returns a verdict (11.5). | Blocking findings → `implement`, review round +1 |
| `ready` | host | — | Evidence manifest written; pull request marked ready; label `state:in-review`; review requested (4.2). | Cannot fail: its preconditions are checked at the end of `implement` and `review` |
| `human_review` | wait | GitHub | Wait for a submitted review (12.2). | Changes requested → `implement`, human round +1 |
| `merge` | host | — | Merge coordinator (12.3). | Conflict → base update (12.4) |
| `done` | host | — | Final status comment; run directory scheduled for removal. | — |

After any return to `implement`, the pipeline re-runs every later stage in order on the new candidate. A change after review always passes `capture`, `integrity`, `verify`, `ci` and `review` again.

**Round caps** *(tunable)*: plan 2, verify 3, CI 2, review 2, human 3, merge-conflict 2. Exceeding a cap escalates, except the merge-conflict cap, which triggers one automatic reset (7.6) if the run has not been reset before.

**Slots.** A run holds a concurrency slot from `claim` through `review`, including `ci`. It releases the slot in `human_review`, at the plan gate, in `await_children`, in `needs-human` and on hold, and re-acquires one before any return to an agent stage.

### 7.4 Plan gate and tier 2 parents

**Plan gate.** When the gate applies, the factory posts the plan as one comment, records `plan_sha256` in the marker, removes any `factory:plan-approved` label, and moves the item to `state:needs-human` with category `plan-approval`.

- **PLAN-1.** The plan comment is rendered by the host from the planner's result in a fixed, parseable layout: the plan text, then for tier 2 every proposed sub-issue (key, title, risk, blocked-by edges, full body). `plan_sha256` is the SHA-256 of the comment body as posted. A plan that does not fit one comment (60,000 characters) is a planner failure with a request to reduce.
- **PLAN-2.** An authorised actor approves by applying `factory:plan-approved` and then `state:ready`. The approval is valid only if its `labeled` event is later than the last edit of the plan comment and of the issue body, and the comment's current SHA-256 equals `plan_sha256`. The factory removes the label when it consumes an approval.
- **PLAN-3.** `state:ready` without a valid approval is a revision request: `plan` runs again with the updated snapshot (plan round +1).
- **PLAN-4.** A plan proposes at most 10 sub-issues *(tunable)*. Each proposed body must pass the tier 1 readiness lint before the plan is posted.

**Tier 2 parent pipeline:** `claim → plan → plan gate → decompose → await_children → done`.

| Stage | Kind | Summary | On failure |
|---|---|---|---|
| `decompose` | host | Re-read the approved plan comment from GitHub, check its hash, and create the sub-issues and blocked-by links by parsing it, never from agent output or a local copy. Record `children` in the marker. | Transient retry; otherwise escalate `infrastructure` |
| `await_children` | wait | Holds no slot. All children closed as completed → close the parent as completed. A child closed otherwise → escalate with `clarification`. | — |

The parent has a fence ref with no commits and no pull request; the ref is deleted when the parent's run ends. `factory:restart` on a parent in `await_children` is rejected with a comment.

### 7.5 Escalation

`needs-human` is a durable state, not an error. The run stays non-terminal and keeps its run row, branch, pull request and workspace. On escalation the factory:

1. Interrupts any stage execution and releases the concurrency slot.
2. Sets `state:needs-human` and records `escalation` in the marker with a category: `clarification`, `spec-conflict`, `plan-approval`, `budget`, `verification`, `review`, `merge-conflict`, `policy-denial`, `spec-changed`, `infrastructure` or `human-request`.
3. Posts one comment: what was tried, the evidence (links to checks, short log excerpts), one specific question or decision, and how to resume.
4. Mentions the approver once. It does not repeat the mention.

An agent may also stop on its own with status `needs_human` (a stop condition was met, the criteria conflict, a criterion is inapplicable, or the task is impossible as written). This is a valid, unpenalised outcome and does not count against retry caps.

A human applying `state:needs-human` to an active run is an escalation with category `human-request`.

While a run is in `needs-human`, the reconciler rows of 7.7 are not evaluated for it, except issue closure, hold and stop. They are evaluated once at resume.

#### 7.5.1 Resume

A human resumes by answering (editing the issue or commenting) and applying `state:ready`. When the reconciler sees `state:ready` on an issue whose run is in `needs-human`, it does not run the claim. It:

1. Validates the label under SEC-1. If invalid, it removes `state:ready` and leaves `state:needs-human`.
2. Re-evaluates readiness rules 2–4 and 6 of 5.3 and waits for a slot. The issue keeps `state:ready` while it waits.
3. Adds `state:in-progress` (or `state:in-review` if the recorded stage is `human_review` or `merge`) and removes the other state labels.
4. Takes a new snapshot.
5. Sets the round counter of the gate that escalated to 0. For category `budget` it grants one more issue-budget increment (section 14) and counts it in `budget_grants`.
6. Chooses the re-entry stage:
   - Title and body unchanged (`title_body_sha256` matches): the recorded stage. A new trusted comment alone never forces a re-plan.
   - Title or body changed, and every edit since the last snapshot is by an authorised actor: `plan`, and any plan approval is void. If the recorded stage is `human_review` or `merge`, the change is noted and the run resumes at the recorded stage.
   - An edit by anyone else, or by an editor that cannot be identified: no resume. `state:ready` is removed and the run stays in `needs-human` with category `spec-changed`.

### 7.6 Rework and reset

- **In-place rework** is the default: rounds add commits to the same branch and pull request.
- **Reset**, in order: close the pull request; force-update `refs/heads/factory/issue-<n>` to the current default-branch head with a lease (the ref is never deleted, so the claim fence holds); discard the workspace and run-local caches; take a new `base_sha`, `policy_sha` and snapshot; zero all round counters; keep spend; attempt +1. The new attempt starts at `plan` with a handoff note saying what failed. Plan approval and locked tests do not carry over, so a plan gate that applied runs again.
- A reset happens on `factory:restart`, or automatically once when the merge-conflict cap is reached. Attempts are capped at 2 *(tunable)*; a further reset request escalates instead.
- Every reset and every human rejection SHOULD be linked to a follow-up issue that changes guidance, a lint or a verifier (15.4).

### 7.7 Changes during a run

On every tick the reconciler compares each active run with GitHub. Rows are evaluated in this order:

| Observation | Action |
|---|---|
| The run's pull request is merged | Enter `done`, whatever the merge strategy and whoever merged. If the issue is still open, close it as completed. If the merged head is not one the factory pushed, record `foreign-merge` in the final status comment and on the dashboard. |
| Issue closed and the pull request is not merged | Cancel: run → `cancelled`; close the pull request; delete the branch per 18.2. |
| `state:triage` applied by a human | Cancel as above, leaving the issue open. |
| `state:needs-human` applied by a human | Escalate with `human-request`. |
| `state:ready` applied while the run is `in-progress` or `in-review` | Remove it and ignore it. |
| `factory:hold` applied | Interrupt and freeze. |
| `tier:*`, `risk:*` or `type:*` changed after claim | Escalate with `spec-changed`. |
| Title or body changed by an authorised actor before the `ready` stage | Interrupt, take a new snapshot, restart from `plan` in place. Not counted as a failure. |
| Title or body changed by anyone else, or the editor cannot be identified | Escalate with `spec-changed`. |
| Title or body changed after the pull request is ready | Note it in the status comment; no automatic action. |
| Pull request closed by a human without merging | Escalate with `review`. |
| Branch head differs from every SHA the factory pushed | Foreign commit: escalate with `policy-denial`; never push over it. |

---

## 8. Orchestrator

### 8.1 Engine

- **ORCH-1.** The engine is a pure function `step(State, Event) -> (State, Vec<Effect>)` with no I/O, clock or randomness. Time and ids arrive as events or injected values.
- **ORCH-2.** One engine task owns all run state. The `runs` row is the authoritative run state; the `events` table is an audit record and is never replayed to derive state.
- **ORCH-3.** Each transition is committed in one transaction: append the event, update the run row, record the effects. Effects fall into three classes:

| Class | Examples | Rule |
|---|---|---|
| External write | GitHub call, push | Stored in the outbox with an effect id; delivered at least once, in FIFO order per run. Completion is one transaction that marks the row done and appends the result event carrying the effect id. `step` ignores a result whose effect id it has already applied. |
| Activity | Agent session, setup, capture, verify | Never in the outbox. Recorded as an activity row (id, sandbox record, process data, state). After a restart it is killed and marked abandoned; the engine receives `ActivityAbandoned` and requests a new one with a new id. |
| Timer | Stage deadlines, backoff, poll | Stored as a deadline on the run row and re-armed from the store at startup. |

- **ORCH-4.** The engine receives only coarse activity events: `ActivityStarted`, `UsageBatch` (all per-model-call usage records since the last batch, at most once per 5 s and always before the end event), `LimitReached(kind)`, `LoopDetected`, `Stalled`, `ActivityEnded(outcome, chain_digest)`. Idle, loop and per-session limits are evaluated in the supervisor from limits passed in the session spec. Raw and normalised agent events go only to `events.jsonl`, which the supervisor hash-chains. The supervisor's stdout reader never blocks on the engine channel.
- **ORCH-5.** `step` runs under `catch_unwind` per run; a panic moves that run to `needs-human` with `infrastructure`. A panic or I/O error in the store writer stops the daemon with a non-zero exit; restart recovery (8.4) is the handling.

### 8.2 Claim

The daemon is the only dispatcher for a store, enforced by an exclusive file lock on the data directory. Claiming an issue is, in order:

1. One GraphQL read of the issue: check SEC-1 on the `state:ready` label and rule 6 of 5.3; capture the snapshot; record tier, risk and type under SEC-1. Read the default-branch head once: it is both `base_sha` and `policy_sha`. Refuse a base tree that contains a submodule entry. Run the readiness lint on the snapshot.
2. Insert the run row carrying all of the above. A unique index on (repository, issue) for non-terminal runs makes this the authoritative local claim.
3. Create `refs/heads/factory/issue-<n>` at `base_sha` through the API. An existing ref whose SHA equals the journalled `base_sha` of this run counts as created. A ref left by a `cancelled` run of this store is force-updated. Any other existing ref aborts the claim: the run row is closed as `cancelled` and the issue goes to `state:needs-human` with category `policy-denial`.
4. Add `state:in-progress`, then remove `state:ready`.
5. Create the status comment with the complete marker.

Running two daemons against one repository from different hosts is unsupported. The branch-ref step is a best-effort fence against that mistake, not a distributed lock.

**Deadlines.** There are no leases. The supervisor owns one set of deadlines per stage kind:

| Stage kind | Deadline |
|---|---|
| Agent | Idle: no agent event for 300 s while no tool call is outstanding. Tool call: a single tool call outstanding for more than `tool_call_timeout_secs` (policy, default 1800). Wall clock and budget: section 14. |
| Deterministic (setup, capture, verify) | The command timeout only. |
| Wait: `ci` | `ci.timeout_secs` from the push of the head SHA (default 3600). |
| Wait: `human_review`, plan gate, `await_children` | None; one nudge (PR-3). |

If the supervising task itself dies, the activity row is found on the next reconcile, its processes are killed, and the stage restarts (counts toward Poison, 8.6).

### 8.3 Scheduling

Each tick, in order: reconcile active runs (7.7); validate policy (16.3) and the workflow audit (VAL-6a); fetch candidates; sort; dispatch while limits allow.

| Limit | Default *(tunable)* |
|---|---|
| Concurrent runs holding a slot, global | 4 |
| Concurrent runs holding a slot, per repository | 2 |
| Concurrent merges per repository | 1 (fixed) |
| Factory pull requests awaiting human review per repository | 3 |
| Free space on the data volume | No dispatch below `min_free_bytes` (operator) |
| Run directory size | Per-run quota, checked at stage boundaries and every 5 minutes; exceeding it escalates with `budget` |
| Spend (section 14) | per stage, per issue, per day |

When the review-queue limit is reached, no new `implement` stage starts in that repository; runs already past `implement` continue. Dispatch also pauses on provider rate-limit events, on the GitHub write budget (GH-16), and when a circuit breaker is open (8.6).

### 8.4 Restart recovery

On startup the daemon, before dispatching anything:

1. Takes the store lock.
2. Reaps every activity recorded as live: destroys its sandbox instance and sweeps for surviving processes (AG-18).
3. For each non-terminal run, compares the `runs` row with GitHub (labels, marker, branch head, pull request, checks) and with the run directory. GitHub wins. The run resumes at the start of its recorded stage, replaying journalled sub-steps (a journalled candidate SHA, a journalled push) instead of recomputing them.
4. If the store is missing or corrupt, `sf rebuild` reconstructs runs from status-comment markers, branches and pull requests, under GH-5b:
   - The approver is re-read from the latest `labeled` event for `state:ready`.
   - A branch head that differs from the marker's `head_sha` is accepted only if it carries a `factory/integrity` check run from the factory App with this run id; otherwise it is a foreign commit.
   - A run whose marker stage is `ci`, `ready`, `human_review` or `merge` keeps that stage. A run rebuilt in an agent or deterministic stage restarts at `plan` with a workspace created from `head_sha`, because its handoff files are gone.

### 8.5 Idempotency keys

| Side effect | Key and replay rule |
|---|---|
| Branch | `base_sha` is journalled before the ref is created; an existing ref equal to it counts as created (8.2). |
| Candidate commit | Tree, parent, dates and message are journalled with the candidate SHA before any push. Recovery uses the journalled SHA and never rebuilds a journalled candidate. If the tree equals the previous candidate's, no new commit is made. |
| Push | The executor reads the remote ref first. Remote head equals the candidate: pushed. Remote head equals the previously journalled head: push. Anything else: foreign commit (7.7). |
| Pull request | Lookup of open pull requests by head branch before create. |
| Status comment | Comment id in the journal; on rebuild, GH-5. |
| Other comments | First-line marker `<!-- factory:event <run>/<seq> -->` in an App-authored comment, looked up before posting. |
| Check run | List check runs for the head SHA filtered by name and App id; update the one whose `external_id` matches, else create. |
| Merge | Read the pull request before every merge call: merged with our head SHA is success. A 409 means a request is pending: poll the pull request's merged state for up to 10 minutes, then re-check eligibility. |
| Agent session | Session id generated and journalled before spawn; a restarted activity gets a new id. |
| Follow-up issue, sub-issue | First-line marker `<!-- factory:followup <run>/<index> -->` or `<!-- factory:subissue <run>/<plan sha256>/<key> -->` in the body; lookup by listing issues created by the App since the run started, never the search API. |

### 8.6 Failure handling

| Class | Examples | Handling |
|---|---|---|
| Transient | Network error, 5xx, provider overload, secondary rate limit | Exponential backoff with jitter, capped at 5 minutes; no round cost |
| Deterministic | 401, 403, 422, invalid policy, missing sandbox | No retry; escalate with `infrastructure` or pause dispatch |
| Gate failure | Capture, integrity, verify, CI or review failed | Bounded rounds (7.3) |
| Agent failure | Status `failed`, no result, schema-invalid result | Restart the stage once with a fresh session and the failure summary in the handoff; then escalate with `verification` |
| Stall | An agent deadline of 8.2 passed | Interrupt; restart the stage once with a fresh session; then escalate |
| Budget | Any limit in section 14 reached | Stop the stage; escalate with `budget` |
| Poison | The same stage was abandoned or restarted 3 times | Escalate with `infrastructure` |
| Systemic | 5 consecutive runs fail with the same class *(tunable)* | Open the circuit breaker: pause all dispatch, report through `sf status` and the dashboard issue (15.5) |

### 8.7 Loop and no-progress detection

- **In session:** the supervisor hashes each tool call (name and arguments). A repeating pattern of period 1–3 within the last 10 calls *(tunable)* triggers an interrupt and a restart of the stage with a note describing the loop. A second detection escalates.
- **Across rounds:** if two consecutive rounds end with the same failing-check fingerprint or the same candidate tree, the run escalates without using its remaining rounds.

---

## 9. Workspaces and git

### 9.1 Layout

```
<data_dir>/
  store.db                         journal (section 18)
  mirrors/<owner>/<repo>.git       bare mirror, fetched by the host only
  runs/<run_id>/
    workspace/                     implementer's clone (untrusted)
    host.git/                      host-owned bare repository for candidates and publishing
    quarantine/                    object directory written by sandboxed capture
    export/<sha>/                  clean checkout of a candidate, for verify and review
    target/                        CARGO_TARGET_DIR for agent stages
    handoff/                       plan.md, progress.md, feedback/*.md, diff files (host-written)
    exec/<seq>/home/               fresh HOME, CARGO_HOME, temp and agent config per stage execution
    events.jsonl                   raw agent event stream, hash-chained
    stages/<seq>-<stage>.json      stage results
    logs/                          full command output
```

`handoff/` and `logs/` are host-written. Workers see a per-role, read-only view of them outside the workspace root: the reviewer's view excludes `progress.md` and implementer feedback. The host writes `progress.md` from the `notes` field of each result, so it never reads a file out of a workspace.

### 9.2 Workspace creation

- **WS-1.** The host fetches into the bare mirror, then creates `workspace/` with `git clone --no-local --no-checkout --single-branch --branch <default> <mirror> workspace`, checks out `base_sha` detached and removes the remote, all before any worker touches the directory. `--reference`, `--shared`, hard-linked and plain local-path clones are not used: the workspace MUST contain its own object store holding only objects reachable from the base, because the mirror is not visible inside the sandbox and holds other runs' unmerged branches.
- **WS-2.** Directory names are derived from ids, never from issue text. The agent's working directory MUST be the workspace root.
- **WS-3.** The agent MAY run git in its workspace, including committing for its own checkpoints. Nothing in `workspace/.git` is ever used by the host.
- **WS-4.** Policy may define `setup` commands (for example `cargo fetch --locked`). They run inside the sandbox, with a timeout, at the start of every stage execution that builds: each implementer session and each verify.
- **WS-5.** Run directories are removed 24 hours after a terminal state *(tunable)*; `target/` and exports are removed earlier (9.3).
- **WS-6.** The workspace tree MUST derive from the previous candidate. After a base update (12.4) the host discards `workspace/` and re-creates it from the update commit, so a later round can never revert what landed upstream.
- **WS-7.** The host resolves the toolchain named by `rust-toolchain.toml` at the base commit (channel, components, targets), read as data from the object database, to an installed toolchain directory and puts that directory's `bin` first on the worker `PATH`. rustup and mise shims are never on the worker `PATH`. A missing toolchain or component is a blocking onboarding failure.

Linked worktrees are not used: they share refs, config and hooks with the hub repository, which conflicts with INV-2.

### 9.3 Build output

- `CARGO_TARGET_DIR` is outside the workspace: `runs/<run_id>/target` for agent stages. `verify` gets a new, empty target directory on every execution, so a build is cold each time. That cost is accepted; CI is the second clean-environment gate.
- `target/` and exports are deleted when the run enters `human_review` or a terminal state.
- No cache writable by a worker is shared between runs or between an agent stage and `verify`. `sccache` is not supported in v1. Seeding target directories from a host-built copy-on-write clone MAY be added after probe P-13 measures it.

### 9.4 Capture: building the candidate

Capture runs after the implementer's sandbox instance is destroyed and the process sweep (AG-18) is empty.

1. **Scan (host, `lstat` only, never following symlinks).** Fail if any entry other than the root `.git` has a name that git would treat as `.git` (case-insensitive, including filesystem-equivalent spellings), at any depth; or if any entry is not a regular file, directory or symlink.
2. **Stage (sandboxed git).** In a new sandbox instance with the workspace mounted read-only, no network and no credentials, the host's git binary runs with a host-written configuration (hooks, filters, fsmonitor, system and global config all disabled). Its object directory is `quarantine/`, with `host.git` objects as a read-only alternate. It builds a fresh index from the previous candidate's tree (or `base_sha`), lists changed and untracked paths honouring ignore rules, and rejects any path that is neither a regular file nor a symlink or exceeds 5 MiB, and any change set over the total-bytes cap. If nothing is rejected it stages the listed paths and writes a tree. A rejected path fails the capture; the host never builds a candidate that silently omits one.
3. **Ingest (host, bare `host.git`).** Verify the quarantined objects (hashes recomputed, `fsck` checks), then walk the new tree and reject any entry whose mode is not `100644`, `100755` or `120000`. Submodule entries therefore cannot enter a candidate.
4. **Commit.** `git commit-tree` on the previous published head (or `base_sha`), with the factory identity, journalled dates, the validated subject and the host-written body and trailers. Journal the candidate and tree SHA.

**Subject validation.** The subject is a single line of at most 72 characters without control characters. It is rejected, returning to `implement`, if it contains (case-insensitively) `[skip ci]`, `[ci skip]`, `[no ci]`, `[skip actions]`, `[actions skip]` or `skip-checks`, or anything GH-5a forbids. The pull request title and the squash title and message are generated by the host under the same rule.

**Export.** `verify` and `review` use a clean checkout of the candidate made from `host.git` with a temporary index and `git checkout-index -a --prefix=runs/<run_id>/export/<sha>/`, with no filters or attributes applied. `git archive` MUST NOT be used, because it honours `export-ignore` from the candidate's own `.gitattributes`. The export is writable and disposable for `verify`, read-only for `review`.

**Publish.** Push the journalled candidate to `refs/heads/factory/issue-<n>` over HTTPS. The installation token reaches `git push` through a credential helper built into `sf` that receives it over an inherited pipe; it never appears in arguments, environment or a file (INV-1). Then create the draft pull request if no open one exists for the head branch, otherwise update its body, and report the three factory checks for the head SHA.

- **WS-8.** All git mutations run through the `git` CLI in one wrapper crate that parses only porcelain or NUL-delimited output. `gix` MAY be used for read-only fast paths. `git2` is not used. Fetches into a mirror are serialised per repository.
- **WS-9.** The capture, ingest and export design MUST be prototyped in M0 against a hostile-tree suite before the pipeline depends on it: nested repositories (directory, gitfile, `.GIT` case variant) carrying `core.fsmonitor`; hooks and config; symlinked directories; FIFOs and hard links; `.gitattributes` with filters, `export-ignore` and `-diff`; an agent-edited `.gitignore`; LFS pointers; case collisions; executable bits; large files; an object from another run's branch must not be readable in a fresh workspace. The exact staging mechanism is confirmed by probe P-7.

---

## 10. Agent execution

### 10.1 Driver interface

```rust
trait AgentDriver {
    fn capabilities(&self) -> Capabilities;        // resume, interrupt, structured_output, budget_cap, reports_cost
    async fn start(&self, spec: SessionSpec, sandbox: &dyn SandboxInstance) -> Result<SessionHandle>;
}

struct SessionSpec {
    session_id: Uuid,            // generated and journalled by the engine
    role: Role,                  // Planner | TestAuthor | Implementer | Reviewer
    cwd: PathBuf,                // workspace (planner, implementer) or candidate export (reviewer)
    system_prompt: String,       // byte-stable per stage and policy version
    prompt: String,
    result_schema: serde_json::Value,
    tools: ToolPolicy,           // allowlist
    model: ModelSpec,            // id, effort
    limits: Limits,              // turns, usd (min of stage cap and remaining issue budget), wall clock, idle, tool call
    env: BTreeMap<String, String>,
}

// SessionHandle: events() -> Stream<AgentEvent>; interrupt(); kill(); wait() -> SessionOutcome
```

`AgentEvent` is a normalised enum: `Init`, `AssistantText`, `ToolCall`, `ToolResult`, `Usage`, `RateLimited`, `Result`, `Unknown(raw)`. Unknown message types and fields are preserved, never fatal.

v1 ships one driver, for Claude Code. A Codex driver and a generic Agent Client Protocol driver are later additions behind the same trait; the ACP crate has had four major versions in five months, so it is not a v1 dependency.

### 10.2 Roles

| Role | File access | Context given |
|---|---|---|
| Planner | Workspace mounted read-only | Snapshot, repository instructions |
| Test author (M3) | Workspace, test paths only | Snapshot, plan |
| Implementer | Workspace read-write | Snapshot, plan, `progress.md`, feedback from failed gates |
| Reviewer | Candidate export mounted read-only | Snapshot, plan, the diff `base_sha..head`. Never the implementer's transcript, `progress.md` or feedback. |

Read-only roles are enforced by the sandbox mount mode, not by the tool allowlist alone. Sessions run one at a time within a run. The reviewer SHOULD be configurable to a different model than the implementer.

### 10.3 Claude Code driver

The driver runs the CLI as a subprocess and speaks its stream-JSON protocol. No community SDK crate is used; there is no official Rust SDK.

- **AG-1.** Every invocation is headless and bare: `claude -p --bare --setting-sources user --output-format stream-json --verbose --input-format stream-json`, with an explicit `--session-id`, `--model`, `--permission-mode`, `--permission-prompts none`, tool allowlist (`--tools` / `--allowedTools`), `--settings <file>`, `--mcp-config <file> --strict-mcp-config`, `--json-schema <inline schema>`, `--max-turns`, `--max-budget-usd` and `--append-system-prompt-file <file>`. Flag names are those of Claude Code 2.1.311. The driver owns the mapping. `sf doctor` verifies each flag by invoking the CLI with it against a stub model endpoint and checking for an unknown-option error; it does not parse `--help`, which omits some supported flags.
- **AG-2.** Bare mode skips the repository's `CLAUDE.md`, hooks, skills, plugins and `.mcp.json`, but project settings files can still apply an `env` block and helper commands. `--setting-sources user` is therefore mandatory, so no project or local settings file is read. This is intended (INV-2, INV-10).
- **AG-2a.** Repository instructions are computed by the host at the policy commit: `AGENTS.md` and the files it references, to at most 2 levels, within a size cap, only inside directories that policy declares (default `docs/**`), resolved only to regular blobs read from the object database. Symlink entries and paths containing `..` are rejected; HTML comments are stripped. The implementer and planner receive them through the system prompt file. The reviewer's system prompt contains only the factory's role prompt; repository instructions reach it in a delimited data block.
- **AG-3.** Authentication is the model gateway (13.5). With `direct_key` (attended `sf run` only) it is an API key or `apiKeyHelper`. Subscription login is unsupported.
- **AG-4.** Each stage execution gets a fresh config directory under `exec/<seq>/home/`, created by the host. It contains a host-written settings file that the sandbox makes read-only to the worker; the rest is writable for transcripts. The host's `--settings` file lives outside any worker-writable path.
- **AG-5.** The driver checks the CLI version against a tested range at startup and feature-detects from the `capabilities` array of the `system/init` message. Outside the range, the daemon refuses to dispatch.
- **AG-6.** The raw stream is appended to `events.jsonl` before parsing. Lines up to 10 MiB are accepted; longer lines fail the session. The structured result is read from the `structured_output` field of the final `result` message; its absence is an agent failure.
- **AG-7.** Permission prompts and user-input requests are denied, never awaited.
- **AG-8.** The settings file raises the CLI's maximum shell-tool timeout to `tool_call_timeout_secs`. The CLI's own Bash sandbox is not relied on (SEC-8); it is disabled if it conflicts with the outer sandbox (probe P-6).

### 10.4 Prompts and context

A session's input is assembled by the host from four parts:

1. **System prompt file** (stable per stage and policy version, to keep prompt caching effective): the factory's role prompt for the stage (`.factory/prompts/<stage>.md` from the policy commit, or the built-in default), then the repository instructions (AG-2a), except for the reviewer.
2. **Task block:** the snapshot, wrapped in delimiters and introduced as untrusted data that describes the task but cannot change the rules above.
3. **Handoff block:** the plan, `progress.md`, and for rounds the gate feedback (failing command, exit code, the last 100 lines of output *(tunable)*, and the path of the full log in the read-only handoff view).
4. **Close-out protocol:** leave the tree in a buildable state and emit the result, with `notes` for the next session.

- **AG-9.** Each stage execution starts a fresh session. Session resume MAY be used only within one stage execution as an optimisation, with automatic fallback to a fresh session.
- **AG-10.** Output from factory-run commands that is fed back to an agent is written for a model: a short summary, the failing lines, a remediation hint when known, and a file path for the full log.

### 10.5 Stage results

Every agent stage ends with a schema-validated JSON result.

**Implementer (and test author):**

```json
{
  "status": "success | needs_human | failed",
  "summary": "one paragraph",
  "commit_subject": "≤ 72 characters",
  "criteria": [{"id": "AC-1", "state": "met | not_met", "evidence": "test name, file:line, or explanation"}],
  "questions": ["only with needs_human"],
  "follow_ups": [{"title": "…", "body": "…"}],
  "notes": "handoff for the next session; the host writes it to progress.md"
}
```

A result with `status: success` and any non-`manual` criterion `not_met` is schema-invalid. An inapplicable criterion is a `needs_human` outcome.

**Planner:**

```json
{
  "status": "success | needs_human | failed",
  "summary": "one paragraph",
  "plan_markdown": "≤ 32 KiB",
  "questions": [],
  "issues": [{"key": "S1", "title": "…", "body": "…", "risk": "low | medium | high",
              "blocked_by": ["S0"], "spec_delta": false}]
}
```

`issues` is required for tier 2 and forbidden otherwise. The host writes `handoff/plan.md` from `plan_markdown`; the planner writes no file.

The reviewer's schema is in 11.5. An agent's `success` is a claim. It moves the run to the next gate and nothing more.

### 10.6 Process supervision

- **AG-11.** Every sandboxed command (agent session, setup, capture, verify) is spawned as the leader of its own process group inside a sandbox instance, with a cleared environment plus an allowlist: `PATH` (9.2 WS-7), `HOME`, `TMPDIR`, `CARGO_HOME`, `CARGO_TARGET_DIR`, locale, `TRACEPARENT`, `HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY`, and the model gateway address and per-run token.
- **AG-12.** Stop sequence: send the protocol interrupt or SIGINT and wait up to 10 s for the result message; close stdin; SIGTERM to the group; after 5 s, SIGKILL to the group and destroy the sandbox instance. After SIGTERM the supervisor does not wait for a result message, because the CLI does not emit one.
- **AG-13.** Stdout is read through a length-capped line codec; stderr is drained concurrently to a log file. Pipes are used, not a PTY. After the leader exits, output is drained for at most 2 s and the pipe handles are then closed, so a grandchild holding a pipe cannot hang the supervisor.
- **AG-14.** The agent's own `--max-turns` and `--max-budget-usd` are set equal to the factory's session limits, so the control plane's threshold (BUD-1) fires first; a CLI-side cap result is handled as Budget.
- **AG-15.** The sandbox record, pid, process group id and process start time are journalled before the first exec.
- **AG-16.** Every sandboxed command is started through a supervisor shim (`sf __supervise`, same binary) that holds a pipe from the daemon and kills its whole tree when that pipe closes, so workers do not outlive a killed daemon. On Linux the sandbox also uses a PID namespace with die-with-parent.
- **AG-17.** `sf` ships a launchd plist and a systemd unit whose stop timeout (45 s) exceeds the daemon's shutdown budget (19.3).
- **AG-18.** After every sandboxed command the supervisor sweeps for survivors: any process whose process group or session equals the journalled one, or whose working directory or executable is under `runs/<run_id>/`. They are killed, and the next stage MUST NOT start until the sweep is empty. The same sweep is the orphan rule of 8.4. On macOS a process that changes session and working directory can escape the sweep; this residual gap is recorded in 23.3.
- **AG-19.** The egress proxy and the model gateway admit connections for a run only while one of its activities is live. This closes the spend and exfiltration path for a process that escaped the sweep.

---

## 11. Validation

Passing tests is not enough evidence that a change is correct, and agents weaken tests they are allowed to edit (report 03). The pipeline is therefore layered, and the implementer controls none of the layers: every gate after `implement` runs on the candidate commit, outside the implementer's environment (INV-11).

### 11.1 Layers

| # | Gate | Decides | Since |
|---|---|---|---|
| 1 | `integrity` — diff policy on the host | Did the change touch what it must not, or weaken tests? | M1: path, structural and secret rules. M3: tampering heuristics. |
| 2 | `verify` — repository commands on a clean export, in a sandbox | Does the committed tree build, lint and pass tests? | M1 |
| 3 | `ci` — GitHub Actions on the pull request | Does it pass in a second clean environment? | M1 |
| 4 | `review` — clean-context reviewer agent | Does the diff satisfy each criterion, and is it sound? | M2 |
| 5 | Human review | Is this acceptable to merge? | M1 |
| 6 | Locked acceptance tests from `author_tests` | Were the criteria tested by someone other than the builder? | M3 |
| 7 | Diff-scoped mutation testing in CI | Do the tests detect changed behaviour? | M3 |
| 8 | Holdout scenarios in a separate private repository | Does it work on checks the builder never saw? | Later, optional |

### 11.2 Verify

Policy lists the commands, for a Rust repository typically:

```toml
[verify]
commands = [
  "cargo fmt --all --check",
  "cargo clippy --workspace --all-targets --locked -- -D warnings",
  "cargo nextest run --workspace --locked",
  "cargo test --doc --workspace --locked",
]
timeout_secs = 1800
```

- **VAL-1.** Commands run on the candidate export in a new sandbox instance with a fresh home and target directory, the same sandbox level as the agent, and no credentials (INV-3, INV-11). `setup` commands run first. A change that adds or alters dependencies makes `cargo build` arbitrary code execution; the sandbox, not the command list, is the control.
- **VAL-2.** `factory/verify` records the tree SHA that was verified and is reported only on a head whose tree equals it (GH-9).

### 11.3 Integrity check

A deterministic check on `base_sha..candidate`, run by the host on every candidate. Each rule yields `pass`, `flag` or `fail`.

| Rule | Outcome |
|---|---|
| Change under a **locked path** (tests written by `author_tests` for this run) | `fail` |
| Change under `.github/workflows/` | `fail`. For a `type:ci` issue the run escalates with `policy-denial` and attaches the proposed workflow as a patch for a human to commit, because the App cannot push it (GH-11). |
| Change under a **protected path**: `.factory/**`, `AGENTS.md`, `CLAUDE.md`, `.claude/**`, `.mcp.json`, the instruction directories of AG-2a and any file in the computed instruction set, `.github/**`, `CODEOWNERS`, `.gitattributes` and `.gitignore` at any depth, `docs/specs/**` (outside a `type:spec-delta` issue), `deny.toml`, `clippy.toml`, `rust-toolchain.toml`, `.cargo/**`, plus policy additions | `flag` |
| New or changed `build.rs`; new dependency or changed dependency source in `Cargo.toml` / `Cargo.lock` | `flag` |
| A changed path carries an attribute that hides or alters its diff (`-diff`, `binary`, `diff=`, `merge`, `filter`, `working-tree-encoding`, `linguist-generated`, `linguist-vendored`) | `flag`, listed in the pull request body as "hidden or collapsed in the GitHub diff"; `fail` if the attribute was added or changed in the same diff |
| A binary change to a path that was text at base | `flag` |
| Diff larger than the size budget (default 600 changed lines excluding lockfiles and generated files *(tunable)*) | First time: `fail` with a request to split (`size_strikes` +1). Second time: `flag`. |
| An added path matching the built-in deny list (`target/**`) | `fail` |
| Secret pattern in added content | `fail` |
| M3 heuristics: deleted or newly ignored tests, falling test count, new `#[allow]` or `#[ignore]`, changed snapshots, hand-written `PartialEq` on types used in assertions | `flag` |

Structural rules (file type, file size, nested repositories, submodule entries) are enforced earlier, in capture (9.4).

- **VAL-3.** A `flag` adds the label `factory:protected-change` to the pull request, lists the flagged rules at the top of its body, and makes it ineligible for auto-merge. The flag lives in the journal and the `factory/integrity` check; removing the label does not clear it (SEC-1b). In `enforced` mode CODEOWNERS additionally requires code-owner review for protected paths.
- **VAL-4.** A `fail` returns to `implement` with the rule and the offending paths, and counts as a verify round.
- **VAL-5.** The built-in protected-path list cannot be shortened by repository policy (16.1). "Generated" and "lockfile" are defined only by glob patterns in default-branch policy, never by attributes. Path patterns match at any depth after case folding and Unicode normalisation.
- **VAL-5a.** Every diff the host computes (integrity, size budget, secret scan, reviewer input, no-progress hash) runs against bare `host.git` with no work tree, with external diff and textconv disabled and with no in-tree attributes applied (attribute source set to the empty tree). A file is treated as binary only by content detection.

### 11.4 Continuous integration

- **VAL-6.** The operator installs the workflow template supplied by `sf repo add` (20.1). It runs on `pull_request` and `merge_group` only, with top-level `permissions: contents: read`, no secrets, GitHub-hosted runners, and actions pinned to full commit SHAs. It SHOULD fetch dependencies first and then build and test in a container without network access, because CI otherwise gives agent-written code an open network.
- **VAL-6a. Workflow audit.** At startup, on every default-branch head change and in `sf repo add`, the factory parses every workflow at the default-branch head. Required from M2. Dispatch in the repository pauses (as for an invalid policy, 16.3) unless:
  - every workflow that runs in the context of a factory ref (`push` matching `factory/**`, `pull_request`, `merge_group`) declares top-level `permissions` with no write scope, references no `secrets.*`, uses no `secrets: inherit`, uses GitHub-hosted runners and pins actions to SHAs;
  - `pull_request_target` and `workflow_run` workflows exclude factory branches;
  - every base-context workflow (`issues`, `issue_comment`, `pull_request_review`, `check_*`, `status`) is acknowledged in operator configuration by blob SHA, checks out no pull request head and interpolates no event text into `run:`; a changed blob pauses dispatch until re-acknowledged.

  Anything the parser cannot resolve (a reusable workflow, an unknown expression in `on:` or `permissions:`) fails closed.
- **VAL-7.** Factory pull requests come from a branch in the same repository, so they do not get the fork sandbox. Release and deployment workflows MUST live behind protected environments and MUST NOT share caches with pull request workflows.
- **VAL-8.** The required CI checks are `required_checks = [{ name, app_id }]` in operator configuration per repository, which policy may extend but not shorten; the list is non-empty for any repository the daemon dispatches on. `name` is the check-run name as the API returns it (the job name). In `enforced` mode the factory cross-checks the list against the ruleset. The `ci` stage passes when every listed check exists on the head SHA with conclusion `success`, `neutral` or `skipped` from the listed App; it fails on any other conclusion (CI round +1); a check still missing at `ci.timeout_secs` counts as failed with class `infrastructure`.
- **VAL-9.** On a red check the factory fetches the failing job's log through the API, extracts the failing step's tail, and feeds it back as gate feedback.
- **VAL-10.** Pushes are made with the App installation token, which triggers workflows normally.

### 11.5 Reviewer

The reviewer is a read-only session with a fresh context on the candidate export. It returns:

```json
{
  "verdict": "approve | request_changes",
  "criteria": [{"id": "AC-1", "satisfied": true, "evidence": "…", "confidence": "high | medium | low"}],
  "findings": [{"severity": "blocking | major | minor", "path": "…", "line": 0, "problem": "…", "suggestion": "…"}],
  "scope_creep": ["changes not required by the issue"]
}
```

- **VAL-11.** The control plane, not the reviewer, computes the gate: it fails if any non-`manual` criterion has `satisfied: false`, any finding is `blocking`, or `scope_creep` is non-empty and policy forbids it. `major` and `minor` findings are listed in the pull request for the human.
- **VAL-12.** The reviewer never approves the pull request on GitHub (INV-5) and is never the only gate before merge.
- **VAL-13.** `review` may be disabled by repository policy for tier 0 only; for other tiers only by operator configuration. The reviewer's precision is measured against human decisions (15.2); a reviewer stage that neither blocks bad changes nor agrees with humans is a candidate for removal.

### 11.6 Evidence manifest

**Criterion evaluation** is computed by the control plane on the head SHA. A criterion whose method is `manual` is excluded from the gates and shown as "manual, for the human reviewer". Any other criterion passes when the implementer's state is `met` and, if the `review` stage ran for this head, the reviewer's `satisfied` is true.

The pull request body is generated by the host from a fixed template, with agent text passed through the outbound encoder:

1. Outcome (from the issue) and `Closes #n`.
2. Enforcement mode, risk, tier, and any integrity flags.
3. A table: each acceptance criterion → result → evidence → verification method.
4. Gate results: integrity, verify, CI, review, with round counts.
5. Where to look first (from the reviewer's findings and the largest hunks), and files hidden or collapsed in the GitHub diff.
6. What was left out, and follow-up issues filed.
7. Cost, models and factory version; run id; audit-log digest.

- **VAL-14.** A pull request is marked ready only when every non-`manual` criterion passes and the three factory checks and the required CI checks are satisfied on the head SHA.

### 11.7 Later layers

- **Author tests (M3).** A test-author session writes acceptance tests from the criteria before implementation; the host confirms they fail, captures them as a commit, and records their paths as locked for the run (`locked_paths_sha256` in the marker, the list in the pull request body).
- **Mutation testing (M3).** `cargo mutants --in-diff` runs in CI on the pull request diff; surviving mutants are listed in the manifest. A full run is scheduled weekly and files issues.
- **Holdout scenarios (optional).** End-to-end scenarios kept in a separate private repository that no worker can read, run by the control plane against the built artifact. Only a behavioural description of a failure is returned. Evidence that this generalises is thin; it is not a v1 dependency.

---

## 12. Pull requests and merging

### 12.1 Pull request lifecycle

Draft at first publish → updated on every round → marked ready after all automated gates pass on the head SHA → review requested (4.2) → merged or reset.

### 12.2 Human review

- **PR-1.** The factory acts on submitted reviews, not on individual review comments. "Changes requested" by an authorised actor starts a human round: the review body and its line comments (from authorised actors only) become gate feedback.
- **PR-2.** An approval counts only if it is from an authorised actor, on the current head SHA, and, in the `team` profile, from someone other than the issue's approver. Approvals from Apps and bots never count in the factory's own check, whatever GitHub counts. Any push to the work branch invalidates earlier approvals.
- **PR-3.** If a ready pull request has had no review for 3 days *(tunable)*, the factory nudges the requested reviewers once on the dashboard issue and then stops.

### 12.3 Merge

Merge strategy per repository: `human` (the human merges; the factory observes and finishes the run when it sees the merge, 7.7), `factory_serial` (default from M2), or `native_queue` (M6, only if the capability probe finds a merge queue).

For `factory_serial`, the merge coordinator handles one pull request at a time per repository:

1. **Eligibility**, in every mode: an approval valid under PR-2; the three factory checks and the required CI checks satisfied on the head SHA; the head SHA is one the factory pushed; the base is the default branch; no `factory:hold`; no stop (13.7). In `factory-enforced` mode the three factory gates are read from the journal (after a rebuild: from check runs created by the factory App).
2. **Test-merge** the head into the current default-branch head in bare `host.git` (`git merge-tree --write-tree`, with no in-tree attributes applied). If it conflicts, or if policy `merge.update_when_behind = "always"` and the branch is behind, do a base update (12.4). Otherwise leave the branch untouched, so the approval stays on the SHA the human saw.
3. **Re-check** approvals, hold and stop immediately before the call, then merge:
   - `enforced` mode: `PUT /repos/{o}/{r}/pulls/{n}/merge-async` with `sha`, `merge_method: "squash"`, `merge_action: "direct_merge"`, and the host-generated `commit_title` and `commit_message` (5.6); never `bypass_rules`. Journal the returned id and poll the pull request's merged state. A `failed` result re-runs step 1; twice in a row escalates with `infrastructure`.
   - `factory-enforced` mode: the synchronous merge endpoint with `sha` and the same title and message, so the window between check and merge is one request.
4. **Finish.** If GitHub has not closed the issue within one tick (the repository's auto-close setting may be off), the factory closes it as completed. It writes the final status comment and deletes the branch.

With the default `merge.update_when_behind = "on_conflict"`, a textually clean merge can still break the default branch. This is accepted for small pull requests. A red default-branch CI run after a factory merge pauses merges in that repository, files a `state:triage` issue linking the merged pull request, and counts as a tripwire (15.3).

- **PR-4.** The factory never merges a pull request whose head it did not push.
- **PR-5.** Auto-merge without human approval exists only at autonomy level A3 (13.6).
- **PR-6.** `native_queue` (M6) requires the factory to report its checks on merge-group commits (`gh-readonly-queue/<default>/*`), copying verdicts only when every member's head SHA is one it pushed. Its permissions and polling are specified with that milestone (probe P-19).

### 12.4 Base update

A base update brings the default branch into the work branch without rewriting it.

1. The host creates an update commit in `host.git` with parents (previous published head, new default-branch head). If the test-merge was clean, its tree is the merge result and the commit is the new candidate.
2. If it conflicted, the host re-creates the workspace from the merge result including conflict markers, writes the conflicting paths to the handoff, and runs an implementer round (merge-conflict round +1). The resulting candidate has the same two parents.
3. `base_sha` in the marker becomes the new default-branch head; integrity and reviewer diffs use it.
4. The workspace is re-created from the candidate (WS-6), and the run re-enters at `integrity`. All later stages run on the new head, including `human_review`. A re-approval caused by a base update does not count against the human round cap.

Whether a base-merge push dismisses approvals under GitHub's stale-review rules when the pull request diff is unchanged is probe P-12; until answered, the factory assumes it does.

---

## 13. Security

### 13.1 Threat model

Attacker-controlled input: issue titles, bodies and comments; pull request text and review comments; repository content including dependencies, build scripts and proc macros; any web page an agent fetches; every file in a workspace, including under `.git`. The design assumes a worker can be fully subverted and asks what it can then reach.

| Threat | Control |
|---|---|
| Injection steers the agent to exfiltrate secrets | The worker holds no GitHub credential (INV-1) and, under the daemon, no model key (13.5); the environment is scrubbed; egress is default-deny (13.4). |
| Injection makes the agent write a malicious change | Integrity flags on sensitive paths, dependencies and hidden diffs; independent reviewer; human approval by default; CI without secrets. Residual risk: a plausible backdoor passing a tired reviewer. Small pull requests and the review-queue limit are the mitigation. |
| Workspace content attacks the host (hooks, nested repositories, symlinks, FIFOs, attributes, crafted names) | INV-2 and INV-3: the host does not open workspace paths; git on a workspace runs sandboxed; objects are verified at ingest; diffs ignore in-tree attributes (9.4, VAL-5a). |
| Agent weakens the checks that gate it | Verify and review run on a clean export (INV-11); protected and locked paths; App without Workflows permission; checks bound to the factory App; policy and instructions read from the default branch (INV-10); workflow audit (VAL-6a). |
| Agent text forges factory state on GitHub (markers, closing keywords, mentions, skip tokens) | GH-5, GH-5a; host-written commit bodies and squash messages. |
| Unauthorised person triggers or steers work | SEC-1 for every human input; SEC-1a for plan sub-issues; SEC-1b. |
| Text edited between approval and use | 5.3 rule 6; snapshot from the same read as the check; 7.7; PLAN-2. |
| Confused deputy: the factory approves or merges its own work | INV-5; PR-2; PR-4; App never a bypass actor. |
| Malicious dependency or build script | Sandbox for all builds; dependency changes flagged; `cargo deny` in verify. |
| Agent-written code runs in CI with open network | CI has no secrets and a read-only token (VAL-6a). On a private repository its content can still leave through CI; the template's no-network build step is the control, and repositories that keep their own CI job accept the risk. |
| Runaway cost | Budgets enforced and metered by the control plane through the gateway; circuit breakers; kill switches (13.7). In `direct_key` mode only the provider-side spend limit binds. |
| Data leaves through allowed channels (pull request text, branch content, the model endpoint) | The sandbox contains nothing beyond the one repository. On public repositories this residual risk is accepted and stated. |
| Compromised control plane | `enforced` mode: GitHub rules still require a human approval. In `factory-enforced` mode, and for A3 classes, this risk is not mitigated. |

The OWASP Top 10 for Agentic Applications is the review checklist for changes to this section.

### 13.2 Trust and triggering

- **SEC-1.** Human inputs that change factory behaviour are: `state:ready`, `factory:hold`, `factory:restart`, `factory:plan-approved`, `factory:stop`, `tier:*`, `risk:*`, `priority:*`, `type:ci` and `type:spec-delta`. Each is honoured only if the most recent `labeled` event for that exact label, with no later `unlabeled` event, has an actor of type User whose repository permission is write, maintain or admin when checked through the permission endpoint. `author_association` is not used. A tier, risk or type label that fails the check is read as its most restrictive value (`risk:high`, plan gate required, not `type:ci`). Issue types and fields are honoured only through their label fallbacks until probe P-17 shows their changes are attributable.
- **SEC-1a.** A `state:ready` applied by the factory App is honoured only on a sub-issue that: was created by the factory App; is listed with its body hash in the `children` of its parent's status marker; still has exactly that body; and whose parent's plan approval is valid (PLAN-2) with an approver who still has write permission at the sub-issue's claim. That approver is the sub-issue's approver. Any edit to the sub-issue voids the inherited approval and returns it to `state:triage` with one comment.
- **SEC-1b.** Factory-set labels are never read back as authority: the journal and check runs are the source.
- **SEC-2.** **Input trust level** of an issue: `trusted` if the issue author and every editor of its title and body are authorised actors; otherwise `untrusted`. An issue authored by the factory App takes the trust level of the input of the run that produced it. An `untrusted` issue is dispatched only if the repository's sandbox level is `hardened` (13.3), even when a maintainer applied `state:ready`.
- **SEC-3.** The snapshot contains the title, the body, and comments by authorised actors. Other comments are not shown to agents.
- **SEC-4.** Before use, snapshot text is sanitised: HTML comments and zero-width or bidirectional control characters are removed, and the size is capped (32 KiB body, 8 KiB per comment, 64 KiB total *(tunable)*).
- **SEC-5.** Pull requests from forks are never processed.

### 13.3 Sandbox

```rust
trait Sandbox {
    fn capabilities(&self) -> SandboxCaps;   // fs_isolation, process_isolation, egress_filtering, vm_boundary, guest_os, guest_arch
    async fn create(&self, run: RunId, policy: &SandboxPolicy) -> Result<Box<dyn SandboxInstance>>;
    async fn reap(&self, record: &SandboxRecord) -> Result<()>;      // after a daemon restart
}

trait SandboxInstance {
    fn record(&self) -> SandboxRecord;                               // journalled before the first exec
    fn command(&self, spec: &ExecSpec) -> Result<Command>;
    async fn kill_all(&self) -> Result<()>;
    async fn destroy(self: Box<Self>) -> Result<()>;
}
```

One instance exists per stage execution, created from that stage's policy (mounts, read-only or read-write, network) and destroyed when it ends.

| Level | Requires | Backends | Allowed for |
|---|---|---|---|
| `none` | — | Passthrough | The fake agent only |
| `process` | `fs_isolation`, `process_isolation`, `egress_filtering` | macOS: a deny-by-default Seatbelt profile through `sandbox-exec`. Linux (M4): bubblewrap with PID, IPC and user namespaces and seccomp. | `trusted` input (M1–M3) |
| `hardened` | plus `vm_boundary` | macOS: Apple `container` (one lightweight VM per stage execution; Linux guest). Linux: Firecracker-class microVM or gVisor. | `untrusted` input; any A3 auto-merge (M4) |

- **SEC-6.** Filesystem policy, deny by default:

  | Access | Paths |
  |---|---|
  | Read-write | The workspace (implementer) or the candidate export (verify); `exec/<seq>/home/` and temp; the stage's target directory; `quarantine/` (capture only) |
  | Read-only | The resolved toolchain; the agent CLI; the role's handoff view; the workspace (planner, capture); the candidate export (reviewer); the host-written settings file; a backend-defined OS baseline enumerated in code and covered by a snapshot test |
  | Denied | Everything else, including the host home directory, SSH agent, Docker socket, keychain, cloud credential files, the mirror and the rest of the data directory |

- **SEC-6a.** A worker cannot signal, trace or message a process outside its sandbox. On macOS the profile limits `signal` and `process-info` to the same sandbox and allows `mach-lookup` only for a fixed list that excludes the security daemon, launchd job submission and AppleEvents. Argument and environment visibility of same-user host processes is not prevented at level `process` on macOS, which is why INV-1 forbids secrets there.
- **SEC-7.** The sandbox fails closed. The sandbox level has no implicit default: a repository whose operator configuration omits it is not dispatched. If the configured level cannot be provided, the run does not start.
- **SEC-8.** The sandbox wraps the whole agent process, not only its shell commands. The agent CLI's built-in sandbox covers only its Bash tool and is not relied on.
- **SEC-9.** A VM backend with a Linux guest cannot serve repositories that must build on macOS, including possibly the factory's own. Policy key `requires_host_os` makes such a backend unavailable for that repository (decision D-5).

### 13.4 Network egress

- **SEC-10.** Workers have no direct network access. The only reachable endpoint is a per-run control-plane proxy: a loopback port (Seatbelt), a Unix socket bound into the namespace with an in-sandbox forwarder (bubblewrap), or the host interface (VM). The per-run endpoint is how a connection is attributed to a run (AG-19).
- **SEC-11.** The proxy allows a short hostname list: the model gateway; the crate registry (`index.crates.io`, `static.crates.io`) or a configured mirror; plus hosts added in operator configuration. For attended `sf run` in `direct_key` mode the model API endpoint replaces the gateway. `github.com` and `api.github.com` are never on a worker's list. Every denied request is logged with the run id.
- **SEC-12.** Hostname filtering does not stop abuse of an allowed multi-tenant host. The allowlist is kept short for that reason, and repository policy cannot extend it. Cargo git dependencies outside the list are a blocking onboarding failure.

### 13.5 Credentials

| Credential | Held by | Notes |
|---|---|---|
| GitHub App private key | Control plane, from the OS keychain or a 0600 file | Never passed to any child process |
| Installation token | Control plane | One repository, about 1 hour. Reaches `git push` only through the built-in credential helper over an inherited pipe. A test dumps the arguments and environment of every host child during a push and finds no token. |
| Operator's own GitHub credential | Not stored | Used once by `sf repo check` to read the bypass list (DEP-4) |
| Model API key, `gateway` mode | Control plane | Required for `sf daemon`. The worker gets the gateway address and a per-run token valid only there. The gateway replaces any authentication header the worker sends, injects the key, meters usage, and forwards only message-creation and token-counting paths. |
| Model API key, `direct_key` mode | Worker environment | Attended `sf run` at level `process` only; `sf daemon` refuses to start with it. The key is exposed to a subverted worker and budgets are advisory; use a key with a provider-side spend limit. |

### 13.6 Autonomy levels

| Level | Meaning |
|---|---|
| A0 | Lint and triage only; no code. |
| A1 | Plan only; the plan is posted for a human. |
| A2 | **Default.** The factory opens pull requests; a human approves; the factory or the human merges. |
| A3 | Auto-merge without human approval for classes defined in operator configuration, subject to the conditions below. M6. |
| A4 | Broad auto-merge. Out of scope. |

An A3 class is a deterministic predicate over the diff, evaluated by the integrity check: an allowlist of path globs, a maximum changed-line count and allowed file modes; for dependency bumps, only version and checksum fields of crates already in `Cargo.lock`, the same registry source, and a minimum release age. A class defined only by labels or tier is invalid.

A3 conditions, all required: `enforced` mode with a verified bypass list; sandbox level `hardened`; `risk:low` valid under SEC-1; the diff satisfies the class predicate; no integrity flag; all required checks pass; the class has at least 30 merged pull requests at A2 with a 30-day revert-or-rework rate under 5% *(both tunable; no published evidence supports specific numbers)*. Any tripwire in 15.3 drops the repository back to A2 automatically.

How an A3 merge satisfies a ruleset that requires an approval, without the factory approving (INV-5), is not settled. Two candidate layouts are recorded in 23.2 (P-18) and the choice is made at M6. Through M5, 12.3 has no exception to PR-2.

The operator configuration sets a ceiling per repository; repository policy can only lower it.

### 13.7 Kill switches

Each has a test.

1. `sf pause` / `sf drain`: stop dispatch locally; `drain` lets running stages reach a checkpoint.
2. A GitHub-visible stop: an open issue labelled `factory:stop` in the repository, valid under SEC-1, checked every tick. All stage executions in that repository are interrupted and nothing is published or merged. The stop stays in force until an authorised actor removes the label or closes the issue; closure by anyone else, or through a pull request, is ignored. If the stop check itself fails, no new stage starts and nothing is published or merged; running stage executions are interrupted after 3 consecutive failed checks *(tunable)*.
3. Suspend the App installation in GitHub settings.
4. Revoke the model API key.

### 13.8 Audit

Every run keeps an append-only record: actor and permission at trigger, snapshot hash, policy SHA, sandbox level and record, models, gate verdicts, cost, candidate SHAs (journal), and tool calls with allow and deny decisions (`events.jsonl`). The supervisor hash-chains `events.jsonl`; the engine journals the final digest of each activity; the run's combined digest is written into the pull request body.

---

## 14. Budgets and cost

Limits are enforced by the control plane from gateway-metered usage. Agent-side caps are a second line only: they are client-side estimates and can be exceeded.

| Scope | Default *(tunable; to be set by the operator)* |
|---|---|
| Session: turns / wall clock | 150 / 60 min |
| Session spend: plan / implement / review | $3 / $15 / $3 |
| Issue, cumulative across rounds and attempts | $40; each resume from a `budget` escalation grants a further $20 |
| Repository per day | $100 |
| Global per day | $150 |
| Factory eval runs | A separate budget; not counted against the daily limits |

Idle and tool-call time are the stall rules of 8.2, not budgets.

- **BUD-1.** Enforcement leaves 10% headroom below each limit.
- **BUD-2.** Reaching a session or issue limit stops the session and escalates with `budget`. Reaching a daily limit pauses dispatch until the next UTC day; running sessions finish their stage.
- **BUD-3.** Model id, effort and a price table are per-stage configuration, loaded at runtime. Nothing about models or prices is compiled in.
- **BUD-4.** Spend is recorded per model call (input, output, cache-read and cache-write tokens, and dollars) and rolled up per stage, run, issue and merged pull request. Cumulative spend appears in the status comment and the pull request body.
- **BUD-5.** The primary cost metric is dollars per merged pull request, counting failed and abandoned runs. Token volume and pull request count are not treated as progress.

---

## 15. Observability

### 15.1 Logs and traces

- `tracing` spans carry repository, issue, run id, attempt and stage. Logs are JSON lines on disk; a human-readable format on the terminal.
- OpenTelemetry export is optional and behind a feature flag: one span per stage and per model or tool call. GenAI attribute names are isolated in one module pinned to a semantic-conventions version, because those conventions are not yet stable.
- `TRACEPARENT` is set in each worker's environment.
- Run and issue ids go on spans and logs, never on metric labels.

### 15.2 Metrics

| Group | Metrics |
|---|---|
| Flow | Ready → pull request time; pull request → merge time; human review latency; pull request size |
| Yield | Merge rate; first-pass verify and CI rates; rounds per gate; escalation rate by category; abandonment |
| Quality | Rework and revert rate within 30 days; escaped defects (bug issues linked to a factory pull request); reviewer agreement with humans |
| Cost | Dollars per run and per merged pull request; cache-read share |
| Safety | Integrity flags and fails; capture rejections; egress denials; SEC-1 rejections; budget stops; secret-scan hits |

`sf metrics` prints them from the journal; no external metrics system is required.

### 15.3 Tripwires

Evaluated daily per repository. Any of these pauses A3 (if enabled) and opens a dashboard notice: 30-day revert-or-rework rate above 10%; an escaped defect in an auto-merged change; a red default-branch CI run after a factory merge (which also pauses merges, 12.3); any secret-scan hit in a published commit; three budget stops in a day *(all tunable)*.

### 15.4 Feedback into the harness

Every escalation, reset, human "changes requested" and escaped defect is classified (`spec-gap`, `missing-context`, `missing-guardrail`, `agent-error`, `flake`, `infrastructure`). `sf metrics --failures` reports classes over time. For classes other than `agent-error`, the factory files a `state:triage` issue proposing the durable fix: a spec change, an `AGENTS.md` or docs change, a new lint or structural test, or a flake quarantine.

### 15.5 Dashboard issue

Each repository has one pinned issue, maintained by the factory and edited in place: active runs, queue, items waiting on a human with their age, yesterday's spend, breaker, mode and sandbox-level status. It is the only place the factory nudges.

---

## 16. Configuration

### 16.1 Layers

| Layer | Location | Trust | Holds |
|---|---|---|---|
| Operator | `~/.config/sf/config.toml` | Trusted | Credentials and their mode, data directory and disk limits, repository allowlist with profile, reviewers, sandbox level, autonomy ceiling, required CI checks and acknowledged workflows per repository, egress allowlist, global budgets and concurrency, agent CLI path and version range |
| Repository policy | `.factory/policy.toml` and `.factory/prompts/*.md` at the default-branch head | Reviewed, but changeable by pull request | Stages enabled, verify and setup commands, per-stage model and limits, round caps, size budget, extra protected paths and required checks, instruction directories, generated-file globs, merge settings |
| Issue | Labels | Honoured only under SEC-1 | Tier, risk, priority, type, hold, restart |

- **CFG-1.** Repository policy can only tighten operator settings: lower budgets and concurrency, lower autonomy, add protected paths and required checks, set `merge.update_when_behind = "always"`, and disable `plan` for tier 0–1, `author_tests`, and `review` for tier 0. It cannot lower the sandbox level, extend the egress allowlist, shorten built-in protected paths or the required-check list, disable `capture`, `integrity`, `verify` or human review, or name credentials.
- **CFG-2.** Files are TOML, parsed with unknown fields rejected, and each has a `schema_version`. A JSON Schema is generated for editor support.
- **CFG-3.** A change to `.factory/**` is itself a protected-path change and is never auto-merged.

### 16.2 Example policy

```toml
schema_version = 1

[stages]
plan = "auto"            # auto = by tier and risk
author_tests = false
review = true

[models]
plan      = { id = "claude-opus-5-5",   effort = "high" }
implement = { id = "claude-opus-5-5",   effort = "medium" }
review    = { id = "claude-sonnet-5-5", effort = "high" }

[limits]
plan_rounds = 2
verify_rounds = 3
ci_rounds = 2
review_rounds = 2
max_changed_lines = 600
tool_call_timeout_secs = 1800

[setup]
commands = ["cargo fetch --locked"]
timeout_secs = 600

[verify]
commands = ["cargo fmt --all --check",
            "cargo clippy --workspace --all-targets --locked -- -D warnings",
            "cargo nextest run --workspace --locked"]
timeout_secs = 1800

[ci]
timeout_secs = 3600
extra_required_checks = []      # the base list is operator configuration (VAL-8)

[merge]
strategy = "factory_serial"
update_when_behind = "on_conflict"

[instructions]
directories = ["docs/**"]

[paths]
protected_extra = ["migrations/**"]
generated = ["**/*.snap"]
lockfiles = ["Cargo.lock"]
```

Model ids are examples; they are configuration and will change.

### 16.3 Reload

Policy is re-read when the default-branch head changes. An invalid policy is rejected: the last valid version stays in force for running stages, new dispatch in that repository pauses, and the dashboard shows the error. A run uses the policy commit recorded at claim for its whole life, except that a reset or a base update picks up the current one.

---

## 17. Command line and control API

One binary, `sf`. Every command accepts `--json`.

| Group | Commands |
|---|---|
| Setup | `sf init` · `sf app create` · `sf repo add|check|remove <owner/repo>` · `sf doctor [--probe]` |
| Operate | `sf daemon` · `sf run <owner/repo#n>` (same engine, one item, foreground, resumable) · `sf status` · `sf show <run|issue>` · `sf logs <run> [--follow]` · `sf pause` · `sf resume` · `sf drain` · `sf cancel <run>` · `sf retry <run>` |
| Inspect | `sf cost [--since]` · `sf metrics [--failures]` · `sf policy check|explain` · `sf issue lint <n>` |
| Maintain | `sf gc` · `sf rebuild` · `sf export <run>` · `sf eval run|report` · `sf self-update [--rollback]` |

- **CLI-1.** `sf doctor` checks credentials, App permissions, CLI versions and flags, sandbox availability at the configured level, git version, the toolchain for each repository, enforcement mode and the capability probe. `--probe` additionally runs the live probe suite (23.2) against a scratch repository and records the answers.
- **CLI-2.** The daemon serves HTTP/JSON on a Unix socket with mode 0600 in the data directory: `GET /v1/state`, `GET /v1/runs/{id}`, `GET /v1/runs/{id}/events` (stream), `POST /v1/refresh`, `POST /v1/pause|resume|drain`, `POST /v1/runs/{id}/cancel|retry`. There is no TCP listener by default. The API is a convenience: the daemon is correct without any client.
- **CLI-3.** `sf run` and `sf daemon` share all engine code and the store. `sf run` takes the store lock, so it cannot run beside a daemon on the same store; run again on the same issue, it resumes from the journal.
- **CLI-4.** `sf cancel` is equivalent to a human applying `state:triage`. `sf retry` restarts the recorded stage with its stall and poison counters cleared.

---

## 18. Local store and retention

### 18.1 Store

SQLite in WAL mode, accessed from one writer thread.

| Table | Content |
|---|---|
| `runs` | One row per run: repository, issue, stage, attempt, rounds, spend, deadlines, ids of branch, pull request and status comment. Authoritative run state, with a `state_version`. |
| `events` | Append-only audit journal of engine events |
| `outbox` | Pending external writes with effect ids and retry state |
| `activities` | Stage executions: kind, sandbox record, pid, pgid, start time, model, usage, outcome, chain digest |
| `spend` | Per model call usage records |
| `etags` | Conditional-request cache for the gateway |
| `probes` | Capability and probe results per repository |

- Every table that refers to a work item includes the repository id, so one daemon serves several repositories.
- Migrations are forward-only, hand-rolled on `PRAGMA user_version`. `sf self-update` drains, copies `store.db` with the SQLite backup API to `store.db.pre-<version>`, then migrates. Rollback reinstalls the previous binary, restores that file and reconciles (GitHub wins).
- Each release converts the previous `state_version` on load; an unknown version moves the run to `needs-human` with `infrastructure`.

### 18.2 Retention

| Data | Default *(tunable)* |
|---|---|
| `target/`, exports | Removed when the run enters `human_review` or a terminal state |
| Workspace and run directory | Removed 24 h after a terminal state |
| Agent event streams and transcripts | 30 days; 180 days for failed, reset or reverted runs |
| Journal rows | Kept; event detail pruned after 180 days |
| Remote branch | Deleted on merge; 14 days after a close without merge |

Transcripts are never uploaded to GitHub. `sf gc` applies retention; `sf export <run>` writes a run's directory and journal rows to an archive.

---

## 19. Rust implementation

### 19.1 Workspace

One virtual Cargo workspace: edition 2024, resolver 3, `rust-version = "1.100"`, `publish = false`. The toolchain is pinned in both `mise.toml` and `rust-toolchain.toml`.

| Crate | Responsibility | May depend on (normal dependencies) |
|---|---|---|
| `factory-core` | Domain types, effects and events, the pure engine, policy and config types, issue parser, readiness lint, integrity rules, prompt assembly and sanitisation as pure functions, `Clock` and `IdGen` | (no I/O crates) |
| `factory-store` | `Store` trait, SQLite journal, outbox, migrations | core |
| `factory-proc` | Process groups, supervisor shim, stop sequence, line codec, timeouts | — |
| `factory-sandbox` | `Sandbox` traits, `SandboxPolicy`, backends | proc, core |
| `factory-git` | `Git` trait, `git` CLI wrapper, mirrors, workspaces, capture, export, publishing | proc, sandbox, core |
| `factory-github` | `GitHub` trait, octocrab adapter, gateway, GraphQL documents, outbound encoder | core |
| `factory-agent` | `AgentDriver` trait, Claude driver, event normalisation | proc, sandbox, core |
| `factory-net` | Egress proxy, model gateway | core |
| `factory-daemon` | Supervisor, scheduler, reconciler, effect executors (including setup and verify), control API | all of the above |
| `factory-cli` | The `sf` binary | daemon |
| `factory-testkit` | Fake agent binary, `fake-github`, fixtures | core, proc, store, git, agent, github |
| `xtask` | Developer automation | — |

Each port trait is defined in the crate that owns the seam. Core defines domain data and effects only; `factory-daemon` maps effects to port calls. A structural test over `cargo metadata` enforces these directions for normal dependencies; dev-dependencies may additionally include `factory-testkit`.

### 19.2 Stack

Versions are those current on 2026-11-18. Pre-1.0 and fast-moving crates are pinned exactly, and the pinned set MUST resolve (`cargo generate-lockfile`) as an M0 task.

| Concern | Choice | Notes |
|---|---|---|
| Async runtime | `tokio` 1.x, `tokio-util` (`CancellationToken`, `TaskTracker`) | A cancellation token tree: root → run → activity |
| Processes | `process-wrap` 10 | Process groups and sessions |
| Git | `git` CLI ≥ 2.42 | `gix` read-only at most; no `git2` |
| GitHub | `octocrab` 0.54.x, exact pin, built-in retry disabled | Behind the port trait; hand-written GraphQL with serde types; a test asserts a failed POST is not re-sent by the client |
| HTTP | `reqwest`; `axum` 0.8 for the Unix-socket API and the gateway | |
| Store | `rusqlite` 0.43 (`bundled`) | One writer thread; migrations hand-rolled (`rusqlite_migration` currently requires an older `rusqlite`) |
| Config | `serde`, `toml`, `schemars` | Hand-rolled layering |
| CLI | `clap` 4 (derive) | |
| Errors | `thiserror` in libraries; `anyhow` in the binary only | Library errors carry a failure class: transient, permanent, needs-human, rate-limited with reset time |
| Observability | `tracing`, `tracing-subscriber`; `opentelemetry` 0.33 behind a feature | |
| Testing | `cargo-nextest`, `insta`, `wiremock`, `proptest`; failpoints behind a cargo feature | |
| Supply chain | `cargo-deny`, an unused-dependency check, committed lockfile, `--locked` everywhere | |
| Release | `cargo-dist` 0.34 with attestations; `cargo-auditable` | |

Not used: any community Claude SDK crate, `git2` for mutations, `figment`, `sled`, `serde_yaml`, `rig-core` in the core, an MCP server (the agent needs no factory tools in v1; its result and follow-ups travel in the structured result).

### 19.3 Conventions

- Workspace lints deny `unwrap_used`, `expect_used`, `await_holding_lock`, and, through `disallowed_methods`, bare `tokio::spawn` outside the supervisor and `std::process::Command::new` outside `factory-proc`. `xtask` is exempt.
- A panic in a task fails that run and never the daemon, except in the store writer (ORCH-5).
- Shutdown on SIGINT or SIGTERM: stop dispatch, interrupt activities through the stop sequence, flush the journal, release the lock, within 30 s.

### 19.4 Testing

| Layer | What |
|---|---|
| Engine | Property tests over event sequences for the invariants of 3.1: one active run per issue, round caps respected, no merge effect without a valid approval, a duplicated or re-delivered result event leaves state unchanged |
| Recovery (simulation) | The real engine and the real SQLite store with scripted executors driven by a fault plan: crash before commit, after commit before the effect, after the effect before the acknowledgement, duplicate completion, reordered completion. Property: no duplicate external write, and convergence to the fault-free state. |
| Process crash | A daemon built with failpoints is killed with SIGKILL at each stage boundary and restarted, against `fake-github` |
| Drivers | A `fake-agent` binary that replays scripted stream-JSON and misbehaves on demand: hang, garbage, oversized line, loop, ignore SIGINT, a `setsid` grandchild holding stdout, the daemon killed mid-session. Recorded fixtures from the real CLI. |
| Git | Real `git` in temp directories with the hostile-tree suite of WS-9; a round after a base update never reverts upstream content |
| GitHub | `fake-github`: a stateful HTTP fake implementing the REST and GraphQL subset the gateway uses, hosted by the test process so it survives a killed daemon; pushes go to a local bare repository. `wiremock` for adapter edge cases including rate-limit and ETag paths. |
| Sandbox | Per backend: read the host home, a keychain item, or the arguments and environment of a host process; reach a non-allowlisted host; write outside the allowed paths; signal the daemon; submit a launchd job; send an AppleEvent. An M0 probe records which of these the Seatbelt prototype blocks. |
| End to end | With the fake agent against `fake-github` on every pull request. Against a scratch GitHub repository only in a scheduled workflow on the default branch, behind a protected environment, with a separate test App installed on the scratch repository only. |
| Contract | A scheduled test against the installed agent CLI that asserts the flags and message shapes the driver relies on, and that `.claude/settings.json`, `.claude/settings.local.json` and a settings file written mid-session into the config directory have no effect |

### 19.5 CI for the factory repository

`cargo fmt --check`; `cargo clippy -D warnings`; `cargo nextest run --locked`; `cargo test --doc`; `cargo deny check`; unused-dependency check; the structural test. The workflow follows VAL-6 itself.

---

## 20. Onboarding a repository

### 20.1 `sf repo add`

1. Check that the App is installed; read the capability probe and enforcement mode.
2. Produce a readiness report. **Blocking:** a single-command setup and test entry point that passes on the default branch under the sandbox; an `AGENTS.md`; the toolchain of WS-7 installed; no submodules, no `filter=` attributes and a clean round-trip checkout at the default-branch head; no Cargo git dependency outside the egress list; the workflow audit (VAL-6a). **Advisory:** check speed, flake rate, documentation layout, architecture rules, the repository's auto-close setting, other installed Apps that can write pull requests, and past approving reviews by bot accounts.
3. Open one onboarding pull request containing `.factory/policy.toml`, default prompts, the issue forms, a `CODEOWNERS` entry for protected paths and an `AGENTS.md` skeleton if missing. The CI workflow template is not pushed by the App (GH-11): `sf repo add` writes it to a local file and prints the commit command, and the operator adds it to the onboarding branch with their own credentials.
4. Do nothing else in that repository until the pull request is merged and a workflow satisfying VAL-6a exists on the default branch. Closing the pull request opts the repository out.
5. After the merge: create the label set through the API, write the workflow's check names into `required_checks`, and only then activate the ruleset of 20.3.

Until M5 the same steps are done by hand from this checklist.

### 20.2 Modes after onboarding

A repository advances by operator command: `observe` (A0: readiness lint and plans only) → `propose` (A2) → named A3 classes.

### 20.3 Recommended ruleset

For the default branch, in `enforced` mode: require a pull request; one approval; dismiss stale approvals on new commits; require approval of the most recent push by someone other than the pusher; require code-owner review; require the status checks `factory/integrity`, `factory/verify`, `factory/review` (source: the factory App) and the CI checks of VAL-8; do not require branches to be up to date; block force pushes and deletions. Signed commits are not required in v1 (5.6).

Required checks apply to every pull request, so human-authored pull requests need a rule:

- `solo`: the repository-admin role is a bypass actor in pull-request-only mode. This is required, because an author cannot approve their own pull request and the factory checks exist only on factory pull requests. Human pull requests are merged through that bypass and are outside the factory's guarantee. The factory App is never a bypass actor.
- `team`: no bypass is needed. The factory reports its three checks as `neutral` ("not a factory pull request") on every open same-repository pull request that is not its own; such pull requests wait for the daemon. An admin bypass in pull-request-only mode MAY be configured as a fallback for daemon outages.

The mode detection of 4.1 accepts exactly these bypass configurations.

### 20.4 Repository conventions the factory expects

- A root `AGENTS.md` of about 100 lines that maps into `docs/`; details live in versioned documents, not in the instruction file.
- Fast, deterministic checks runnable with one command.
- Architecture rules enforced mechanically (crate boundaries, lints with remediation text in their messages).

---

## 21. Bootstrapping and evaluating the factory

### 21.1 Self-hosting

- **BOOT-1.** Humans (with interactive agent assistance) build M0 and M1. From M1 the factory is run attended on its own repository from an installed, tagged build; from M2 unattended.
- **BOOT-2.** The binary that operates on the factory repository is always an installed, pinned release, never a build of the tree it is changing. The factory version is recorded in every marker.
- **BOOT-3.** Upgrading the operating binary is a human action (`sf self-update`), allowed only after the eval gate passes on the candidate: a smoke eval until M3, the eval suite afterwards. Previous binaries and store backups are kept for rollback (18.1).
- **BOOT-4.** The factory's own gate code, prompts, policy and this specification are protected paths in its repository and are never auto-merged.
- **BOOT-5.** From M2 the factory also runs on a second, unrelated repository, so defects that self-hosting would hide become visible.

### 21.2 Evaluation

| Layer | Content | Gate |
|---|---|---|
| Harness tests | Section 19.4 with the fake agent | Every pull request |
| Smoke eval (M2) | The five issues merged in M1, one trial each, deterministic graders | Binary promotion until M3 |
| Factory eval suite (M3) | 20–50 tasks built from the factory's own closed issues and merged pull requests, each with the repository state, the issue, and deterministic graders. Run from a clean environment. Reported as pass@1 and pass-all-trials, with cost and duration. Split into a regression set (must not get worse) and a capability set. | Required before promoting a new binary, prompt set, policy default or model; trial counts per promotion type are decision D-8 |
| Production metrics | 15.2 with tripwires | Continuous |

Public benchmark scores are not acceptance criteria.

---

## 22. Roadmap

| Milestone | Scope | Exit criterion |
|---|---|---|
| **M0 Foundations** | Workspace, CI and a tagged release build; `factory-core` types and engine skeleton with property tests; `factory-proc` with the supervisor shim; git wrapper; GitHub App creation; `sf doctor`; the probe suite (23.2); prototypes of capture, ingest and export against the hostile-tree suite (WS-9) and of the Seatbelt profile | All M0 probes answered and recorded; the hostile-tree suite passes; a cold `cargo build` of the factory repository and one real agent turn succeed under the Seatbelt profile with only the proxy reachable |
| **M1a Skeleton** | Store with `runs` and `events`; claim; implement at sandbox level `process` with the egress proxy (fake agent, then the Claude driver); per-session budgets; capture; integrity with path, structural and secret rules; verify on the export; publish a draft pull request; then exit. `sf run` on the same issue resumes from the journal. | One real issue reaches a draft pull request and survives an interrupted run |
| **M1b Attended run** | Status comment and marker; factory check runs; CI wait; ready and evidence manifest; observation of human review and merge (strategy `human`) | Five real issues on the factory repository merged by a human from factory pull requests, using a tagged, installed build |
| **M2 Daemon** | Poll and reconcile loop; outbox, activities and recovery; scheduler and limits for two repositories onboarded by hand; SEC-1 for all human inputs; outbound encoder; workflow audit; model gateway (the daemon refuses `direct_key`); escalation and resume; plan stage and gate; reviewer stage; in-place rounds; base update; reset; `factory_serial` merge; follow-up issue filing; dashboard issue; control API; kill switches; `sf rebuild`; `sf self-update` with rollback; smoke eval | 48 hours unattended on a second repository with no manual repair; the process-crash test passes at every stage boundary; no model key present in any worker; self-hosting on a pinned release begins |
| **M3 Validation depth** | Issue forms and readiness lint enforced; tiers, spec deltas and tier 2 decomposition; `author_tests` with locked paths; tampering heuristics; mutation testing in CI; failure classification; eval suite v1 | The eval suite gates a release; reviewer agreement with humans is measured |
| **M4 Hardening** | VM sandbox backend; Linux host with the `process` backend; hash-chained audit digest in pull requests; `untrusted` input permitted | Sandbox escape tests pass at `hardened` on both hosts |
| **M5 Operations** | `sf repo add` onboarding automation; more than two repositories; metrics and tripwires; retention and `sf gc`; `sf export`; OpenTelemetry export | A third repository onboarded through the onboarding pull request plus the operator-committed workflow file |
| **M6 Merge automation** | A3 classes with predicates, promotion and demotion, and the chosen ruleset layout; `native_queue` strategy; optional `createCommitOnBranch` publishing; second agent driver | One A3 class enabled on one repository with tripwires exercised in a test |

Until M4 the factory runs only on repositories where every issue author and editor is an authorised actor (SEC-2).

---

## 23. Open items

### 23.1 Decisions for the operator

| # | Decision | Assumed until decided |
|---|---|---|
| D-1 | Is the factory repository public, or private on a paid plan? On the current Free personal account a private repository gets `factory-enforced` mode only. | Supports both; recommends `enforced` |
| D-2 | Model credential (API key or cloud provider) and the budget values of section 14. | Console API key behind the gateway; the listed defaults |
| D-3 | Host platforms. | macOS only through M3; Linux in M4 |
| D-4 | Which second repository does the factory run on from M2? | To be named |
| D-5 | VM backend on macOS for M4. Apple `container` is young, its hardening record is unverified, and its Linux guest cannot build macOS-only repositories. | Apple `container`, re-evaluated at M4 |
| D-6 | Should the reviewer use a different model family from the implementer? | Same provider, different model; configurable |
| D-7 | The daemon runs at sandbox level `process` during M2–M3. The research recommended a VM boundary for all unattended runs. | Accepted, because input is restricted to authorised actors until M4 |
| D-8 | Eval trials and budget required per promotion type (binary, prompts, policy defaults, model). | Regression set at one trial for a binary; full suite at three trials otherwise |
| D-9 | `merge.update_when_behind`: `on_conflict` keeps the human approval on the reviewed commit but can let a semantic conflict reach the default branch; `always` re-validates every time at the cost of re-approval. | `on_conflict` |

### 23.2 Probes

Facts the design depends on that research could not confirm from primary sources. `sf doctor --probe` answers each against a scratch repository, and the answer is recorded in the store and in `docs/research/probes.md`. All are M0 unless marked.

| # | Question | Design impact |
|---|---|---|
| P-1 | What does creating an existing ref return (409 or 422, and the message)? | Claim fence error handling (8.2) |
| P-2 | Does an approving review from a GitHub App, or from `github-actions[bot]`, count toward required approvals under a ruleset? | Onboarding warning; PR-2 already ignores them |
| P-3 | Can an App bot be an issue assignee? | Optional "assigned to the factory" signal |
| P-4 | How does an issue form render into the body? | Parser of 6.2 |
| P-5 | Is a push that changes `.github/workflows/` rejected for an App without the Workflows permission on every push path? | Defence in depth for 11.3 |
| P-6 | Does the agent CLI's inner Bash sandbox work inside an outer Seatbelt sandbox? | AG-8 |
| P-7 | Does sandboxed staging with a quarantine object directory and host-side ingest behave as specified on the hostile-tree suite, and what is the exact ignore-rule mechanism? | 9.4, WS-9 |
| P-8 | Who is reported as the editor of an issue title and body through the API? Is `lastEditedAt` set for API edits, and does a title change always produce a rename event? | 5.3 rule 6, 7.7, SEC-2 |
| P-9 | Do blocked-by dependencies work across repositories, and what is the per-issue limit? | 5.3; cross-repository work is out of scope for v1 regardless |
| P-10 | Do comment edits count toward the content-creation rate limit? | GH-16 budget |
| P-11 | Does `merge-async` behave as documented on a personal-account repository, including `merge_action` and the custom commit message? | 12.3 |
| P-12 | Does a base-merge commit pushed by the App dismiss approvals under "dismiss stale approvals" or "require approval of the most recent push" when the pull request diff is unchanged? | 12.4; decision D-9 |
| P-13 | Cold versus seeded build time and size for the factory repository. | 9.3 |
| P-14 | Is `current_user_can_bypass` populated for installation tokens on the branch-rules endpoint? | DEP-4 |
| P-15 | Which encodings suppress mentions, issue references and closing keywords in comments, pull request bodies and commit messages? | GH-5a |
| P-16 | Which of the sandbox escape attempts of 19.4 does the Seatbelt prototype block? | SEC-6a |
| P-17 | Do issue type and issue field changes expose an actor and a timestamp? | SEC-1; use of types and fields |
| P-18 | (M6) For A3: does a ruleset accept zero required approvals with required code-owner review, and does an unowned CODEOWNERS pattern lift the requirement? The alternative is a fourth factory-reported check `factory/approval`. | 13.6 |
| P-19 | (M6) Can an installation token list merge-queue refs and entries, and which permission does that need? | PR-6 |

### 23.3 Known weak points

- Human review is the main defence against a plausible but wrong change, and review quality falls as volume rises. The review-queue limit and the size budget are blunt tools.
- In the `solo` profile there is no separation between the person who approves the issue and the person who approves the code, and human pull requests bypass the ruleset.
- Reviewer agents are lenient by default and unevaluated until M3.
- The agent CLI's protocol and flags change often. The pinned version range and the contract test contain this but do not remove it.
- On macOS at sandbox level `process`, a worker process that leaves its session and working directory can survive the sweep until the daemon restarts; AG-19 cuts its network, not its CPU.
- Every verify is a cold build. On a large repository this may dominate cycle time until a safe seeded cache exists.
- Most published throughput and quality figures for such systems are self-reported. No target in this specification is derived from them.

---

## Appendix A. Mapping to prior art

| This spec | OpenAI Symphony | StrongDM Attractor |
|---|---|---|
| Poll / dispatch / reconcile tick (8.3) | Same loop | — |
| `state:*` labels (5.2) | Tracker states Todo / In Progress / Human Review / Merging / Rework / Done | — |
| Status comment (5.4) | Workpad comment | — |
| Typed stages with result contract (7.3, 10.5) | — | Graph nodes with outcomes |
| Plan gate, human review (7.4, 12.2) | Human Review state | Human gate nodes |
| Holdout scenarios (11.7) | — | Scenarios as holdout tests |
| `.factory/policy.toml` (16) | `WORKFLOW.md` | DOT pipeline file |

Symphony's Elixir reference now includes a minimal GitHub Issues adapter (open/closed only, no dependencies); it is prior art for the adapter boundary, not for a label-driven workflow. Fabro (MIT, Rust, Attractor lineage) is worth reading as implementation prior art before M1.

## Appendix B. Research and review

The reports in `docs/research/` hold the evidence and source URLs behind this specification:

- `01-state-of-the-art`, `02-orchestration-architecture`, `03-specs-and-validation`, `04-github-integration`, `05-rust-implementation`, `06-security-governance-ops`: the six research angles.
- `07-gaps`: contradictions between the reports and how they were resolved. This specification departs from two of its resolutions, recorded as D-7 and in 13.6.
- `08-fact-check`: 66 load-bearing claims checked against primary sources (56 confirmed, 9 partly, 1 wrong; corrections are applied here).
- `09-spec-review`: the 59 findings of the adversarial review of draft 1, each with the reviewer's proposed fix and the sceptic's verdict. This draft addresses each one, not always with the proposed fix.
