# Software Factory — Specification

| | |
|---|---|
| Status | Draft 4. Drafts 1 and 2 were reviewed (`docs/research/09-spec-review.md`, `10-spec-recheck.md`) and draft 3 addressed the findings. Draft 4 applies the operator's requirement decisions (23.1) and has not been reviewed. |
| Date | 2026-11-21 |
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
- **G6.** Reach lights-out operation by evidence. Every class of work starts with the operator reviewing each pull request, and is promoted to merging without review only on its measured record (13.6). The operator always approves each issue.

### 1.2 Non-goals

- A second issue tracker, or any store that holds work items GitHub does not.
- A general workflow engine. The pipeline is fixed in code; only stage settings are data.
- A multi-user or hosted service. This is a personal factory: one person runs one daemon on their own machine, signed in to the agent CLI with their own account. The factory never handles anyone else's model credentials.
- Several agents writing to one branch. Parallelism comes from independent issues.
- Merging without review before it is earned (G6, 13.6).
- Opening issues or pull requests on repositories the operator has not added to the factory.
- Working on issues written by anyone but the operator (SEC-2).
- Releasing or deploying. The factory's job ends at merge; it never holds a publishing or deployment credential.
- Running on macOS or Windows. The host is a Linux server (4.4).

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

The terms this specification uses are defined once, in `GLOSSARY.md` at the root of the factory repository, and are used here as defined there (D-21). The file cites the ids and section numbers of this specification.

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
│  no GitHub credential · scrubbed environment · default-deny network  │
└──────────────────────────────────────────────────────────────────────┘
```

### 3.1 Invariants

These hold in every mode and milestone. A change that breaks one is a defect.

- **INV-1.** No GitHub credential ever enters a worker, and none appears in the arguments, environment or files of any child process.
- **INV-2.** The control plane never executes anything from a workspace and never reads configuration from one: no hooks, no workspace `.git/config`, no agent settings, no `.cargo/config.toml` or `rust-toolchain.toml`. It does not open paths inside a workspace. Its only access is an `lstat`-only walk and git commands run inside a sandbox (9.4).
- **INV-3.** Every command that builds or runs repository code, and every git command whose work tree is a workspace, runs inside a sandbox.
- **INV-4.** A run starts only on an action by an authorised actor whose permission was checked through the GitHub API at that moment. There is no exception: every issue the factory works on was approved individually.
- **INV-5.** The factory identity never approves a pull request and never bypasses a repository rule.
- **INV-6.** At most one non-terminal run exists per (repository, issue), and at most one stage execution per run at a time.
- **INV-7.** All external side effects are idempotent and keyed (8.5). Repeating any step after a crash produces no duplicate branch, pull request, comment, check run or issue.
- **INV-8.** The local store is a rebuildable cache. If it disagrees with GitHub, GitHub wins.
- **INV-9.** Text from issues, pull requests, comments, logs and agents is data. It is never interpolated into a shell command or a control-plane decision, and it is encoded before being written back to GitHub (GH-5a).
- **INV-10.** The policy and agent instructions that govern a run are read from a default-branch commit, never from the workspace or the work branch.
- **INV-11.** `verify` and `review` never see the workspace. They run on a clean export of the candidate commit, in a fresh sandbox instance with a fresh home directory. Nothing one stage execution writes is visible to a later one except the workspace and the implementer target directory (both visible to implementer rounds only), the candidate commit and the structured result.

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
- **DEP-3.** `enforced` mode requires a public repository, or a private repository on GitHub Pro, Team or Enterprise. The spec recommends `enforced` for any repository the factory runs on unattended. The operator's repositories are public (decision D-1), so they run in `enforced` mode, which auto-merge requires (13.6).
- **DEP-4.** The App has no Administration permission, so it cannot read a ruleset's bypass list. `sf repo check` reads it once with the operator's own credential (not stored) and records the result; it repeats when the ruleset's `updated_at` changes. Until then the mode is shown as `enforced (bypass list unverified)`. The accepted bypass list is fixed by 20.3; the factory App is never on it. If the list read is not the one 20.3 fixes, or contains the factory App, the mode is `factory-enforced` until a later `sf repo check` passes. At every mode detection the daemon reads `updated_at` of each contributing ruleset (ids come from the branch-rules response); when it changes, the mode reverts to `enforced (bypass list unverified)` until the operator runs `sf repo check` again. If probe P-14 shows `current_user_can_bypass` is populated for installation tokens, any value other than `never` on a contributing ruleset makes the mode `factory-enforced`.

### 4.2 Approval

The factory has one human. The approver of an issue also reviews and approves its pull request, and review is requested from that approver. There is no separation between approving the work and approving the code (23.3).

### 4.3 Capability probe

At startup and daily, the factory probes each repository and records what is available: rulesets, native merge queue, issue types, issue fields, sub-issues, issue dependencies. Features are used only when the probe finds them; each has a fallback (section 5).

### 4.4 Host requirements

- A Linux server on the operator's own hardware (x86-64 or arm64) with KVM available to the factory's user. macOS and Windows are not supported hosts. The workspace still compiles on macOS for development, with the sandbox backends compiled out; there only sandbox level `none` with the fake agent works. Linux-only code is developed in a local Linux VM (DEV-3).
- Unprivileged user namespaces enabled, and `bubblewrap` installed.
- `git` ≥ 2.42. The data directory on its own volume if possible.
- An agent CLI inside a tested version range (v1: Claude Code; 10.3).
- The daemon runs as a dedicated system user under systemd, by default `sf`. Its home directory is `/home/sf` by default and can be any other path (CFG-4). The operator controls it through GitHub and through `sf` over SSH; there is no remote client.
- A run does not start when the sandbox backend for its required level is unavailable (13.3).

**The operator's server.** Observed on 2026-11-20 by a script that the operator ran over SSH as their own account, without root (`docs/research/14-server-facts.md`). The server is not named here, and no address is given. The operator then installed `bubblewrap` and Claude Code and ran the script again the same day; what that run added is marked "second run" below. A third run that day was made as the factory's system user, which the operator had created as a member of group `kvm`. Both are recorded under P-24 in `docs/research/probes.md`. An observation made later is added with its own date.

| | Observed | Requirement above |
|---|---|---|
| Distribution | Debian 13 (trixie) on bare metal, the distribution of the development VM (DEV-3). systemd 257, cgroup v2 | Met |
| Kernel | `6.12.116+deb13-amd64` | None stated |
| CPU | x86-64, one Intel Core Ultra 5 125H: 14 cores, 18 logical CPUs, VT-x | Met |
| Memory | 14.9 GiB, and 977 MiB of swap | None stated |
| Disk | One 931.5 GiB NVMe disk. Free: 49 GiB of 55 on `/`, 64 MiB of 22 GiB on `/var` (104 MiB at the second run; 11 GiB at the third, after the operator removed unused container images), 792 GiB of 838 on `/srv` | Met: `/srv` is a volume of its own that can hold the data directory |
| `/dev/kvm` | Present as `crw-rw---- root kvm`, with `kvm_intel` loaded. The operator's account is not in group `kvm` and could not open the device. Third run: the factory's user opened it, and `KVM_GET_API_VERSION` and `KVM_CREATE_VM` succeeded; no guest was booted (P-24) | Met |
| User namespaces | `kernel.unprivileged_userns_clone` is 1. AppArmor is enabled, but the kernel has no `kernel.apparmor_restrict_unprivileged_userns`, the setting that DEV-6 lifts on the hosted runners. The account created a user and a mount namespace and mounted a tmpfs in them | Met |
| `bubblewrap` | Not installed at first. Second run: 0.12.0, not setuid. The five tests of P-16 passed as the operator's account with no setup step, and the sandbox held no network interface but `lo`. Third run: the same as the factory's user, and a sandbox given a seccomp filter ran under it (P-24) | Met |
| `git` | 2.47.3, a version the capture prototype ran on (P-7) | Met |
| Agent CLI, Rust toolchain | Not found at first. Second run: Claude Code 2.1.311 under the operator's home directory, the version P-21 and P-22 ran on; no session was started. No Rust toolchain for that account. Third run: the factory's user has neither. The two runs for P-6 the same day: sessions ran as the factory's user from a copy of that binary, inside and outside a bubblewrap stand-in | Met for the operator's account only |
| `socat` | Not installed at first. The operator installed Debian's 1.8.0.3 on 2026-11-20 for the second run of P-6 and stated the same day that it stays. The agent CLI's own sandbox needs it, and AG-8 keeps that sandbox off | None stated |
| The factory's user | Third run: the system user `sf`, which of the groups `kvm`, `sudo`, `wheel` and `docker` is in `kvm` alone. Its home directory was not read. No systemd unit yet (AG-17) | Met |
| Reached | Over SSH, with a listener on port 22 on every address. The operator reaches it from the home network and, over a VPN, while travelling | Met |
| Reaches | `github.com`, `api.github.com`, `api.anthropic.com`, `index.crates.io` and `static.crates.io` answered over HTTPS with no proxy, and `git ls-remote` of the scratch repository worked. No request carried a credential | None stated |

Provisioning the server (22, M0) therefore still includes an agent CLI and a toolchain (WS-7) that the factory's user can read, and the data directory, which `/srv` has room for.

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
| `type:ci`, `type:spec-delta` | Change classes with their own integrity handling (11.3): `type:spec-delta` permits changes under `docs/specs/**`; `type:ci` turns a workflow change into a patch for a human to commit. |

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
6. Its title and body have not been edited since the `labeled` event that applied the current `state:ready`. An edit counts as since the event when its time is the event's own second or a later one. The times are the body's `lastEditedAt`, which a title change does not move, and that of the title's last rename event. GitHub reports whole seconds, and a body edit in the event's second cannot be placed before it or after it; the title is held to the same rule. One command that edits an issue and applies the label landed both in one second in every trial of P-8, and the issue then fails this rule. The check and the snapshot come from one GraphQL read (title, body, `lastEditedAt`, rename events, the `labeled` event), so they cannot diverge. On failure the factory removes `state:ready`, sets `state:triage` and says so once.

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

This field list is normative for `v1`. `escalation` is `{"category","seq"}` while the run is in `needs-human`. `children` (tier 2 parents only) is the list of sub-issue numbers. `head_sha` is written on the transition from `publish` to `ci`.

- **GH-4.** The marker is written only on stage transitions, on escalations and when the run reaches a terminal state (`"stage":"done"` or `"cancelled"`). Heartbeats and deadlines MUST NOT be written to GitHub.
- **GH-5.** The status comment is the comment whose id is in the journal or, on rebuild, the most recently created App-authored comment whose body starts with the marker block; earlier ones belong to terminal runs of the same issue. A marker whose stage is `done` or `cancelled` reconstructs no run. A marker anywhere else, including other App-authored comments, issue bodies and pull request bodies, is ignored. Dedupe markers (8.5) are recognised only as the first line of a comment or issue body authored by the App.
- **GH-5a.** All text that originates from an agent, an issue, a review or a log and is written to GitHub passes through one outbound encoder. It caps length, renders the text inside a fenced block labelled as agent output (except sub-issue bodies, PLAN-5), and neutralises `<!--`, `-->`, @mentions and issue references. Agent text never starts a body. The fence is longer than every run of backticks in the text, because text that ends its fence is live from there on. A reference is any of:
  - a `#` that a digit follows, which covers `#<digits>` and `owner/repo#<digits>`;
  - `GH-<digits>`, compared without regard to case;
  - `<owner>/<repo>/issues/<digits>` or `<owner>/<repo>/pull/<digits>`, with or without a scheme and a host before it, since GitHub takes the issue path alone for a reference.

  The encoder neutralises by inserting a zero-width space (U+200B), in fenced and unfenced text alike: after every `@`; after the `#` of the first form; after the hyphen of the second; before the number of the third, and after the colon of its scheme, so that no link to a wrong address is left; and after every `&` that a `#` or an ASCII letter follows. The last is because GitHub decodes a character reference before it looks for a mention or a reference, so that `&#64;`, `&#x40;` and `&commat;` are each a live `@`. A backslash escape or a character reference MUST NOT be used instead of the zero-width space: on a comment, an issue body and a pull request body neither stops a notification, a cross-reference or a closing (P-15). A commit message is not Markdown: a fence stops nothing there, and `&#<digits>;` is a reference to issue `<digits>`, under the first form. Nothing invisible is put into a commit subject, so a subject that contains a reference or a skip token (9.4) is rejected instead. A closing keyword is left as it is everywhere. It acts only through a reference directly after it, so a subject such as `fix: handle empty input` passes.
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
- **GH-9.** A factory check is reported for a SHA only when the gate ran on exactly that commit's tree. 12.3 and 8.4 accept a factory check only when its `external_id` is the run id.
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

- **GH-18.** The source of truth for change detection is a level-triggered poll: `GET /repos/{o}/{r}/issues?state=all&sort=updated&since=<watermark>` with an ETag, every 30 s *(tunable)*, refreshing only the items that changed. Open pull requests of active runs are polled for reviews and check results on the same tick. The default-branch commit produced by a factory merge is polled for its check results as well (12.3).
- **GH-19.** A full resync of all open work items runs every hour to catch anything the watermark missed.
- **GH-20.** The factory needs no inbound network endpoint. Webhooks are out of scope for v1; if added later they only enqueue "reconcile issue n now".

---

## 6. Work items and specs

### 6.1 Tiers

| Tier | Use | Required before `state:ready` | Plan gate |
|---|---|---|---|
| 0 | Chore: dependency bump, typo, formatting | Outcome and at least one acceptance criterion | No |
| 1 | Task or bug with one verifiable outcome | Full issue form (6.2) | Only if `risk:high` |
| 2 | Feature spanning several issues | A parent issue describing the feature | No: each proposed sub-issue is approved on its own |

A tier 2 parent is never implemented directly. It follows the parent pipeline (7.4): its plan proposes sub-issues, which the factory creates in `state:triage` for the operator to approve or close one by one.

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

GitHub does not document the layout of the body a form produces. As observed (P-4), it is one block per field in the form's order, with a blank line between blocks: the line `### <label>`, a blank line, then the field's text exactly as submitted. A field left empty gets the text `_No response_`. Nothing in a field's text is escaped, nothing follows the last field's text, and nothing on the issue records that a form made it.

- **WI-1.** The factory parses the body with a tolerant section parser keyed on heading text. It reads the body after the sanitisation of SEC-4 and accepts either line ending. A section starts at a level-3 ATX heading, outside a fenced code block, whose text is one of the six names, compared without regard to case. It runs to the next such heading or to the end of the body. Any other heading is part of the section it stands in, and text before the first section belongs to none. A section's text is what lies between, without the lines at either end that hold nothing but white space. A section whose text is then empty, or is `_No response_` and white space only, is empty.
- **WI-1a.** A name that starts more than one section fails the readiness lint (6.4), and the factory uses no section of such a body. A heading typed into a field is the form's own heading in the body, so such a body does not say where the field ends, and neither the first nor the last section of a name is reliably the field (P-4).
- **WI-1b.** The four bodies that P-4 recorded, with the fields as submitted, are fixtures of the parser's tests: the parser returns every field of the two bodies made from ordinary text, reports the repeated names of the third, and in the fourth finds no Stop conditions section, because that heading is inside a fence that an earlier field left open. Only the web interface renders a form, so `sf doctor --probe` does not repeat P-4: seeing whether GitHub changed the layout takes a new submission by the operator.
- **WI-2.** A `[NEEDS CLARIFICATION: …]` marker anywhere in the body blocks dispatch.

### 6.3 Specs in a target repository

- Living specs live at `docs/specs/<capability>/spec.md`. Each requirement has a stable id `R-<CAP>-<NNN>` and acceptance scenarios.
- A tier 2 change proposes a delta under `docs/changes/<parent issue>-<slug>/` with `ADDED`, `MODIFIED` and `REMOVED` requirement sections. Exactly one sub-issue of a tier 2 plan is the spec-delta issue (`type:spec-delta`); it is blocked by all its siblings and merges the delta into the living spec.
- Spec files are protected paths (11.3): changing them outside a spec-delta issue trips a flag.

The format is implemented natively. The factory does not shell out to spec-kit, OpenSpec or Kiro.

The factory's own repository does not use this layout yet (D-18, D-19).

### 6.4 Readiness lint

`sf issue lint <n>` and the dispatcher both run the same check. An issue is ready when: required sections for its tier are present and non-empty (WI-1); no name starts more than one section (WI-1a); every acceptance criterion has a verification method; no clarification marker exists; spec refs resolve at the default-branch head or in the parent's delta; the title contains no skip token (9.4); the body is under 32 KiB after sanitisation (13.2).

A lint failure creates no run. The factory moves the issue to `state:needs-human` with `needs:clarification` and posts one comment listing the failed rules, keyed `<!-- factory:lint <sha256 of title and body> -->`. `state:ready` on a `needs-human` issue that has no run is an ordinary dispatch candidate.

### 6.5 Discovered work

An agent MUST NOT widen its issue. It reports out-of-scope findings in the `follow_ups` field of its result (10.5). The control plane files each as a new issue in `state:triage` with `factory:origin`, linked to the source issue, up to 3 per run *(tunable)*. Factory-filed issues are never auto-approved: the factory never applies `state:ready` to any issue.

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
| plan gate | wait | GitHub | Required for `risk:high`; otherwise skipped (7.4) | — |
| `author_tests` | agent | sandbox | Optional (M3). A separate session writes failing acceptance tests from the criteria; their paths are then locked for `implement`. | Escalate |
| `implement` | agent, sole writer | sandbox | Change the workspace to satisfy the criteria; emit a result (10.5). | Agent failure (8.6) |
| `capture` | host + sandboxed git | sandbox | Build the candidate commit from the workspace and journal its SHA (9.4). | Rejected paths → `implement`, verify round +1 |
| `integrity` | deterministic | host | Diff policy on the candidate (11.3). | `fail` → `implement`, verify round +1; `flag` → continue, mark the pull request |
| `verify` | deterministic | sandbox | Run the policy's commands on a clean export of the candidate (11.2). | → `implement`, verify round +1 |
| `publish` | host | — | Push the journalled candidate; upsert the draft pull request; report the factory checks. | Transient retry; foreign commit → escalate |
| `ci` | wait | GitHub | Wait for the required CI checks on the head SHA (11.4). | Red check → `implement` with the failing log excerpt, CI round +1. Required check missing at the timeout → escalate with `infrastructure` (VAL-8). |
| `review` | agent, read-only | sandbox | Clean-context reviewer returns a verdict (11.5). | Blocking findings → `implement`, review round +1 |
| `ready` | host | — | Evidence manifest written; pull request marked ready; label `state:in-review`; review requested (4.2). | Cannot fail: its preconditions are checked at the end of `implement` and `review` |
| `human_review` | wait | GitHub | Wait for a submitted review (12.2). | Changes requested → `implement`, human round +1 |
| `merge` | host | — | Merge coordinator (12.3). | Conflict → base update (12.4) |
| `done` | host | — | Final status comment; run directory scheduled for removal. | — |

After any return to `implement`, the pipeline re-runs every later stage in order on the new candidate. A change after review always passes `capture`, `integrity`, `verify`, `ci` and `review` again.

**Round caps** *(tunable)*: plan 2, verify 3, CI 2, review 2, human 3, merge-conflict 2. Exceeding a cap escalates, except the merge-conflict cap, which triggers one automatic reset (7.6) if the run has not been reset before.

**Slots.** A run holds a concurrency slot from `claim` through `review`, including `ci`. It releases the slot in `human_review`, at the plan gate, in `await_children`, in `needs-human` and on hold, and re-acquires one before any return to a stage that runs in a sandbox (an agent stage, `capture` or `verify`), including re-entry at `integrity` after a base update (12.4). The run waits in its current stage until a slot is free.

### 7.4 Plan gate and tier 2 parents

**Plan gate.** It applies to `risk:high` issues. The factory posts the plan as one comment, records `plan_sha256` in the marker, removes any `factory:plan-approved` label, and moves the item to `state:needs-human` with category `plan-approval`.

- **PLAN-1.** The plan comment is rendered by the host from the planner's result, with agent text encoded (GH-5a). `plan_sha256` is the SHA-256 of the comment body as posted. A plan that does not fit one comment (60,000 characters) is a planner failure with a request to reduce.
- **PLAN-2.** An authorised actor approves by applying `factory:plan-approved` and then `state:ready`. The approval is valid only if its `labeled` event is later than the last edit of the plan comment and of the issue body, and the comment's current SHA-256 equals `plan_sha256`. When it consumes a valid approval the factory writes `plan_approved_by` to the marker and then removes the label; validity is evaluated once, at consumption.
- **PLAN-3.** `state:ready` without a valid approval is a revision request: `plan` runs again with the updated snapshot (plan round +1).

**Tier 2 parent pipeline:** `claim → plan → decompose → await_children → done`. A parent has no plan gate. The operator approves or closes each proposed sub-issue instead, so nothing the factory proposes is worked on without its own approval (INV-4).

| Stage | Kind | Summary | On failure |
|---|---|---|---|
| `decompose` | host | Create the proposed sub-issues from the planner's result (PLAN-4, PLAN-5) in `state:triage`, link them as sub-issues of the parent, create the blocked-by links, record `children` in the marker, and post one comment on the parent listing them. | Transient retry; otherwise escalate `infrastructure` |
| `await_children` | wait | Holds no slot. When every child is closed: if all were completed, close the parent as completed; otherwise escalate with `clarification`, so the operator can close the parent or open a new one with a revised scope. | — |

- **PLAN-4.** A plan proposes at most 10 sub-issues *(tunable)*. Each proposed body must pass the tier 1 readiness lint before any sub-issue is created. When the feature changes a spec, exactly one sub-issue is the spec-delta issue (6.3).
- **PLAN-5.** Sub-issue titles and bodies are the one place where agent-proposed text becomes an unfenced issue body, because the operator reads and approves exactly that text. The host builds each one once: the planner's text with the neutralisations of GH-5a applied but no fencing, capped at the 32 KiB lint limit (a longer body is a planner failure). PLAN-4 lints exactly these bytes. They hold the zero-width spaces of GH-5a, which SEC-4 removes before the parser, the lint or an agent reads the text. The issue body is the first-line `factory:subissue` marker (8.5), a newline, then those bytes, so the section parser (WI-1) and the readiness lint apply as they do to a human-written issue.
- **PLAN-6.** The factory applies `factory:origin`, the plan's proposed `risk:*` label and, on the spec-delta issue, `type:spec-delta` when it creates a sub-issue. Labels the factory applied at creation to an issue it authored are covered by the operator's approval: they are honoured once an authorised actor applies `state:ready` to that issue, as long as they have not changed since creation. The operator can change them before approving, under SEC-1.

The parent has a fence ref with no commits and no pull request; the ref is deleted when the parent's run ends. `factory:restart` on a parent in `decompose` or `await_children` is rejected with a comment.

### 7.5 Escalation

`needs-human` is a durable state, not an error. The run stays non-terminal and keeps its run row, branch, pull request and workspace. On escalation the factory:

1. Interrupts any stage execution and releases the concurrency slot.
2. Sets `state:needs-human` and records `escalation` in the marker with a category: `clarification`, `spec-conflict`, `plan-approval`, `budget`, `verification`, `review`, `merge-conflict`, `policy-denial`, `spec-changed`, `infrastructure` or `human-request`.
3. Posts one comment: what was tried, the evidence (links to checks, short log excerpts), one specific question or decision, and how to resume.
4. Mentions the approver once. It does not repeat the mention.

An agent may also stop on its own with status `needs_human` (a stop condition was met, the criteria conflict, a criterion is inapplicable, or the task is impossible as written). This is a valid, unpenalised outcome and does not count against retry caps.

A human applying `state:needs-human` to an active run is an escalation with category `human-request`.

While a run is in `needs-human`, only these rows of 7.7 are evaluated on every tick: the run's pull request is merged (the run enters `done`); issue closed and the pull request is not merged; `state:triage` applied by a human (the run is cancelled); `factory:hold`. The stop of 13.7 also applies. All other rows are evaluated once, at resume (7.5.1).

#### 7.5.1 Resume

A human resumes by answering (editing the issue or commenting) and applying `state:ready`. When the reconciler sees `state:ready` on an issue whose run is in `needs-human`, it does not run the claim. It:

1. Validates the label under SEC-1. If invalid, it removes `state:ready` and leaves `state:needs-human`.
2. Re-evaluates readiness rules 2–4 and 6 of 5.3. Hold or an open blocker: the issue keeps `state:ready` and the resume waits. Lint failure: the factory removes `state:ready`, leaves `state:needs-human`, sets the category to `clarification` and posts the lint comment of 6.4. Rule 6 failure (title or body edited since the resuming `labeled` event): the factory removes `state:ready`, leaves `state:needs-human` and says so once. On a resume the factory never sets `state:triage`.
3. Chooses the re-entry stage:
   - Title and body unchanged (`title_body_sha256` matches): the recorded stage. A new trusted comment alone never forces a re-plan.
   - Title or body changed, and every edit since the last snapshot is by an authorised actor: `plan`, and any plan approval is void. If the recorded stage is `human_review` or `merge`, the change is noted and the run resumes at the recorded stage.
   - An edit by anyone else, or by an editor that cannot be identified: no resume. `state:ready` is removed, the run stays in `needs-human` with category `spec-changed`, and the remaining steps are not performed.
   - If a valid `factory:restart` is also present, the re-entry is a reset (7.6) instead. Without `state:ready`, `factory:restart` on a run in `needs-human` has no effect.
4. Waits for a slot unless the re-entry stage is `human_review` or `merge`. The issue keeps `state:ready` while it waits.
5. Adds `state:in-progress` (or `state:in-review` if the re-entry stage is `human_review` or `merge`) and removes the other state labels.
6. Takes a new snapshot.
7. Sets the round counter of the gate that escalated to 0 and restarts that stage's deadline. For category `budget` it grants one more issue-budget increment (section 14) and counts it in `budget_grants`. For category `plan-approval` the plan counter is not zeroed: it counts revision requests (PLAN-3). When it exceeds the plan cap the factory does not re-plan; it escalates with `clarification`, and resuming from that escalation sets the plan counter to 0.

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
| Issue closed and the pull request is not merged | Cancel: run → `cancelled`; write the terminal stage to the marker; close the pull request; delete the branch per 18.2. |
| `state:triage` applied by a human | Cancel as above, leaving the issue open. |
| `state:needs-human` applied by a human | Escalate with `human-request`. |
| `state:ready` applied while the run is `in-progress` or `in-review` | Remove it and ignore it. |
| `factory:hold` applied | Interrupt and freeze. |
| `factory:restart` applied | If valid under SEC-1: reset (7.6) and remove the label; if the attempt cap is reached, escalate instead. If the public state was `in-review`, set `state:in-progress`. If invalid: remove the label and ignore it. A tier 2 parent in `decompose` or `await_children` rejects it (7.4). |
| `tier:*`, `risk:*` or `type:*` changed after claim | Escalate with `spec-changed`. |
| Title or body changed by an authorised actor before the `ready` stage (tier 2 parent: before `decompose`) | Interrupt, take a new snapshot, restart from `plan` in place. Not counted as a failure. |
| Title or body changed by anyone else, or the editor cannot be identified | Escalate with `spec-changed`. |
| Title or body changed after the pull request is ready, or on a tier 2 parent in `decompose` or `await_children` | Note it in the status comment; no automatic action. |
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
| Activity | Agent session, capture, verify (each including its setup commands) | Never in the outbox. Recorded as an activity row (id, sandbox record, process data, state). After a restart it is killed and marked abandoned; the engine receives `ActivityAbandoned` and requests a new one with a new id. |
| Timer | Stage deadlines, backoff, poll | Stored as a deadline on the run row and re-armed from the store at startup. |

- **ORCH-4.** The engine receives only coarse activity events: `ActivityStarted`, `UsageBatch` (all per-model-call usage records since the last batch, with the figures of BUD-9, at most once per 5 s and always before the end event), `LimitReached(kind)`, `LoopDetected`, `Stalled`, `ActivityEnded(outcome, chain_digest)`. In `gateway` mode the usage records in `UsageBatch`, and the spend the supervisor compares with the session's dollar limit, come from the model gateway's meter for the run's token; the CLI's own usage events are written to `events.jsonl` and used only to report a discrepancy. In `subscription` and `direct_key` modes the CLI's usage events are the only source (13.5, section 14). Idle, loop and per-session limits are evaluated in the supervisor from limits passed in the session spec. Raw and normalised agent events go only to `events.jsonl`, which the supervisor hash-chains. The supervisor's stdout reader never blocks on the engine channel.
- **ORCH-5.** `step` runs under `catch_unwind` per run; a panic moves that run to `needs-human` with `infrastructure`. A panic or I/O error in the store writer stops the daemon with a non-zero exit; restart recovery (8.4) is the handling.

### 8.2 Claim

The daemon is the only dispatcher for a store, enforced by an exclusive file lock on the data directory. Claiming an issue is, in order:

1. One GraphQL read of the issue: check SEC-1 on the `state:ready` label and rule 6 of 5.3; capture the snapshot; record tier, risk and type under SEC-1. Read the default-branch head once: it is both `base_sha` and `policy_sha`. Refuse a base tree that contains a submodule entry. Run the readiness lint on the snapshot.
2. Insert the run row carrying all of the above. A unique index on (repository, issue) for non-terminal runs makes this the authoritative local claim.
3. Journal the attempt, then create `refs/heads/factory/issue-<n>` at `base_sha` with `POST /git/refs`. GitHub refuses the call with a 422 when the ref exists, and with a 422 for every fault in the ref name or the object that probe P-1 tried, and only an undocumented message tells them apart. A 422 from this call is therefore not a Deterministic failure under 8.6 until the claim has read the ref by its exact name, with `GET /git/ref/heads/factory/issue-<n>`:
   - The ref exists. It counts as created only on a replay, and only if its SHA equals the journalled `base_sha` of this run. A create is a replay when an earlier create of this run may have taken effect unseen: the attempt was already journalled when the daemon started (8.4), or the earlier request ended in a Transient failure (8.6). A ref left by a `cancelled` run of this store is force-updated. Any other existing ref aborts the claim, a ref at `base_sha` that answers the first create of the run included, since this run did not make it: the run row is closed as `cancelled` and the issue goes to `state:needs-human` with category `policy-denial`.
   - The ref does not exist (404). The create was refused for another reason and is a Deterministic failure (8.6), and the escalation carries GitHub's message. One such reason is a branch whose name collides with the ref, `factory` or one under `factory/issue-<n>/`, which GitHub reports only as `Reference update failed`.
4. Add `state:in-progress`, then remove `state:ready` and every other `state:*` label (`state:triage`, or `state:needs-human` left by a lint failure), and remove `needs:clarification` if present.
5. Create the status comment with the complete marker.

Running two daemons against one repository from different hosts is unsupported. The branch-ref step is a best-effort fence against that mistake, not a distributed lock. Neither GitHub's answer nor the ref says who created it (probe P-1), so a run that replays the create cannot tell its own ref from one that another daemon made at the same `base_sha` in the meantime.

**Deadlines.** There are no leases. The supervisor owns one set of deadlines per stage kind:

| Stage kind | Deadline |
|---|---|
| Agent | Idle: no agent event for 300 s while no tool call is outstanding. Tool call: a single tool call outstanding for more than `tool_call_timeout_secs` (policy, default 1800). Wall clock and budget: section 14. |
| Deterministic (capture, verify, and the setup step of any activity) | The command timeout only. |
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
| Dispatch schedule | Optional hours in which new stages may start (operator), to leave plan capacity for the operator's own interactive use |

When the review-queue limit is reached, no new `implement` stage starts in that repository; runs already past `implement` continue. Dispatch also pauses when the provider reports that a limit is reached (in `subscription` mode: until the plan limit's reported reset time; running sessions are interrupted and their stages restart afterwards without a round or failure being counted; the `rate_limit_event` with status `allowed` that every completed session emits is not such a report, probe P-22), on the GitHub write budget (GH-16), and when a circuit breaker is open (8.6). Before that pause exists (M2), a session that meets the plan's limit ends as BUD-9 says.

### 8.4 Restart recovery

On startup the daemon, before dispatching anything:

1. Takes the store lock.
2. Reaps every activity recorded as live: destroys its sandbox instance and sweeps for surviving processes (AG-18).
3. For each non-terminal run, compares the `runs` row with GitHub (labels, marker, branch head, pull request, checks) and with the run directory. GitHub wins. The run resumes at the start of its recorded stage, replaying journalled sub-steps (a journalled candidate SHA, a journalled push) instead of recomputing them.
4. If the store is missing or corrupt, `sf rebuild` reconstructs runs from status-comment markers, branches and pull requests, under GH-5b:
   - The approver is re-read from the latest `labeled` event for `state:ready`.
   - A branch head that differs from the marker's `head_sha` is accepted only if it carries a `factory/integrity` check run from the factory App with this run id; otherwise it is a foreign commit.
   - A run whose marker stage is `ci`, `ready`, `human_review`, `merge` or `await_children`, or that is waiting at the plan gate, keeps that stage; a plan approved after a rebuild is re-read from the plan comment after its hash is checked against `plan_sha256`. A run rebuilt in `decompose` re-runs it; the sub-issue markers of 8.5 make that idempotent. A run rebuilt in any other stage restarts at `plan` with a workspace created from `head_sha` (from `base_sha` when nothing was published), because its handoff files are gone.
   - A marker that carries an `escalation` rebuilds the run in `needs-human`; these rules apply to its recorded stage at resume (7.5.1).

### 8.5 Idempotency keys

| Side effect | Key and replay rule |
|---|---|
| Branch | `base_sha` and the attempt are journalled before the create is sent; on a replay, an existing ref equal to `base_sha` counts as created (8.2). |
| Candidate commit | Tree, parents, dates and message are journalled with the candidate SHA before any push. Recovery uses the journalled SHA and never rebuilds a journalled candidate. If the tree equals the previous candidate's, no new commit is made. |
| Push | The executor reads the remote ref first. Remote head equals the candidate: pushed. Remote head equals the previously journalled head: push. Anything else: foreign commit (7.7). |
| Pull request | Lookup of open pull requests by head branch before create. |
| Status comment | Comment id in the journal; on rebuild, GH-5. |
| Other comments | First-line marker `<!-- factory:event <run>/<seq> -->` in an App-authored comment, looked up before posting. |
| Check run | List check runs for the head SHA filtered by name and App id; update the one whose `external_id` matches, else create. |
| Merge | Read the pull request before every merge call: merged with our head SHA is success. A 409 from `merge-async` means a request is pending: poll the pull request's merged state for up to 10 minutes, then re-check eligibility. |
| Agent session | Session id generated and journalled before spawn; a restarted activity gets a new id. |
| Follow-up issue, sub-issue | First-line marker `<!-- factory:followup <run>/<index> -->` or `<!-- factory:subissue <run>/<key> -->` in the body; lookup by listing issues created by the App since the run started, never the search API. |

### 8.6 Failure handling

| Class | Examples | Handling |
|---|---|---|
| Transient | Network error, 5xx, provider overload, secondary rate limit | Exponential backoff with jitter, capped at 5 minutes; no round cost |
| Deterministic | 401, 403, 422, invalid policy, missing sandbox | No retry; escalate with `infrastructure` or pause dispatch |
| Gate failure | Capture, integrity, verify, CI or review failed | Bounded rounds (7.3) |
| Agent failure | Status `failed`, no result, schema-invalid result | Restart the stage once with a fresh session and the failure summary in the handoff; then escalate with `verification` |
| Stall | The idle or tool-call deadline of 8.2 passed | Interrupt; restart the stage once with a fresh session; then escalate |
| Budget | Any limit in section 14 reached; usage credits in use (BUD-8) | Stop the stage; escalate with `budget` (through M1: BUD-7) |
| Poison | The same stage was abandoned or restarted 3 times, not counting a restart the operator starts under BUD-7 | Escalate with `infrastructure` |
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
    quarantine/                    repository of the sandboxed staging git: its index and the objects it writes (9.4)
    ingest/                        object directory the host indexes a captured pack into before it enters host.git (9.4)
    export/<sha>/                  clean checkout of a candidate, for verify and review
    target/                        CARGO_TARGET_DIR for implementer stage executions
    handoff/                       plan.md, progress.md, feedback/*.md, diff files (host-written)
    exec/<seq>/home/               fresh HOME, CARGO_HOME, temp and agent config per stage execution
    exec/<seq>/target/             CARGO_TARGET_DIR for every other stage execution (new and empty each time)
    events.jsonl                   raw agent event stream, hash-chained
    stages/<seq>-<stage>.json      stage results
    logs/                          full command output
```

`handoff/` and `logs/` are host-written. Workers see a per-role, read-only view of them outside the workspace root: the reviewer's view excludes `progress.md` and implementer feedback. The host writes `progress.md` from the `notes` field of each result, so it never reads a file out of a workspace.

### 9.2 Workspace creation

- **WS-1.** The host fetches into the bare mirror, then creates `workspace/` with `git clone --no-local --no-checkout --single-branch --branch <default> <mirror> workspace`, checks out `base_sha` detached and removes the remote, all before any worker touches the directory. The checkout applies no in-tree attributes (WS-11). `--reference`, `--shared`, hard-linked and plain local-path clones are not used: the workspace MUST contain its own object store holding only objects reachable from the base and from this run's own candidates, because the mirror is not visible inside the sandbox and holds other runs' unmerged branches.
- **WS-2.** Directory names are derived from ids, never from issue text. The agent's working directory MUST be the workspace root.
- **WS-3.** The agent MAY run git in its workspace, including committing for its own checkpoints. Nothing in `workspace/.git` is ever used by the host.
- **WS-4.** Policy may define `setup` commands (for example `cargo fetch --locked`). They run inside the sandbox, with a timeout, at the start of every stage execution that builds: each implementer session and each verify. Setup is not a stage execution of its own: it runs as the first step of the activity it prepares, in the same sandbox instance with the same home and target directory, under `setup.timeout_secs`. A setup failure fails that activity with category `infrastructure`.
- **WS-5.** Run directories are removed 24 hours after a terminal state *(tunable)*; `target/` and exports are removed earlier (9.3).
- **WS-6.** The workspace tree MUST derive from the previous candidate. After a base update (12.4) the host discards `workspace/` and re-creates it from the update commit, so a later round can never revert what landed upstream.
- **WS-7.** The host resolves the toolchain named by `rust-toolchain.toml` at the base commit (channel, components, targets), read as data from the object database, to an installed toolchain directory and puts that directory's `bin` first on the worker `PATH`. rustup and mise shims are never on the worker `PATH`. A missing toolchain or component is a blocking onboarding failure.
- **WS-10.** `host.git` is created at claim as a bare, single-branch clone of the mirror's default branch (`--no-local`), with its remote removed. Before every test-merge, base update or rebuild the host fetches the default branch (and, on rebuild, this run's work branch) from the mirror into it. It holds only objects reachable from the default branch and from this run's candidates. A workspace made from a commit that is not on the default branch (WS-6, 12.4, 8.4) is cloned from `host.git` at a temporary ref on that commit in the same way as WS-1; the temporary ref is deleted afterwards.

Linked worktrees are not used: they share refs, config and hooks with the hub repository, which conflicts with INV-2.

### 9.3 Build output

- `CARGO_TARGET_DIR` is outside the workspace. Implementer stage executions of one attempt share `runs/<run_id>/target`. `plan`, `author_tests`, `verify` and `review` each get a new, empty target directory on every execution, so a build there is cold each time. That cost is accepted; CI is the second clean-environment gate.
- `target/`, per-execution target directories and exports are deleted when the run enters `human_review` or a terminal state.
- No cache writable by a worker is shared between runs, or between the implementer and any other stage. `sccache` is not supported in v1. Seeding target directories from a host-built copy-on-write clone MAY be added after probe P-13 measures it.

### 9.4 Capture: building the candidate

Capture runs after the implementer's sandbox instance is destroyed and the process sweep (AG-18) is empty.

1. **Scan (host, `lstat` only, never following symlinks).** Fail if any entry other than the root `.git` has a name that git would treat as `.git` (case-insensitive, including filesystem-equivalent spellings), at any depth; or if any entry is not a regular file, directory or symlink. The root `.git` is not entered. The scan covers ignored directories, and it records the type and size of every entry. It runs before any git command, because git leaves some of these entries out of its listings without a message (an untracked FIFO or socket, a `.git` entry it does not take for a repository, anything in an ignored directory), and git blocks when `.gitignore` is a FIFO (P-7).
2. **Stage (sandboxed git).** `quarantine/` is a repository the host creates anew for each capture: an empty object directory with `host.git`'s objects as an alternate, and a host-written configuration (hooks, filters, fsmonitor, system and global config all disabled; no in-tree attributes, WS-11). The host's git binary runs with `quarantine/` as its repository and the workspace as its work tree, in new sandbox instances with no network and no credentials, under the filesystem policy of SEC-6 for capture. Everything the commands of this step share is in `quarantine/`. The staging git builds a fresh index from the tree the workspace was last created or captured from (the previous candidate's tree, `base_sha` on the first round, or W's tree in a merge-conflict round, 12.4), so upstream content is never counted as the round's change set. The index file lives in `quarantine/`; nothing in `workspace/.git` is read (WS-3). It then lists the change set: changed and deleted tracked paths with `git diff-files`, submodules ignored, and untracked paths with `git ls-files --others --exclude-standard`. The ignore rules are therefore the `.gitignore` files in the workspace as they stand at capture, the agent's edits included, and nothing else: `quarantine/` has no `info/exclude`, no global excludes file is configured, and the workspace's own `info/exclude` is not read. An ignore rule never drops a tracked path. The host checks the listing against the inventory of step 1, without touching the workspace again. It rejects any listed path that is neither a regular file nor a symlink (git lists a nested repository as its directory) or exceeds 5 MiB, and any change set over the total-bytes cap. If nothing is rejected, the staging git removes the deleted paths from the index (`git update-index --force-remove`), adds the others (`git update-index --add`) and lists the change set again. A path still listed was not staged, and that fails the capture: `update-index` skips a path it refuses and still exits 0. The staging git then writes a tree and emits the objects it wrote as one pack on its standard output (`git pack-objects --local`). A rejected path fails the capture; the host never builds a candidate that silently omits one.
3. **Ingest (host, bare `host.git`).** The host takes two things from the sandbox, the name of the tree and the pack, and opens nothing in `quarantine/`. It indexes the pack into `ingest/`, a new object directory with `host.git`'s objects as an alternate, using `git index-pack --strict` with `--max-input-size` set from the total-bytes cap. That recomputes the name of every object, runs the `fsck` checks, and fails if the pack refers to an object that is neither in the pack nor in `host.git`. The host then walks the new tree and rejects: any entry whose mode is not `100644`, `100755` or `120000`, so submodule entries cannot enter a candidate; and, by the sizes of the blobs that differ from the previous tree, any file or change set over the capture limits. Only then does the pack move into `host.git`. After a rejection `ingest/` is deleted and `host.git` is unchanged.
4. **Commit.** `git commit-tree` with the previous published head (or `base_sha`) as parent. From a conflicting base update until the next push, the parents are instead W's two parents (previous published head, new default-branch head), never W itself, so the default-branch head stays an ancestor of the work branch. The commit carries the factory identity, journalled dates, the validated subject and the host-written body and trailers. Journal the candidate and tree SHA.

**Capture limits** *(tunable)*: 5 MiB per file, 50 MiB of new or changed bytes per candidate, and a 300 s timeout for the sandboxed staging step. Repository policy may lower them and cannot raise them above operator configuration. A capture timeout is an abandoned activity (8.6, Poison).

**Subject validation.** The subject is a single line of at most 72 characters without control characters. It is rejected, returning to `implement`, if it contains (case-insensitively) `[skip ci]`, `[ci skip]`, `[no ci]`, `[skip actions]`, `[actions skip]` or `skip-checks`, or anything GH-5a forbids. The pull request title and the squash title are generated by the host from the issue title, after the sanitisation of SEC-4, with every reference (GH-5a) removed (skip tokens are already excluded by the lint, 6.4); the squash message is host-written (5.6).

**Export.** `verify` and `review` use a clean checkout of the candidate made from `host.git`: `git read-tree <candidate>` into a temporary index, then `git --git-dir=host.git --work-tree=runs/<run_id>/export/<sha> checkout-index -a` into that new, empty, host-created directory (bare `host.git` has no work tree of its own, and `checkout-index` needs one), with no filter configured and no attribute applied (WS-11; left to itself, `checkout-index` applies the candidate's own `.gitattributes`). `git archive` MUST NOT be used, because it honours `export-ignore` from the candidate's own `.gitattributes`. The export is writable and disposable for `verify`, read-only for `review`.

**Publish.** Push the journalled candidate to `refs/heads/factory/issue-<n>` over HTTPS. The installation token reaches `git push` through a credential helper built into `sf` that receives it over an inherited pipe; it never appears in arguments, environment or a file (INV-1). Then create the draft pull request if no open one exists for the head branch, otherwise update its body, and report the three factory checks for the head SHA.

- **WS-8.** All git mutations run through the `git` CLI in one wrapper crate that parses only porcelain or NUL-delimited output. `gix` MAY be used for read-only fast paths. `git2` is not used. Fetches into a mirror are serialised per repository.
- **WS-9.** The capture, ingest and export design MUST be prototyped in M0 against a hostile-tree suite before the pipeline depends on it: nested repositories (directory, gitfile, `.GIT` case variant) carrying `core.fsmonitor`; hooks and config; symlinked directories; FIFOs and hard links; `.gitattributes` with filters, `export-ignore` and `-diff`; an agent-edited `.gitignore`; LFS pointers; case collisions; executable bits; large files; an object from another run's branch must not be readable in a fresh workspace. Probe P-7 did this for the git commands of this section, without the factory's sandbox, and added cases: a `.git` entry that names no repository; a special file or a nested repository in an ignored directory; a FIFO named `.gitignore`; a tracked directory replaced by a symlink; attributes that change bytes (`eol`, `ident`, `working-tree-encoding`), in the base commit and in the round; and a hostile pack or tree name in place of the staging git's output.
- **WS-11.** No in-tree attribute is applied when content moves between a tree and a directory. The workspace checkout (WS-1), staging and the export run with the attribute source set to the empty tree, as the diffs of VAL-5a do. Each therefore moves bytes unchanged: an untouched workspace is an empty change set, and every path of a candidate has in the export the bytes it had in the workspace. The source is set with the `--attr-source` option and not with the `GIT_ATTR_SOURCE` variable alone, because a git too old to know the option rejects it but ignores the variable and applies the attributes. The cost is that `text`, `eol`, `ident` and `working-tree-encoding` do nothing in a workspace, a candidate or an export: a file the agent writes with CRLF line endings is committed with them (P-7).

### 9.5 Ecosystems

Target repositories are Rust first, with other languages to follow (decision D-10). Everything specific to a language's toolchain therefore sits behind one interface, `Ecosystem`, in `factory-core`, with `rust` as the only implementation in v1:

- toolchain resolution and the worker `PATH` (WS-7);
- the environment for build output and caches (9.3);
- the default `setup` and `verify` commands offered at onboarding (11.2);
- integrity rules for dependencies and build scripts, lockfile and generated-file defaults, and the built-in deny list (11.3);
- the advisory database source (VAL-1) and the mutation-testing command (11.7);
- the dependency-bump class predicate (13.6);
- ecosystem-specific readiness checks at onboarding (20.1).

Policy names the ecosystem (`ecosystem = "rust"`). The pipeline, sandbox, GitHub model and gates contain no Cargo-specific logic outside this interface, and a structural test enforces that. Where this specification names `cargo` commands or Cargo files elsewhere, it describes the `rust` implementation. Adding a language means adding an implementation, not changing the pipeline. Repositories that mix ecosystems are out of scope for v1.

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
    limits: Limits,              // turns, usd (min of stage cap and remaining issue budget; through M1 the stage cap, BUD-7), wall clock, idle, tool call
    env: BTreeMap<String, String>,
}

// SessionHandle: events() -> Stream<AgentEvent>; interrupt(); kill(); wait() -> SessionOutcome
```

`AgentEvent` is a normalised enum: `Init`, `AssistantText`, `ToolCall`, `ToolResult`, `Usage`, `RateLimited`, `Result`, `Unknown(raw)`. Unknown message types and fields are preserved, never fatal.

v1 ships one driver, for Claude Code. A Codex driver (which can likewise reuse a saved ChatGPT login) and a generic Agent Client Protocol driver are later additions behind the same trait; the ACP crate has had four major versions in five months, so it is not a v1 dependency.

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

- **AG-1.** Every invocation is headless, and bare except in `subscription` mode (AG-3a): `claude -p --bare --setting-sources user --output-format stream-json --verbose --input-format stream-json`, with an explicit `--session-id`, `--model`, `--permission-mode`, `--permission-prompts none`, tool allowlist (`--tools` / `--allowedTools`), `--settings <file>`, `--mcp-config <file> --strict-mcp-config`, `--json-schema <inline schema>`, `--max-turns`, `--max-budget-usd`, `--append-system-prompt-file <file>` and `--include-partial-messages` (BUD-4). Flag names are those of Claude Code 2.1.311. The driver owns the mapping. `sf doctor` verifies each flag by invoking the CLI with it against a stub model endpoint and checking for an unknown-option error; it does not parse `--help`, which omits some supported flags.
- **AG-2.** Bare mode skips the repository's `CLAUDE.md`, hooks, skills, plugins and `.mcp.json`, but project settings files can still apply an `env` block and helper commands. `--setting-sources user` is therefore mandatory, so no project or local settings file is read. This is intended (INV-2, INV-10).
- **AG-2a.** Repository instructions are computed by the host at the policy commit: `AGENTS.md` and the files it references, to at most 2 levels, within a size cap, only inside directories that policy declares (default `docs/**`), resolved only to regular blobs read from the object database. Symlink entries and paths containing `..` are rejected; HTML comments are stripped. The implementer and planner receive them through the system prompt file. The reviewer's system prompt contains only the factory's role prompt; repository instructions reach it in a delimited data block.
- **AG-3.** Authentication follows the credential mode (13.5): `subscription` (default) passes a long-lived token from `claude setup-token` as `CLAUDE_CODE_OAUTH_TOKEN`; `gateway` points the CLI at the model gateway; `direct_key` passes an API key or `apiKeyHelper`.
- **AG-3a.** Bare mode does not read the subscription token, so in `subscription` mode the CLI runs without `--bare` and the isolation bare mode gave is rebuilt from other controls: `--setting-sources user`, `--strict-mcp-config` with a host-written MCP file, and the per-execution config directory of AG-4 as the only user-level configuration. On Claude Code 2.1.311 these kept out of the session everything that probe P-21 planted in the working directory and that took effect without them: `CLAUDE.md`, `CLAUDE.local.md`, `.claude/rules/` and `AGENTS.md`; skills, commands and subagents; the hooks and `env` block of `.claude/settings.json` and of `.claude/settings.local.json`, and the permission rules of the second; `.mcp.json`; a plugin folder under `.claude/skills/`; and what the CLI otherwise picks up after the start, from a subdirectory or from a file written during the session. The flags are those of every role (AG-1), so the planner and the implementer load none of these files from the workspace either, and repository instructions reach a session only through AG-2a (INV-10). This was observed on one CLI version, on macOS and outside the sandbox. Until the contract test (19.4) has passed on the server, the host still removes `CLAUDE.md`, `CLAUDE.local.md`, `.claude/` and `.mcp.json` from the export the reviewer runs on, as a second layer (the diff still shows changes to them). The contract test asserts the lock-down on every tested CLI version, because the CLI's defaults for `-p` are announced to change.
- **AG-4.** Each stage execution gets a fresh config directory under `exec/<seq>/home/`, created by the host. The CLI reads its user-level configuration from this directory and applies a change to it while a session runs (probe P-21). So the sandbox makes read-only to the worker everything in it that the CLI reads as configuration: the settings file and `skills/`, for which the probe observed this, and `CLAUDE.md`, `rules/`, `commands/` and `agents/`, for which it is assumed. The host creates each of them, empty where it has no use for it. The rest is writable for transcripts. The host's `--settings` file lives outside any worker-writable path.
- **AG-5.** The driver checks the CLI version against a tested range at startup and feature-detects from the `capabilities` array of the `system/init` message. Outside the range, the daemon refuses to dispatch.
- **AG-6.** The raw stream is appended to `events.jsonl` before parsing. Lines up to 10 MiB are accepted; longer lines fail the session. The structured result is read from the `structured_output` field of the final `result` message; its absence is an agent failure.
- **AG-7.** Permission prompts and user-input requests are denied, never awaited.
- **AG-8.** The settings file raises the CLI's maximum shell-tool timeout to `tool_call_timeout_secs`. The host's `--settings` file (AG-4) sets `sandbox.enabled` to `false`. The CLI's own Bash sandbox is not relied on (SEC-8), so it is kept off in every session and not left to its default, which is off on Claude Code 2.1.311. Probe P-6 found nothing gained by turning it on: it then either did not start, or took from a shell command part of what the outer sandbox had left it, or failed the command.

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

- **AG-11.** Every sandboxed command (agent session, setup, capture, verify) is spawned as the leader of its own process group inside a sandbox instance, with a cleared environment plus an allowlist: `PATH` (9.2 WS-7), `HOME`, `TMPDIR`, `CARGO_HOME`, `CARGO_TARGET_DIR`, locale, `TRACEPARENT`, `HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY`, and, for agent sessions only, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` (SEC-11) and the model credential for the mode (13.5): the subscription token, or the model gateway address and per-run token.
- **AG-12.** Stop sequence: send the protocol interrupt or SIGINT and wait up to 10 s for the result message; close stdin; SIGTERM to the group; after 5 s, SIGKILL to the group and destroy the sandbox instance. After SIGTERM the supervisor does not wait for a result message, because the CLI does not emit one.
- **AG-13.** Stdout is read through a length-capped line codec; stderr is drained concurrently to a log file. Pipes are used, not a PTY. After the leader exits, output is drained for at most 2 s and the pipe handles are then closed, so a grandchild holding a pipe cannot hang the supervisor.
- **AG-14.** The agent's own `--max-turns` and `--max-budget-usd` are set equal to the factory's session limits, so the control plane's threshold (BUD-1) fires first, except for what subagents spend (BUD-10); a CLI-side cap result is handled as Budget.
- **AG-15.** The sandbox record, pid, process group id and process start time are journalled before the first exec.
- **AG-16.** Every sandboxed command is started through a supervisor shim (`sf __supervise`, same binary) that holds a pipe from the daemon and kills its whole tree when that pipe closes, so workers do not outlive a killed daemon. The sandbox also uses a PID namespace with die-with-parent (level `process`) or a VM (level `hardened`), so no worker process outlives its instance.
- **AG-17.** `sf` ships a systemd unit whose stop timeout (45 s) exceeds the daemon's shutdown budget (19.3).
- **AG-18.** After every sandboxed command the supervisor sweeps for survivors: any process whose process group or session equals the journalled one, or whose working directory or executable is under `runs/<run_id>/`. They are killed, and the next stage MUST NOT start until the sweep is empty. The same sweep is the orphan rule of 8.4. With a PID namespace or a VM per instance, the sweep is a second check and not the primary control.
- **AG-19.** The egress proxy and the model gateway admit connections for a run only while one of its activities is live. This closes the spend and exfiltration path for a process that escaped the sweep. The gateway accepts a per-run token only on connections that arrive through that run's own proxy endpoint (SEC-10); a token presented anywhere else is refused and logged with both run ids.
- **AG-20.** An agent session may start subagents (D-22). Their calls count toward the session's limits as BUD-6 and BUD-10 say.

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
| 8 | Holdout scenarios in a separate private repository | Does it work on checks the builder never saw? | M6, for classes that require it (13.6) |

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

- **VAL-1.** Commands run on the candidate export in a new sandbox instance with a fresh home and target directory, the same sandbox level as the agent, and no credentials (INV-3, INV-11). `setup` commands run first. A change that adds or alters dependencies makes `cargo build` arbitrary code execution; the sandbox, not the command list, is the control. The host fetches the RustSec advisory database daily (never a worker) and exposes it read-only to verify; `cargo deny` in verify runs without fetching.
- **VAL-2.** `factory/verify` records the tree SHA that was verified and is reported only on a head whose tree equals it (GH-9).

### 11.3 Integrity check

A deterministic check on `base_sha..candidate`, run by the host on every candidate. Each rule yields `pass`, `flag` or `fail`.

| Rule | Outcome |
|---|---|
| Change under a **locked path** (tests written by `author_tests` for this run) | `fail` |
| Change under `.github/workflows/` | `fail`. For a `type:ci` issue the run escalates with `policy-denial` and attaches the proposed workflow as a patch for a human to commit, because the App cannot push it (GH-11). A `state:ready` resume from this escalation is handled as a reset (7.6): the new attempt starts from the default-branch head that then contains the committed workflow. If nothing else remains to do, the human closes the issue instead. The escalation comment states both options. |
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

- **VAL-6.** The operator installs the workflow template supplied by `sf repo add` (20.1). It runs on `pull_request`, `merge_group` and `push` to the default branch and on nothing else, with top-level `permissions: contents: read`, no secrets, GitHub-hosted runners, and actions pinned to full commit SHAs. It SHOULD fetch dependencies first and then build and test in a container without network access, because CI otherwise gives agent-written code an open network.
- **VAL-6a. Workflow audit.** At startup, on every default-branch head change and in `sf repo add`, the factory parses every workflow at the default-branch head. Required from M2. Dispatch in the repository pauses (as for an invalid policy, 16.3) unless:
  - every workflow that runs in the context of a factory ref (`push` matching `factory/**`, `pull_request`, `merge_group`) declares top-level `permissions` with no write scope, references no `secrets.*`, uses no `secrets: inherit`, uses GitHub-hosted runners and pins actions to SHAs;
  - `pull_request_target` and `workflow_run` workflows exclude factory branches;
  - every base-context workflow (`issues`, `issue_comment`, `pull_request_review`, `check_*`, `status`) is acknowledged in operator configuration by blob SHA, checks out no pull request head and interpolates no event text into `run:`; a changed blob pauses dispatch until re-acknowledged.

  Anything the parser cannot resolve (a reusable workflow, an unknown expression in `on:` or `permissions:`) fails closed.
- **VAL-7.** Factory pull requests come from a branch in the same repository, so they do not get the fork sandbox. Release and deployment workflows MUST live behind protected environments and MUST NOT share caches with pull request workflows.
- **VAL-8.** The required CI checks are `required_checks = [{ name, app_id }]` in operator configuration per repository, which policy may extend but not shorten; the list is non-empty for any repository the daemon dispatches on. `name` is the check-run name as the API returns it (the job name). In `enforced` mode the factory cross-checks the list against the ruleset. The `ci` stage passes when every listed check exists on the head SHA with conclusion `success`, `neutral` or `skipped` from the listed App; it fails on any other conclusion (CI round +1). A check still missing at `ci.timeout_secs` is not a CI round: the run escalates with category `infrastructure` and does not return to `implement`.
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
- **Holdout scenarios (M6).** End-to-end scenarios kept in a separate private repository that no worker can read, run by the control plane against the built artifact. Only a behavioural description of a failure is returned. Evidence that this generalises is thin, so only classes that cover service behaviour require it (13.6).

---

## 12. Pull requests and merging

### 12.1 Pull request lifecycle

Draft at first publish → updated on every round → marked ready after all automated gates pass on the head SHA → review requested (4.2) → merged or reset.

### 12.2 Human review

- **PR-1.** The factory acts on submitted reviews, not on individual review comments. "Changes requested" by an authorised actor starts a human round: the review body and its line comments (from authorised actors only) become gate feedback.
- **PR-2.** An approval counts only if it is from an authorised actor and on the current head SHA. Approvals from Apps and bots never count in the factory's own check, whatever GitHub counts. Any push to the work branch invalidates earlier approvals.
- **PR-3.** If a ready pull request has had no review for 3 days *(tunable)*, the factory nudges the approver once on the dashboard issue and then stops.

### 12.3 Merge

Merge strategy per repository: `human` (the human merges; the factory observes and finishes the run when it sees the merge, 7.7), `factory_serial` (default from M2), or `native_queue` (not scheduled: merge queues are unavailable on personal-account repositories).

Under `human` the factory still runs the test-merge of step 2 on every tick while the run is in `human_review` or `merge`: a conflict starts a base update (12.4). Before base update exists (M2), `sf run` reports the conflict in the status comment and stops; the operator cancels the run and approves the issue again.

For `factory_serial`, the merge coordinator handles one pull request at a time per repository:

1. **Eligibility**, in every mode: an approval valid under PR-2, or a class decision under 13.6; the three factory checks and the required CI checks satisfied on the head SHA; the head SHA is one the factory pushed; the base is the default branch; no `factory:hold`; no stop (13.7). In `factory-enforced` mode the three factory gates are read from the journal (after a rebuild: from check runs created by the factory App).
2. **Test-merge** the head into the current default-branch head in bare `host.git` (`git merge-tree --write-tree`, with no in-tree attributes applied). If it conflicts, or if policy `merge.update_when_behind = "always"` and the branch is behind, do a base update (12.4). Otherwise leave the branch untouched, so the approval stays on the SHA the human saw.
3. **Re-check** approvals, hold and stop immediately before the call, then merge:
   - `enforced` mode: `PUT /repos/{o}/{r}/pulls/{n}/merge-async` with `sha`, `merge_method: "squash"`, `merge_action: "direct_merge"`, and the host-generated `commit_title` and `commit_message` (5.6); never `bypass_rules`. A 202 returns a request id: journal it and poll the request status together with the pull request's merged state. Merged with the journalled head SHA is success. A 200 means already merged: confirm the merged head SHA. A `failed` status, or a 400 or 422 from the call, re-runs step 1 and is not a Deterministic failure under 8.6; twice in a row escalates with `infrastructure`. A 409 is handled as in 8.5.
   - `factory-enforced` mode: the synchronous merge endpoint with `sha` and the same title and message, so the window between check and merge is one request. A 405 (not mergeable) or 409 (the head no longer matches `sha`) re-runs step 1 at once.
4. **Finish.** If GitHub has not closed the issue within one tick (the repository's auto-close setting may be off), the factory closes it as completed. It writes the final status comment and deletes the branch.

With the default `merge.update_when_behind = "on_conflict"`, a textually clean merge can still break the default branch. This is accepted for small pull requests. After each factory merge the reconciler reads the VAL-8 checks on the merge commit on every tick until all have concluded or `ci.timeout_secs` has passed. A failing conclusion, or a check still missing at the timeout, is a red run: it pauses merges in that repository, files a `state:triage` issue linking the merged pull request, and counts as a tripwire (15.3). The pause ends when the VAL-8 checks are satisfied on the current default-branch head, or when the operator runs `sf resume`. Runs waiting in `merge` keep their approval.

- **PR-4.** The factory never merges a pull request whose head it did not push.
- **PR-5.** Auto-merge without human approval exists only at autonomy level A3 (13.6).
- **PR-6.** `native_queue` (not scheduled) would require the factory to report its checks on merge-group commits (`gh-readonly-queue/<default>/*`), copying verdicts only when every member's head SHA is one it pushed. Its permissions and polling are specified with that milestone (probe P-19).

### 12.4 Base update

A base update brings the default branch into the work branch without rewriting it.

1. The host fetches the new default-branch head into `host.git` (WS-10) and creates an update commit there with parents (previous published head, new default-branch head). If the test-merge was clean, its tree is the merge result and the commit is the new candidate. If it conflicted, its tree is the merge result including conflict markers; this commit is a work-in-progress base **W**: it is journalled, never pushed and never a candidate.
2. In the conflict case the host re-creates the workspace from W, writes the conflicting paths to the handoff, and runs an implementer round (merge-conflict round +1). The resulting candidate, and every later candidate until one is published, has W's two parents (9.4).
3. `base_sha` and `policy_sha` in the marker become the new default-branch head (16.3). Integrity and reviewer diffs use the new `base_sha`; every stage from `integrity` on uses the policy, prompts and repository instructions of the new `policy_sha`. If that policy is invalid, the base update waits as in 16.3.
4. In the clean case the workspace is re-created from the candidate (WS-6) and the run re-enters at `integrity`; in the conflict case the pipeline continues from `implement`. All later stages run on the new head, including `human_review`. A re-approval caused by a base update does not count against the human round cap.
5. If the push of an update commit is rejected because the range carries default-branch changes under `.github/workflows/` (GH-11, probe P-20), the failure is not retried. The run does an automatic reset (7.6) if one is still available, and otherwise escalates with `policy-denial`, naming the workflow files that changed on the default branch.

Whether a base-merge push dismisses approvals under GitHub's stale-review rules when the pull request diff is unchanged is probe P-12; until answered, the factory assumes it does.

---

## 13. Security

### 13.1 Threat model

Attacker-controlled input: issue titles, bodies and comments; pull request text and review comments; repository content including dependencies, build scripts and proc macros; any web page an agent fetches; every file in a workspace, including under `.git`. The design assumes a worker can be fully subverted and asks what it can then reach.

| Threat | Control |
|---|---|
| Injection steers the agent to exfiltrate secrets | The worker holds no GitHub credential (INV-1); the environment is scrubbed; egress is default-deny (13.4). The one secret a worker can hold is the model credential: none in `gateway` mode; in `subscription` mode a token that can only make model requests, and whose literal value is searched for in every candidate and every text written to GitHub (13.5). |
| Injection makes the agent write a malicious change | Integrity flags on sensitive paths, dependencies and hidden diffs; independent reviewer; human approval by default; CI without secrets. Residual risk: a plausible backdoor passing a tired reviewer. Small pull requests and the review-queue limit are the mitigation. |
| Workspace content attacks the host (hooks, nested repositories, symlinks, FIFOs, attributes, crafted names) | INV-2 and INV-3: the host does not open workspace paths; git on a workspace runs sandboxed; objects are verified at ingest; checkout, staging, export and diffs ignore in-tree attributes (9.4, WS-11, VAL-5a). |
| Agent weakens the checks that gate it | Verify and review run on a clean export (INV-11); protected and locked paths; App without Workflows permission; checks bound to the factory App; policy and instructions read from the default branch (INV-10); workflow audit (VAL-6a). |
| Agent text forges factory state on GitHub (markers, closing keywords, mentions, skip tokens) | GH-5, GH-5a; host-written commit bodies and squash messages. |
| Someone other than the operator triggers or steers work | SEC-1 for every human input; SEC-1b; SEC-2 (only the operator's issues); no issue runs without its own approval (INV-4). |
| Text edited between approval and use | 5.3 rule 6; snapshot from the same read as the check; 7.7; PLAN-2. |
| Confused deputy: the factory approves or merges its own work | INV-5; PR-2; PR-4; App never a bypass actor. |
| Malicious dependency or build script | Sandbox for all builds; dependency changes flagged; `cargo deny` in verify. |
| Agent-written code runs in CI with open network | CI has no secrets and a read-only token (VAL-6a). On a private repository its content can still leave through CI; the template's no-network build step is the control, and repositories that keep their own CI job accept the risk. |
| Runaway cost | Turn, time and usage limits enforced by the control plane; circuit breakers; kill switches (13.7). In `subscription` mode the worst case is bounded by the plan's limits, not by a bill, while usage credits are off (13.5). In `gateway` mode usage is metered by the control plane. In `direct_key` mode only the provider-side spend limit binds. |
| Data leaves through allowed channels (pull request text, branch content, the model endpoint) | The repositories are public, so the only thing of value in a sandbox is the model credential (13.5). |
| Compromised control plane | `enforced` mode: GitHub rules still require a human approval. In `factory-enforced` mode, and for A3 classes, this risk is not mitigated. |

The OWASP Top 10 for Agentic Applications is the review checklist for changes to this section.

### 13.2 Trust and triggering

- **SEC-1.** Human inputs that change factory behaviour are: `state:ready`, `factory:hold`, `factory:restart`, `factory:plan-approved`, `factory:stop`, `tier:*`, `risk:*`, `priority:*`, `type:ci` and `type:spec-delta`. Except as PLAN-6 provides, each is honoured only if the most recent `labeled` event for that exact label, with no later `unlabeled` event, has an actor of type User whose repository permission is write, maintain or admin when checked through the permission endpoint. `author_association` is not used. A tier, risk or type label that fails the check is read as its most restrictive value (`risk:high`, plan gate required, not `type:ci`). Issue types and fields are honoured only through their label fallbacks until probe P-17 shows their changes are attributable. Removal is covered too: an `unlabeled` event for a `tier:*`, `risk:*` or `type:*` label by anyone who is not an authorised actor is ignored; the value of the last authorised `labeled` event stays in force and the factory re-applies the label once. `state:triage`, `state:needs-human` and closing an issue are deliberately honoured from any human, because they can only stop work (7.7).
- **SEC-1a.** (Removed. The factory never applies `state:ready`; see PLAN-6 for labels on issues it files.)
- **SEC-1b.** Factory-set labels are never read back as authority: the journal and check runs are the source.
- **SEC-2.** The factory works only on issues written by the operator: the issue's author and every editor of its title and body must be authorised actors, or the factory App for an issue it filed. Any other issue is never dispatched, whoever labels it and at any sandbox level. To act on someone else's report, the operator restates it in an issue of their own. The editors of the body are those its edit history names, one for every edit, a revision whose text was deleted included; the issue's `editor` field names only the last of them. The editors of the title are the actors of its rename events. The factory App is recognised by the numeric id of its bot account (`databaseId` in GraphQL, `id` in REST) and not by its login, which GraphQL gives without the `[bot]` that REST appends (P-8).
- **SEC-3.** The snapshot contains the title, the body, and comments by authorised actors. Other comments are not shown to agents.
- **SEC-4.** Before use, snapshot text is sanitised: HTML comments and zero-width or bidirectional control characters are removed, and the size is capped (32 KiB body, 8 KiB per comment, 64 KiB total *(tunable)*).
- **SEC-5.** Pull requests from forks are never processed.

### 13.3 Sandbox

```rust
trait Sandbox {
    fn capabilities(&self) -> SandboxCaps;   // fs_isolation, process_isolation, egress_filtering, vm_boundary
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

| Level | Requires | Backend | Used for |
|---|---|---|---|
| `none` | — | Passthrough | The fake agent only |
| `process` | `fs_isolation`, `process_isolation`, `egress_filtering` | bubblewrap with PID, IPC, network and user namespaces and a seccomp filter | Attended `sf run` (M1); development |
| `hardened` | plus `vm_boundary` | One KVM microVM per stage execution (Firecracker or Cloud Hypervisor, decision D-5). The workspace or export is attached as a block device or virtio-fs share, and the per-run proxy is the VM's only network peer. | `sf daemon` (from M2); required for any auto-merge |

- **SEC-6.** Filesystem policy, deny by default:

  | Access | Paths |
  |---|---|
  | Read-write | The workspace (implementer) or the candidate export (verify); `exec/<seq>/home/`, except the configuration in it that AG-4 names, and temp; the stage's target directory; `quarantine/`, which also holds the capture index (capture only) |
  | Read-only | The resolved toolchain; the agent CLI; the host's `git` installation; `host.git/objects` and the host-written git configuration (capture only); the host-fetched advisory database (verify only); the role's handoff view; the workspace (planner, capture); the candidate export (reviewer); the host-written configuration in the config directory (AG-4); a backend-defined OS baseline enumerated in code and covered by a snapshot test |
  | Denied | Everything else, including the host home directory, SSH agent, container sockets, credential files, the mirror and the rest of the data directory |

- **SEC-6a.** A worker cannot see, signal, trace or message a process outside its sandbox: at level `process` through PID, IPC and user namespaces with a private `/proc`; at level `hardened` through the VM boundary. INV-1 keeps every GitHub credential out of child processes regardless. The only secret in a worker's environment is the model credential (13.5).
- **SEC-7.** The sandbox fails closed. `sf daemon` dispatches only at `hardened`; `process` is accepted for attended `sf run`; `none` only with the fake agent. If the required level cannot be provided, the run does not start.
- **SEC-8.** The sandbox wraps the whole agent process, not only its shell commands. The agent CLI's built-in sandbox covers only its Bash tool and is not relied on; AG-8 keeps it off.
- **SEC-9.** The microVM guest image is built by the host from a pinned base, contains the resolved toolchain and the agent CLI, and is mounted read-only. Its digest is journalled for every run. Repositories that need another operating system to build are out of scope.

### 13.4 Network egress

- **SEC-10.** Workers have no direct network access. The only reachable endpoint is a per-run control-plane proxy: a Unix socket bound into the namespace with an in-sandbox forwarder (bubblewrap), or a host-side interface that routes only to the proxy (microVM). The per-run endpoint is how a connection is attributed to a run (AG-19).
- **SEC-11.** The proxy allows a short hostname list: the model endpoint for the credential mode (the model gateway in `gateway` mode; in `subscription` and `direct_key` modes the hosts the agent CLI needs for model requests, fixed by the contract test: in `subscription` mode `api.anthropic.com` alone, probe P-22); the crate registry (`index.crates.io`, `static.crates.io`) or a configured mirror; plus hosts added in operator configuration. `github.com` and `api.github.com` are never on a worker's list. Every denied request is logged with the run id. Without the traffic switch in the environment of AG-11 the agent CLI also asks for a telemetry host, which is not on the list.
- **SEC-12.** Hostname filtering does not stop abuse of an allowed multi-tenant host. The allowlist is kept short for that reason, and repository policy cannot extend it. Cargo git dependencies outside the list are a blocking onboarding failure.

### 13.5 Credentials

| Credential | Held by | Notes |
|---|---|---|
| GitHub App private key | Control plane, from a 0600 file or a systemd credential | Never passed to any child process |
| Installation token | Control plane | One repository, about 1 hour. Reaches `git push` only through the built-in credential helper over an inherited pipe. A test dumps the arguments and environment of every host child during a push and finds no token. |
| Operator's own GitHub credential | Not stored | Used once by `sf repo check` to read the bypass list (DEP-4) |
| Subscription token, `subscription` mode (default) | Control plane (0600 file or systemd credential); worker environment | The operator creates it with `claude setup-token` from their own Claude Max plan, on a machine with a browser since the server is headless, and gives it to `sf init`. It is passed to agent sessions only (never to setup, capture or verify) as `CLAUDE_CODE_OAUTH_TOKEN`. Allowed for `sf run` and `sf daemon`. It is exposed to a subverted worker, so: it can only make model requests; the egress allowlist limits where it can be sent; the host searches every candidate and every text written to GitHub for its literal value and fails the stage on a hit; and the operator re-issues it after any such hit. The CLI runs non-bare (AG-3a). No dollar bill can run away while usage credits are switched off for the account: use is then bounded by the plan's limits, which the factory shares with the operator's own interactive use. A session that completes reports the account's credit state in its `rate_limit_event` (probe P-22), and BUD-8 says what the factory does with it. |
| Model API key, `gateway` mode | Control plane | Not scheduled: the operator uses a Claude Max plan only (decision D-2). It stays specified so that an API key can be added later without redesign. The worker gets the gateway address and a per-run token valid only there. The gateway replaces any authentication header the worker sends, injects the key, meters usage, and forwards only message-creation and token-counting paths. |
| Model API key, `direct_key` mode | Worker environment | Attended `sf run` at level `process` only; `sf daemon` refuses to start with it, because a stolen pay-as-you-go key has no ceiling the factory controls. Use a key with a provider-side spend limit. |

### 13.6 Autonomy levels and classes

| Level | Meaning |
|---|---|
| A0 | Lint and triage only; no code. |
| A1 | Plan only; the plan is posted for the operator. |
| A2 | The factory opens pull requests; the operator approves each one; the factory merges. Every class of work starts here. |
| A3 | A class of work merges without review. |
| Lights-out | The goal (G6): every class that a repository's tier 0 and tier 1 work falls into has earned A3. The operator still approves each issue (INV-4). |

**Classes.** Autonomy is granted per class of work, never per repository. A class is defined in operator configuration and has two parts:

- A **predicate** over the diff, deterministic and evaluated by the integrity check: an allowlist of path globs, a maximum changed-line count and allowed file modes. For dependency bumps the `Ecosystem` supplies it (for Rust: only version and checksum fields of crates already in `Cargo.lock`, the same registry source, and a minimum release age). A class defined only by labels or tier is invalid.
- The **evidence** it requires beyond the standard gates. The factory produces a mix of software, and what can be checked automatically differs by kind: for example a mutation-score floor for library and CLI code, a holdout scenario suite for service behaviour (11.7), browser-driven checks for user interfaces. A kind of work for which no such evidence is defined cannot be a class and stays at A2.

The expected ladder is: documentation; dependency bumps; test-only changes; small tasks in library and CLI code; then service code with scenario suites.

**A3 conditions**, all required for a head to merge without review: `enforced` mode with a verified bypass list; sandbox level `hardened`; risk within the class's ceiling (default `risk:low`), valid under SEC-1; the diff satisfies the class predicate; no integrity flag; all required checks and the class's evidence pass. For such a head the `human_review` stage is skipped, and 12.3 step 1 takes the class decision in place of an approval.

**Promotion** is an operator action (`sf class promote`), offered only when the class has its record: at least 30 merged pull requests at A2, a 30-day revert-or-rework rate under 5%, and the reviewer agent's verdict agreeing with the operator's on at least 95% of them *(all tunable; no published evidence supports specific numbers; decision D-14)*.

**After promotion** the factory marks 1 in 10 auto-merged pull requests of the class *(tunable)* for after-the-fact review on the dashboard issue; a problem found there counts as an escaped defect. Any tripwire in 15.3 demotes the class to A2 automatically; it must earn promotion again.

**Open design point.** How an A3 merge satisfies a ruleset that requires an approval, without the factory approving (INV-5), is not settled. Two candidate layouts are recorded as probe P-18 and the choice is made in M4. Until then 12.3 has no exception to PR-2.

Repository policy can only lower what operator configuration allows.

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

Limits are enforced by the control plane. In `gateway` mode usage is metered by the gateway. In `subscription` and `direct_key` modes usage comes from the agent CLI's own reports, and dollar figures are the API-equivalent cost the CLI reports, not money spent: they still stop runaway sessions and make runs comparable, but the binding constraint in `subscription` mode is the plan's rate limit (8.3). Agent-side caps are a second line only: they are client-side estimates and can be exceeded.

| Scope | Default *(tunable)* | Standing |
|---|---|---|
| Session: turns / wall clock | 200 / 90 min | Set by the operator for M1 (D-22) |
| Session spend: implement | $20 | Set by the operator for M1 (D-22) |
| Session spend: plan / review | $3 / $3 | Assumed (D-14); not enforced before M2 |
| Issue, cumulative across rounds and attempts | $40; each resume from a `budget` escalation grants a further $20 | Assumed (D-14); not enforced before M2 |
| Repository per day | $100 | Assumed (D-14); not enforced before M2 |
| Global per day | $150 | Assumed (D-14); not enforced before M2 |
| Factory eval runs | A separate budget; not counted against the daily limits | Assumed (D-14); not enforced before M2 |

Idle and tool-call time are the stall rules of 8.2, not budgets.

The three values of D-22 rest on a measurement of interactive sessions on the factory repository, not of factory sessions (`docs/research/15-session-sizes.md`). They keep the mark: the first measurement of what they limit is the five issues of M1b.

- **BUD-1.** Enforcement leaves 10% headroom below each limit.
- **BUD-2.** Reaching a session or issue limit stops the session and escalates with `budget`. Reaching a daily limit pauses dispatch until the next UTC day; running sessions finish their stage. Before escalation exists (M2), BUD-7 takes the place of the escalation.
- **BUD-3.** Model id, effort and a price table are per-stage configuration, loaded at runtime. Nothing about models or prices is compiled in.
- **BUD-4.** Spend is recorded per model call (input, output, cache-read and cache-write tokens, and dollars) and rolled up per stage, run, issue and merged pull request. Cumulative spend appears in the status comment and the pull request body. Where usage comes from the agent CLI, the token counts of a model call are those of its `message_delta` event, which `--include-partial-messages` adds to the stream (AG-1): the counts in `assistant` events hold a placeholder for output tokens (probe P-22, observed in `subscription` mode). The CLI reports dollars only in `result`, per model and as a running total for the session. The dollars recorded for a session are those of its last `result`, never a sum over results; between results the supervisor's running figure is computed from the calls' token counts and the price table of BUD-3. A subagent's calls are recorded as BUD-10 says.
- **BUD-5.** The primary cost metric is dollars per merged pull request, counting failed and abandoned runs. Token volume and pull request count are not treated as progress.
- **BUD-6.** What a session limit counts. The supervisor counts every turn the session's stream shows, a subagent's included, from the session's start to its end, however many messages the session is given. Wall clock runs from the session's start to its end. Spend is in API-equivalent dollars as the agent CLI counts them, the one unit of spend in M1. The `implement` value is set against the list prices of the model the operator runs that stage on (D-22). The CLI's own `--max-turns` (AG-14) stays the second line: by its documentation the CLI starts that count over with each message of a stream-JSON input, while its budget total runs on over the session. Neither that, nor a subagent's calls in the stream, was observed (probe P-26).
- **BUD-7.** Through M1 the control plane enforces the session limits and no other row of the table: the turns, the wall clock and the `implement` spend. The other rows bound rounds, escalations and unattended dispatch, which arrive with M2; spend is rolled up per issue from M1a all the same (BUD-4). A session limit that is reached is a Budget failure (8.6), the wall clock like the turns and the spend. `sf run` then interrupts the session, records which limit it was, reports it and exits with a failure. The report goes to the terminal, and from M1b into the status comment as well. Nothing of that session is captured or published. Run again on the same issue, `sf run` resumes from the journal (CLI-3) and restarts the stage with a fresh session and the whole of its session limits: in M1 the operator's command stands in for the resume and the budget grant of 7.5.1. A session stopped at a limit returns no result, and so no notes for `progress.md`: the host writes into the handoff which limit stopped it, as it writes the failure summary after an agent failure (8.6), so that the fresh session knows it continues work that was cut short. Such a restart is the operator's and does not count toward Poison (8.6); the journal records it.
- **BUD-8.** Usage credits (13.5). A session whose `rate_limit_event` shows usage credits in use (`isUsingOverage: true`, or `rateLimitType: "overage"`) is stopped at once, as a session that reached a limit: its dollars are then money. The one state observed is that of an account with credits off, `overageStatus: "rejected"` (probe P-22). In M1 any event that shows something else, an event without that field included, makes `sf run` print a warning and nothing more: the run is attended, and a session can then cost little more than its spend limit (BUD-10). What the daemon does while credits are switched on is decided with M2 and may be stricter (D-14); the stop holds for it as well. Which fields show credits switched on, and what a session that draws on them emits, was not observed, and the operator does not switch credits on to observe it: the stop rests on the documented fields (probe P-26).
- **BUD-9.** M1 sets no limit in units of the plan. The control plane records, from the session's `UsageBatch` (ORCH-4), the window, the utilisation, the reset time and the credit state of every `rate_limit_event`, so that the share of the plan's window a run costs can be measured before such a limit is set. A session that meets the plan's limit in M1 ends as under BUD-7, with no pause and no restart by the factory: the pause of 8.3 is M2. `sf run` keeps the raw stream lines of the rejection, which no probe observed (P-22), and prints the reset time.
- **BUD-10.** Subagents (AG-20). By the CLI's documentation the stream gives a call's real token counts for the main loop only: a subagent's calls appear as lines whose output count is a placeholder, while the session's totals in `result` and the CLI's own budget cap include them. None of this was observed (probe P-26). Until it is, three things hold. The per-call records of BUD-4 cover the main loop, and what a session's subagents spent is recorded as one remainder: the session's total less its main-loop calls. Every subagent call the stream shows counts as a turn (BUD-6). And because the supervisor's running figure can fall short while subagents run, it is the CLI's own cap (AG-14) that stops a session for what they spend: at the limit and not at the threshold of BUD-1, and by the documentation with an overshoot of one response and of whatever subagents are still running.

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

Evaluated daily per repository. Any of these demotes the affected class to A2 (13.6) and opens a dashboard notice: 30-day revert-or-rework rate above 10%; an escaped defect in an auto-merged change; a red default-branch CI run after a factory merge (which also pauses merges, 12.3); any secret-scan hit in a published commit; three budget stops in a day *(all tunable)*.

### 15.4 Feedback into the harness

Every escalation, reset, human "changes requested" and escaped defect is classified (`spec-gap`, `missing-context`, `missing-guardrail`, `agent-error`, `flake`, `infrastructure`). `sf metrics --failures` reports classes over time. For classes other than `agent-error`, the factory files a `state:triage` issue proposing the durable fix: a spec change, an `AGENTS.md` or docs change, a new lint or structural test, or a flake quarantine.

### 15.5 Dashboard issue

Each repository has one pinned issue, maintained by the factory and edited in place: active runs, queue, items waiting on a human with their age, yesterday's spend, breaker, mode and sandbox-level status. It is the only place the factory nudges.

---

## 16. Configuration

### 16.1 Layers

| Layer | Location | Trust | Holds |
|---|---|---|---|
| Operator | `~/.config/sf/config.toml` | Trusted | Credentials and their mode, data directory and disk limits, repository allowlist, classes and their autonomy (13.6), dispatch schedule, required CI checks and acknowledged workflows per repository, egress allowlist, global budgets and concurrency, agent CLI path and version range |
| Repository policy | `.factory/policy.toml` and `.factory/prompts/*.md` at the default-branch head | Reviewed, but changeable by pull request | Stages enabled, verify and setup commands, per-stage model and limits, round caps, size budget, extra protected paths and required checks, instruction directories, generated-file globs, merge settings |
| Issue | Labels | Honoured only under SEC-1 | Tier, risk, priority, type, hold, restart |

- **CFG-1.** Repository policy can only tighten operator settings: lower budgets and concurrency, lower autonomy, add protected paths and required checks, set `merge.update_when_behind = "always"`, and disable `plan` (for tier 0, and for tier 1 issues where the plan gate of 7.4 does not apply), `author_tests`, and `review` for tier 0. It cannot lower the sandbox level, extend the egress allowlist, shorten built-in protected paths or the required-check list, disable `capture`, `integrity`, `verify` or human review, or name credentials.
- **CFG-2.** Files are TOML, parsed with unknown fields rejected, and each has a `schema_version`. A JSON Schema is generated for editor support.
- **CFG-3.** A change to `.factory/**` is itself a protected-path change and is never auto-merged.
- **CFG-4.** No home directory is built into the code or into the systemd unit (AG-17). `~` in the table above is the home directory of the user the process runs as, wherever that is. The factory reads and writes no other path under a home directory unless the operator configuration names it.

### 16.2 Example policy

```toml
schema_version = 1
ecosystem = "rust"

[stages]
plan = "auto"            # auto = by tier and risk; false never removes the plan gate
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
| Inspect | `sf cost [--since]` · `sf metrics [--failures]` · `sf policy check|explain` · `sf issue lint <n>` · `sf class list|promote|demote` |
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
| Target directories, exports | Removed when the run enters `human_review` or a terminal state |
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
| Git | Real `git` in temp directories with the hostile-tree suite of WS-9; a round after a base update never reverts upstream content; a base update where the default branch added a tracked file matching an ignore rule keeps that file; a candidate built after a gate failure on an unpublished update commit has the new default-branch head as an ancestor |
| GitHub | `fake-github`: a stateful HTTP fake implementing the REST and GraphQL subset the gateway uses, hosted by the test process so it survives a killed daemon; pushes go to a local bare repository. `wiremock` for adapter edge cases including rate-limit and ETag paths. |
| Sandbox | Per backend: read the host home, a credential file, or the arguments and environment of a host process; see or signal a host process; reach a non-allowlisted host; write outside the allowed paths; escape through a setuid binary or a mount. During development they also run in the development VM (DEV-3). The tests at level `process` also run in CI; those at level `hardened` do not (DEV-7). |
| End to end | With the fake agent against `fake-github` on every pull request. Against a scratch GitHub repository only in a scheduled workflow on the default branch, behind a protected environment, with a separate test App installed on the scratch repository only. |
| Contract | A scheduled test against the installed agent CLI that asserts the flags and message shapes the driver relies on; the lock-down of AG-3a, with the cases of probe P-21 as its fixture: nothing planted in the working directory takes effect, whether it is there at the start, picked up from a subdirectory or written mid-session; that a worker cannot change the configuration of AG-4; and that a session completes with only the model host of SEC-11 reachable and nothing in the proxy's denied-request log |

### 19.5 Checks and CI for the factory repository

- **DEV-1.** One command, `cargo xtask check`, runs every check of the factory repository. An agent session runs it before each commit, the pre-commit hook runs it again (DEV-9), and CI runs the same command, so the local set and the CI set cannot drift apart. The command was first written on 2026-11-21 (D-23), with formatting, clippy and `cargo test`, which stands in for the test runs of DEV-2 until those are built. Every other check of DEV-2, and the run in the development VM (DEV-3), is added to this command, and nowhere else, when it is built.
- **DEV-2.** The set: `cargo fmt --check`; `cargo clippy -D warnings`; `cargo nextest run --locked`; `cargo test --doc`; `cargo doc` with warnings denied, broken intra-doc links included; `cargo deny check`; the unused-dependency check; the structural test; `typos` over code and documents; a cross-reference check that every requirement id and section number cited in `SPEC.md`, `GLOSSARY.md`, `AGENTS.md` and the code resolves; a check of local Markdown links; `cargo mutants --in-diff` on the changed code, failing on a surviving mutant; coverage with `cargo llvm-cov`, reported without a floor until there is code to measure. If commits become too slow, mutation testing moves to pre-push; no check is dropped.
- **DEV-3.** On macOS, `cargo xtask check` also runs clippy and the tests inside the development VM, on every run, so Linux-only code is checked whatever a change touches. The VM is Lima running the latest Debian stable on the Mac's architecture, with `vmType: vz` and nested virtualisation so that `/dev/kvm` is available. Its configuration is committed, and `cargo xtask` creates and starts it. x86-64 coverage comes from CI.
- **DEV-4.** Development builds of `sf` run in the VM (BOOT-2). It holds the operator's Claude Code login (D-2), which shares quota with the operator's interactive sessions, and no GitHub credential beyond the one `sf` needs for the scratch repository.
- **DEV-5.** The CI workflow follows VAL-6, except that a network namespace takes the place of the container (DEV-6). It has one job per architecture, each running `cargo xtask check` (DEV-1) on a GitHub-hosted runner: `check (x86-64)` on `ubuntu-24.04` and `check (arm64)` on `ubuntu-24.04-arm` (D-20). The labels are pinned, and `ubuntu-latest` is never used, because the setup that bubblewrap needs differs between Ubuntu releases (P-16). The workflow has no path filter, so both checks are reported on every pull request. There is no macOS job. The repository has no self-hosted runner: one would run pull-request code on the operator's own machine.
- **DEV-6.** Each job installs `bubblewrap`, lifts Ubuntu's AppArmor restriction on unprivileged user namespaces (`kernel.apparmor_restrict_unprivileged_userns=0`), and fetches the toolchain, the check tools, the dependencies and the advisory database. It then runs the check in a network namespace that holds only a loopback interface. That is the build without network access of VAL-6, without its container: under Docker's default confinement bubblewrap cannot create its namespaces (P-16). Because the job has no network, a sandbox test of 19.4 uses a listener on the loopback interface as the host that must not be reached; against an internet host it would pass whatever the sandbox did.
- **DEV-7.** Where each check runs:

  | Checks | Hosted runners | Development VM | Server |
  |---|---|---|---|
  | The DEV-2 set, with every test of 19.4 that needs neither KVM nor the Claude Code login, the sandbox tests at level `process` included | On every event of VAL-6, on both architectures | Clippy and the tests, on every run of the check (DEV-3) | Run by the operator on a tagged commit before that build is installed |
  | Sandbox tests at level `hardened` (from M2) | Never | With the other tests (DEV-3) | On a schedule |
  | Tests that need the operator's Claude Code login: the contract test of 19.4 and any real agent turn | Never: the workflow holds no secret (VAL-6) | When a session runs them (DEV-4) | The contract test, on a schedule (19.4) |

  The x86-64 hosted runners have `/dev/kvm` and the arm64 runners do not, and GitHub does not support nested virtual machines on either (P-16). The workflow does not open the device to the runner's user, and a test that was not run is never reported as passed. Nothing that runs on the server reports a GitHub check. The scheduled end-to-end run against the scratch repository (19.4) is a workflow of its own and not a required check.
- **DEV-8.** Once the workflow is on `main`, the operator creates a ruleset for `main`: a pull request is required, with zero approvals; the two checks of DEV-5 are required, with GitHub Actions as their source; force pushes and deletions are blocked; there is no bypass actor. Approvals are zero because the operator writes every pull request merged in M0 and M1a and cannot approve their own (20.3). A change that renames a job changes the ruleset with it. This ruleset is weaker than the one of 20.3, so in M1a, where the factory merges nothing, `sf` detects `factory-enforced` (4.1). In M1b, once the factory reports its check runs (5.5), the operator replaces it with the ruleset of 20.3, and the two checks of DEV-5 are the repository's `required_checks` (VAL-8).
- **DEV-9.** The pre-commit hook is the file `.githooks/pre-commit`, committed in the factory repository and named by `core.hooksPath`; the repository uses no hook manager (D-23). The hook runs `cargo xtask check` (DEV-1) and names no check itself. Before that it unsets the variables through which git hands a hook the repository and the index of the commit being made, the list of `git rev-parse --local-env-vars`, so that a test of 19.4 that runs git in a directory of its own works on its own repository (P-27). `core.hooksPath` is a setting of the clone and is not committed. A `SessionStart` hook in `.claude/settings.json` sets it to `.githooks` at the start of every Claude Code session on the repository, and a clone used in any other way is given it by hand. The value is relative, so it also holds in a linked worktree. The hook checks the work tree as it stands, which is the commit when every change is staged; CI checks the commit itself. There is no pre-push hook. If mutation testing moves there (DEV-2), it is a second file in `.githooks/` that runs a task of `cargo xtask`. The hooks cover the interactive sessions of D-16 only: the control plane runs no hook from a workspace (INV-2), a worker reads no project settings (AG-3a), and nothing on a clone stops a human from skipping a hook. What holds for every commit that reaches `main` is the ruleset of DEV-8.
- **DEV-10.** A Claude Code session on the factory repository is refused a command that makes git skip its hooks (D-23). `.claude/settings.json` holds a `PreToolUse` hook on the Bash tool, and its command hands the event to `cargo xtask pre-tool-use`. The task reads the command line as written, splits it into its commands and their words with the shell's quoting undone, and takes each word `git`, with or without a path, as the start of a git command. It refuses, with exit code 2 and a reason, a `git commit` on which `--no-verify` is in force, written out, abbreviated or as `-n`; a `git push` on which it is in force; and a git command given its own `core.hooksPath` through `-c` or `--config-env`. Which spellings git accepts, and which words are the value of another option, is as probe P-27 observed. The push is refused although there is no pre-push hook, so that adding one (DEV-9) changes nothing here. Two properties of the hook's command rest on P-27. It starts the task only for an event that holds the letters `git`, so that most calls neither wait for a build nor are refused while the task does not build; the `if` field of Claude Code is not used for this, because it let wrapped git commands through. And it turns every exit code of the task but 0 into 2, because Claude Code lets a call go on after any other failure of a hook; the task likewise refuses an event in which it finds no command. A deny rule was not chosen: it is a pattern over the text of the command and cannot tell `-nm` from `-m "naming"`. The refusal guards against a habit, within the limits DEV-9 states for the hooks. The task expands nothing and runs nothing. It does not see a flag or a `git` that comes out of a variable, a substitution, an alias or a function; a command in a string or a file handed to an interpreter, or in the text of a here-document; a `git` whose letters quoting breaks up; the `GIT_CONFIG_*` variables, an included configuration file or a change to the clone's `core.hooksPath`. It refuses the rare command that has the words `git commit -n` as arguments of another program. A session of another coding agent, or one started with the project's settings switched off, has no such hook and only the rule in `AGENTS.md`.

---

## 20. Onboarding a repository

### 20.1 `sf repo add`

1. Check that the App is installed; read the capability probe and enforcement mode.
2. Produce a readiness report. **Blocking:** a single-command setup and test entry point that passes on the default branch under the sandbox; an `AGENTS.md`; the toolchain of WS-7 installed; no submodules, no `filter=` attributes and a clean round-trip checkout at the default-branch head; no branch named `factory`, which would make every work branch impossible to create (8.2); no Cargo git dependency outside the egress list; `cargo deny`, if used in verify, passes without fetching against the host-fetched advisory database; the workflow audit (VAL-6a). **Advisory:** check speed, flake rate, documentation layout, architecture rules, the repository's auto-close setting, other installed Apps that can write pull requests, and past approving reviews by bot accounts.
3. Open one onboarding pull request containing `.factory/policy.toml`, default prompts, the issue forms, a `CODEOWNERS` entry for protected paths and an `AGENTS.md` skeleton if missing. The CI workflow template is not pushed by the App (GH-11): `sf repo add` writes it to a local file and prints the commit command, and the operator adds it to the onboarding branch with their own credentials.
4. Do nothing else in that repository until the pull request is merged and a workflow satisfying VAL-6a exists on the default branch. Closing the pull request opts the repository out.
5. After the merge: create the label set through the API and write the workflow's check names into `required_checks`. Only then is the ruleset of 20.3 activated, by the operator: the App has no Administration permission (GH-11) and `sf` holds no credential that can write rulesets. `sf repo add` prints the ruleset as importable JSON; `sf repo check` then re-detects the mode (4.1) and verifies the bypass list (DEP-4).

Until M5 the same steps are done by hand from this checklist.

### 20.2 Modes after onboarding

A repository advances by operator command: `observe` (A0: readiness lint and plans only) → `propose` (A2) → named A3 classes.

### 20.3 Recommended ruleset

For the default branch, in `enforced` mode: require a pull request; one approval; dismiss stale approvals on new commits; require approval of the most recent push by someone other than the pusher; require code-owner review; require the status checks `factory/integrity`, `factory/verify`, `factory/review` (source: the factory App) and the CI checks of VAL-8; do not require branches to be up to date; block force pushes and deletions. Signed commits are not required in v1 (5.6).

Required checks apply to every pull request, and the factory checks exist only on factory pull requests. The repository-admin role is therefore a bypass actor in pull-request-only mode: without it the operator could not merge a pull request they wrote themselves, since an author cannot approve their own pull request. Human pull requests are merged through that bypass and are outside the factory's guarantee. The factory App is never a bypass actor. The mode detection of 4.1 accepts exactly this bypass configuration.

### 20.4 Repository conventions the factory expects

- A root `AGENTS.md` of about 100 lines that maps into `docs/`; details live in versioned documents, not in the instruction file.
- Fast, deterministic checks runnable with one command.
- Architecture rules enforced mechanically (crate boundaries, lints with remediation text in their messages).

---

## 21. Bootstrapping and evaluating the factory

### 21.1 Self-hosting

- **BOOT-1.** Humans (with interactive agent assistance) build M0 and M1 (D-16). Claude Code sessions on the operator's Mac write the code, one issue, branch and pull request at a time, and the operator reviews and merges every pull request. That work is tracked as issues on the factory repository, grouped under one GitHub milestone per roadmap milestone; the operator never makes them `state:ready`, so the factory never claims them. From M1 the factory is run attended on its own repository from an installed, tagged build; from M2 unattended.
- **BOOT-2.** The binary that operates on the factory repository is always an installed, pinned release, never a build of the tree it is changing. A development build runs only against the scratch repository (`sapientpants/sf-scratch`), never against the factory repository. The factory version is recorded in every marker.
- **BOOT-3.** From M2, upgrading the operating binary is a human action (`sf self-update`), allowed only after the eval gate passes on the candidate: the smoke eval until M3, the eval suite afterwards. The first tag, `v0.1.0`, is cut when the rest of M0 passes in CI and in the development VM; the operator installs it on the server to run M0's exit check (22). During M1 the operator installs a newer tagged build by hand; the gate is the harness tests of 19.4 passing on the tagged commit. Previous binaries and store backups are kept for rollback (18.1).
- **BOOT-4.** The factory's own gate code, prompts, policy and this specification are protected paths in its repository and are never auto-merged. The built-in list of 11.3 covers neither `SPEC.md`, nor `GLOSSARY.md`, which holds the specification's terms (D-21), nor the research records behind the specification, so the factory repository's policy MUST list `SPEC.md`, `GLOSSARY.md` and `docs/research/**` in `[paths] protected_extra` (16.2).
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
| **M0 Foundations** | Workspace, the check command with its pre-commit hook (DEV-1, DEV-9, DEV-10) and the development VM (DEV-3), CI with its ruleset (DEV-5, DEV-8) and the first tagged release build (BOOT-3); the Linux server provisioned (4.4); `factory-core` types, the `Ecosystem` interface with its `rust` implementation, and the engine skeleton with property tests; `factory-proc` with the supervisor shim; git wrapper; GitHub App creation; `sf doctor`; the probe suite (23.2); prototypes of capture, ingest and export against the hostile-tree suite (WS-9), of the bubblewrap sandbox, and of the egress proxy | All M0 probes answered and recorded; the hostile-tree suite passes; a cold build of the factory repository and one real agent turn succeed on the server under bubblewrap with only the proxy reachable |
| **M1a Skeleton** | Store with `runs` and `events`; claim; implement at sandbox level `process` with the egress proxy (fake agent, then the Claude driver in `subscription` mode); per-session limits (BUD-6 to BUD-10); capture; integrity with path, structural and secret rules, including the literal-token scan; verify on the export; publish a draft pull request; then exit. `sf run` on the same issue resumes from the journal. | One real issue reaches a draft pull request and survives an interrupted run |
| **M1b Attended run** | Status comment and marker; factory check runs; CI wait; ready and evidence manifest; observation of human review and merge (strategy `human`) | Five real issues on the factory repository merged by the operator from factory pull requests, using a tagged, installed build |
| **M2 Daemon** | The microVM backend, with the daemon dispatching only at `hardened`; systemd unit; poll and reconcile loop; outbox, activities and recovery; scheduler and limits for two repositories onboarded by hand; SEC-1 for all human inputs; outbound encoder; workflow audit; lock-down contract test and plan-limit pause; escalation and resume; plan stage and gate; reviewer stage; in-place rounds; base update; reset; `factory_serial` merge; follow-up issue filing; dashboard issue; control API; kill switches; `sf rebuild`; `sf self-update` with rollback; smoke eval | 48 hours unattended on a second repository with no manual repair; the process-crash test passes at every stage boundary; sandbox escape tests pass at `hardened`; a planted copy of the subscription token is caught before publish; self-hosting on a pinned release begins |
| **M3 Validation depth** | Issue forms and readiness lint enforced; tiers, spec deltas and tier 2 decomposition; `author_tests` with locked paths; tampering heuristics; mutation testing in CI; failure classification; reviewer agreement tracking; eval suite v1 | The eval suite gates a release; reviewer agreement with the operator is measured |
| **M4 Earned auto-merge** | Classes with predicates and evidence; promotion and automatic demotion; the ruleset layout for A3 (P-18); sampled after-the-fact review; tripwires; hash-chained audit digest in pull requests; first classes: documentation and dependency bumps | One class promoted on the factory repository on its real record; a planted defect demotes it in a test |
| **M5 Operations** | `sf repo add` onboarding automation; more than two repositories; metrics; retention and `sf gc`; `sf export`; OpenTelemetry export | A third repository onboarded through the onboarding pull request plus the operator-committed workflow file |
| **M6 Towards lights-out** | Classes for test-only changes and for small tasks in library and CLI code, backed by mutation-score floors; the holdout scenario runner and classes for service code; a second ecosystem | On one repository, at least half of the pull requests merged over 30 days *(tunable)* needed no review, with no tripwire fired |

Not scheduled: API-key credential modes and the model gateway; a second agent driver; the native merge queue; `createCommitOnBranch` publishing; webhooks; work on issues not written by the operator.

---

## 23. Open items

### 23.1 Decisions

Decided by the operator on 2026-11-18 (D-18 and D-20 on 2026-11-19; the rule of D-16 about the server confirmed on 2026-11-20; D-21, D-22 and D-23 on 2026-11-21):

| # | Decision |
|---|---|
| D-1 | The factory's repository is public on the operator's personal account, so GitHub enforces the ruleset (`enforced` mode). |
| D-2 | The model credential is the operator's own Claude Max subscription. There is no API-key fallback: when plan limits are reached the factory pauses. |
| D-3 | The host is a Linux server on the operator's own hardware with KVM. macOS is not a supported host. |
| D-7 | The daemon always runs at sandbox level `hardened`. (An earlier draft accepted `process` for the daemon; with KVM available that is no longer needed.) |
| D-10 | Target repositories are Rust first; language-specific parts sit behind the `Ecosystem` interface so others can follow (9.5). |
| D-11 | The goal is lights-out operation, earned per class of work (13.6). The operator approves every issue, including each sub-issue of a feature. |
| D-12 | The factory stops at merge. Releases and deployments stay human actions. |
| D-13 | The factory works only on issues the operator wrote (SEC-2). |
| D-16 | M0 and M1 are built by Claude Code sessions on the operator's Mac, one pull request per issue, each merged by the operator (BOOT-1). Every step on the server is the operator's to run; a session never connects to it by itself. The server was reached only from the operator's home when this was decided. The operator now also reaches it over a VPN (4.4), and on 2026-11-20 confirmed that the rule stays. Linux-only code is developed in a local VM (DEV-3). Every check runs before each commit through one command (DEV-1). |
| D-17 | The factory's repository, code and documents alike, is licensed `MIT OR Apache-2.0`, copyright Marc Tremblay. Every crate inherits the licence from the workspace, and the structural test (19.1) checks it. Source files carry no per-file licence header. |
| D-18 | `SPEC.md` stays one file through M0 and M1 and keeps its ids (`INV-`, `GH-`, `SEC-` and so on); it is not split into the living-spec layout of 6.3. Issues in that period cite those ids, with `SPEC.md` as the spec file path. The spec is protected through the repository's policy (BOOT-4). |
| D-20 | CI for the factory repository runs on GitHub-hosted runners only, with one job for x86-64 Linux and one for arm64 Linux; there is no macOS job and no self-hosted runner. Nothing that needs KVM or the Claude Code login runs in CI. Once the workflow exists, a ruleset on `main` requires both jobs and has no bypass actor; the ruleset of 20.3 replaces it at M1b (DEV-5 to DEV-8). |
| D-21 | The conventions of the factory repository (20.4). `GLOSSARY.md` at its root is the one home of the terms; section 2 points at it. The file is protected with the spec (BOOT-4) and its citations are checked (DEV-2). `AGENTS.md` is a map: it states a rule itself only where a session that read nothing else would do harm without it. Through M1a it is written for the interactive sessions of D-16; what a worker receives of it (AG-2a) is decided when the repository's policy is written. None of the labels of 5.2 exists on the factory repository before its onboarding: they are created at step 5 of 20.1, on the operator's instruction, and a session creates none of them before that. Their absence guards nothing by itself: the REST API creates a label that is added by name (P-25). The repository has no `docs/adr/` through M1; this table is its decision record. |
| D-22 | The limits of M1 (section 14). M1 enforces the session limits only, which are the turns, the wall clock and the `implement` spend, and records the rest (BUD-7). Spend stays in API-equivalent dollars as the agent CLI counts them (BUD-6). The values are 200 turns, 90 minutes and $20 for an `implement` session. The dollars are at the list prices of `claude-opus-5-5`, the model the operator runs the factory's implementation on. The values rest on a measurement of the operator's own interactive sessions, most of them on `claude-fable-5-2`, the model the operator writes the specification with, and priced again for the other model (`docs/research/15-session-sizes.md`). They stay tunable. Reaching a limit ends `sf run` with a report, and running it again is the operator's grant (BUD-7). A session that reports usage credits in use is stopped, and credits that are switched on but unused draw a warning (BUD-8). No limit is set in units of the plan; the utilisation is recorded (BUD-9). Decided the same day, after the first answer: the $20 stands; an agent session may use subagents, whose calls count as turns and whose spend the CLI's own cap bounds until probe P-26 shows what the stream gives (AG-20, BUD-10); a restart by `sf run` after a limit tells the fresh session which limit was reached and does not count toward Poison (BUD-7); and the warning is drawn by any event that does not show credits refused, since the operator does not switch credits on to observe another state (BUD-8). What the daemon does while usage credits are switched on, and every other limit of section 14, waits for M2 (D-14). |
| D-23 | The pre-commit hook of the factory repository, and how a Claude Code session is kept from skipping it. The hook is a committed file named by `core.hooksPath`, with no hook manager, and it runs `cargo xtask check` and nothing else (DEV-9). A `PreToolUse` hook that runs a task of `xtask` refuses the session a commit or a push that skips git's hooks, in place of a deny rule in `.claude/settings.json` (DEV-10). `cargo xtask check` is written now, with the checks that exist, and is not left to the rest of M0 (DEV-1). |

Still open:

| # | Decision | Assumed until decided |
|---|---|---|
| D-4 | Which second repository does the factory run on from M2? | To be named |
| D-5 | The microVM monitor: Firecracker or Cloud Hypervisor. | Decided by the M2 prototype (boot time, workspace attachment, guest image build) |
| D-6 | A reviewer from a different model family needs a second subscription and driver. | A different Claude model from the implementer |
| D-8 | Eval trials and budget required per promotion type (binary, prompts, policy defaults, model). | Regression set at one trial for a binary; full suite at three trials otherwise |
| D-9 | `merge.update_when_behind`: `on_conflict` keeps the approval on the reviewed commit but can let a semantic conflict reach the default branch; `always` re-validates every time at the cost of re-approval. | `on_conflict` |
| D-14 | The class ladder, promotion thresholds and audit sampling rate of 13.6, and the limit values of section 14 that D-22 did not set: the `plan` and `review` session spends, the issue limit and its grant, the daily limits and the eval budget. What the daemon does while usage credits are switched on but not in use (BUD-8). | The listed defaults; for usage credits, the warning of BUD-8 |
| D-15 | A dispatch schedule that leaves plan capacity for the operator's own interactive use. | None: the factory may run at any hour |
| D-19 | The layout of the factory's own specification from M2: one file, or the living-spec layout of 6.3 with either the existing id prefixes or ids renamed to `R-<CAP>-<NNN>`. | As D-18, until decided when M3, which builds spec deltas, is scoped |

### 23.2 Probes

Facts the design depends on that research could not confirm from primary sources. `sf doctor --probe` answers each against a scratch repository, except P-4 (WI-1b), and the answer is recorded in `docs/research/probes.md` and, once the store exists (M1a), imported into its `probes` table. All are M0 unless marked. An answered probe keeps its row, which then also states the answer.

| # | Question | Design impact |
|---|---|---|
| P-1 | What does creating an existing ref return (409 or 422, and the message)? Answered on 2026-11-20 by calls on the scratch repository, with the operator's token and not an installation token: a 422 with the message `Reference already exists`, whatever SHA the call asks for, under API versions `2026-03-10` and `2022-11-28`. Every fault in the ref name or the object that was tried got a 422 as well, told apart by its message alone, and a branch name that collides with the ref gets the same message as an object that is not a commit. No 409 was seen. Of two creates sent at once, one succeeded and the other got this 422, in each of 40 pairs; neither the answer nor the ref says who created it. | 8.2, 8.5, 20.1 |
| P-2 | Does an approving review from a GitHub App, or from `github-actions[bot]`, count toward required approvals under a ruleset? | Onboarding warning; PR-2 already ignores them |
| P-3 | Can an App bot be an issue assignee? | Optional "assigned to the factory" signal |
| P-4 | How does an issue form render into the body? Answered on 2026-11-20 by four issues that the operator created on the scratch repository through the form of 6.2, read back through REST and GraphQL: one block per field in the form's order, each the line `### <label>`, a blank line and the field's text as submitted, with a blank line between blocks. An empty field holds `_No response_`; a field of spaces and line breaks holds those. Nothing is escaped, so a heading typed into a field cannot be told from the form's own, and a carriage return in a field stays in the body. A sketch of the parser of WI-1 read back every field of the two bodies made from ordinary text; in the third, written to look like the form's output, two names repeat (WI-1a); in the fourth, a fence left open in one field takes the next field's heading with it, on GitHub's page as in the sketch. Two lines typed by hand arrived with a line feed between them. | 6.2, WI-1 to WI-1b, 6.4 |
| P-5 | Is a push that changes `.github/workflows/` rejected for an App without the Workflows permission on every push path? | Defence in depth for 11.3 |
| P-6 | Does the agent CLI's inner Bash sandbox work inside the outer bubblewrap sandbox and inside the microVM, or must it be disabled? Answered on 2026-11-20 for bubblewrap, on the server as the factory's user, with a stand-in for the factory's sandbox and Claude Code 2.1.311. It works where the stand-in lets a process create a user namespace, once `socat` is installed: the CLI's sandbox nested and confined the command further. Under `--disable-userns` each command failed, or ran unsandboxed when the model asked for that. Without `socat` the CLI warned on stderr and ran every command unsandboxed. It does not have to be disabled: it is off unless a settings file turns it on, and with it off the Bash tool ran in the CLI's own namespaces. `sandbox.enabled: false` in the host's `--settings` file is the setting, and it turned the sandbox off when the config directory had turned it on. The microVM was not tried (M2), and AG-8 does not wait for it. | AG-8 |
| P-7 | Does sandboxed staging with a quarantine object directory and host-side ingest behave as specified on the hostile-tree suite, and what is the exact ignore-rule mechanism? Answered on 2026-11-19 by a prototype of the git commands on git 2.47.3 and 2.56.1, without the factory's sandbox: it does, once 9.4 names the commands and their order. The ignore rules are the workspace's `.gitignore` files at capture, read by `git ls-files --others --exclude-standard`. In-tree attributes have to be switched off at checkout, staging and export alike. | 9.4, WS-1, WS-9, WS-11 |
| P-8 | Who is reported as the editor of an issue title and body through the API? Is `lastEditedAt` set for API edits, and does a title change always produce a rename event? Answered on 2026-11-20 by edits of six issues on the scratch repository, through REST, GraphQL and the web interface, by the operator and by a workflow as `github-actions[bot]`; the factory's own App does not exist yet. The three paths report alike. A body edit sets `lastEditedAt` and `editor` and adds an entry to the edit history, which names the editor of every edit; `editor` names the last one only. A title edit moves none of these and produces a rename event that names its actor: one for each of 32 changes, those of case or of trailing spaces alone included. Sending or saving a value the issue already has leaves no trace. Every time is a whole second, and a body edit sent just before a `labeled` event, or at once with it, can carry that event's time. An App is a `Bot` with the login `github-actions` in GraphQL and `github-actions[bot]` in REST, under one numeric id. | 5.3 rule 6, 7.5, 7.7, SEC-2 |
| P-9 | Do blocked-by dependencies work across repositories, and what is the per-issue limit? | 5.3; cross-repository work is out of scope for v1 regardless |
| P-10 | Do comment edits count toward the content-creation rate limit? | GH-16 budget |
| P-11 | Does `merge-async` behave as documented on a personal-account repository, including `merge_action` and the custom commit message? | 12.3 |
| P-12 | Does a base-merge commit pushed by the App dismiss approvals under "dismiss stale approvals" or "require approval of the most recent push" when the pull request diff is unchanged? | 12.4; decision D-9 |
| P-13 | Cold versus seeded build time and size for the factory repository. | 9.3 |
| P-14 | Is `current_user_can_bypass` populated for installation tokens on the ruleset objects that contribute to the default branch's rules? | DEP-4 |
| P-15 | Which encodings suppress mentions, issue references and closing keywords in comments, pull request bodies and commit messages? Answered on 2026-11-21 by text written to the scratch repository in seven encodings, by the operator and by a workflow as `github-actions[bot]`, and read back as rendered HTML, timeline events and the operator's notifications; the factory's own App does not exist yet. In a comment, an issue body and a pull request body a backslash escape and a character reference suppress nothing: the mention notified, the references were recorded and the pull request closed its issue. A fenced block and a code span suppress all three in a comment, an issue body and a pull request body, as long as the text does not end the fence. A fence suppresses nothing in a commit message, where the commit closed its issue. A zero-width joiner or space after the `@`, the `#` or the hyphen of `GH-` suppresses all three on every surface. An issue URL is matched by its path, `owner/repo/issues/<n>`, with or without a scheme and a host, and a zero-width space inside the path stops it. A closing keyword never acted without a live reference directly after it. Text set by an edit acts as text written new. A mention and a `#<n>` in an issue title did nothing. In a commit message `&#<n>;` is a reference to issue `<n>`. No notification from a commit message was seen, and a push with the workflow's token left no reference events. | GH-5a, PLAN-5, 9.4 |
| P-16 | Can GitHub-hosted Linux runners run the sandbox tests (unprivileged user namespaces for bubblewrap, `/dev/kvm` for the microVM)? Answered on 2026-11-19 with stand-in commands, not the factory's sandbox: runners of both architectures run bubblewrap once a setup step lifts Ubuntu's AppArmor restriction, and on Ubuntu 24.04 also in a network namespace without egress, but not in a container under Docker's default confinement. `/dev/kvm` is present on the x86-64 runners only; booting a microVM there was not tried. | 19.4, DEV-5 to DEV-7 |
| P-17 | Do issue type and issue field changes expose an actor and a timestamp? | SEC-1; use of types and fields |
| P-18 | (M4) For A3: does a ruleset accept zero required approvals with required code-owner review, and does an unowned CODEOWNERS pattern lift the requirement? The alternative is a fourth factory-reported check `factory/approval`. | 13.6 |
| P-19 | (Not scheduled) Can an installation token list merge-queue refs and entries, and which permission does that need? | PR-6 |
| P-20 | Is the push of a base-update merge commit (12.4), or the ref update of a reset (7.6), rejected for an App without the Workflows permission when the default branch changed a file under `.github/workflows/` since the old `base_sha`? | 12.4, 7.6, GH-11 |
| P-21 | In non-bare `claude -p` with `--setting-sources user` and `--strict-mcp-config`, which of these still load from the working directory: `CLAUDE.md`, `CLAUDE.local.md`, skills, commands, subagents, plugins, hooks? Is there a flag or setting that disables each? Answered on 2026-11-19 by live sessions of Claude Code 2.1.311 on macOS, outside the factory's sandbox: none of them, and neither `AGENTS.md`, `.claude/rules/`, the `env` blocks of the two settings files, the permission rules of `.claude/settings.local.json`, nor anything found in a subdirectory or written mid-session. `--setting-sources user` alone does this, `.mcp.json` included. Second switches exist for instruction files, for skills and commands, for hooks and for `AGENTS.md`; none for subagents or plugins. The per-execution config directory is live configuration: what a session writes there takes effect in that session. | AG-3a, AG-4 |
| P-22 | Under subscription authentication: do `--max-turns` and `--max-budget-usd` work, what usage does the stream report, and what does the CLI emit when a plan limit is reached (event shape and reset time)? Which hosts does the CLI contact? Answered on 2026-11-19 by the same sessions, on a `claude setup-token` token, except for the plan limit, which was not reached on purpose. Both caps work and end the session with `error_max_turns` or `error_max_budget_usd` and exit code 1. The final `result` holds tokens and API-equivalent dollars per model, as running totals for the session; per-call output tokens are in `message_delta` events only. A session that completes emits one `rate_limit_event`, after its first response, with the window, its utilisation, `resetsAt` in Unix seconds and the account's usage-credit state; the event of a rejected request is known from the documentation only. `api.anthropic.com` is the one host a session needs; without `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` the CLI also asks for a Datadog log host. | Section 14, 8.3, SEC-11, 13.5 |
| P-23 | (M4) Can a host-side pass-through supply the subscription credential so that the worker holds none: does the CLI accept a custom base URL under subscription authentication, and is that within the plan's terms? | 13.5; would remove the token from workers before auto-merge |
| P-24 | On the server, as the factory's user: do the bubblewrap tests of P-16 pass, does the kernel accept a seccomp filter, and can the user open `/dev/kvm`? The server's other facts were observed on 2026-11-20 (4.4). These three were not: `bubblewrap` was not installed and the account that ran the script is not in group `kvm`. Answered on 2026-11-20 with the stand-in commands of P-16, not the factory's sandbox: as the factory's user, a member of group `kvm`, all five tests pass with bubblewrap 0.12.0 and no setup step, a sandbox given a seccomp filter runs under it, and `/dev/kvm` opens and answers `KVM_GET_API_VERSION` and `KVM_CREATE_VM`. No guest was booted. | 4.4, 13.3; the exit check of M0 (22) |
| P-25 | Does adding a label that does not exist fail, or create the label? Answered on 2026-11-21 by calls on the scratch repository, with the operator's token, which has admin permission there, and not an installation token: the REST API creates the label, grey and without a description, and applies it. Each of the four calls that take label names did so (add labels, replace the label set, update an issue, create an issue), under API versions `2026-03-10` and `2022-11-28`, and each answered with its ordinary success. `gh issue edit --add-label` refuses with `'<name>' not found`. In the one request that tried it, to the add-label endpoint, a name that differs from an existing label only in letter case applied the existing label and created nothing. | GH-1, 20.1, D-21 |
| P-26 | (M1a) What a session limit meets on the stream of a headless session. With stream-JSON input, does the CLI's `--max-turns` count start over with each message while its budget total runs on, as the documentation says? What does the stream show of a subagent's model calls: the lines, their token counts, and whether `--max-turns` and `--max-budget-usd` count them? Which fields of a `rate_limit_event` show usage credits switched on, and in use? No session of P-22 settled any of the three. The third is not provoked: it is recorded when a session meets it (BUD-8). | BUD-6, BUD-8, BUD-10 |
| P-27 | Which command lines make git skip its hooks, what does git hand a hook, and does a `PreToolUse` hook of Claude Code refuse such a command in a live session? Answered on 2026-11-21 on git 2.56.1 and Claude Code 2.1.323 on macOS, by a script in throwaway repositories and by hand in the session that wrote the hook. On a commit, `--no-verify`, `--no-verif`, `--no-veri` and `-n`, alone or among other short options, skip the hook; a shorter abbreviation is refused as ambiguous, the last of `--verify` and `--no-verify` wins, and a word that is the value of another option, `-n` after `-m` or `--` after `--mess`, is no flag and does not end the options. `git push --no-verify` skips the pre-push hook, and `-n` there is a dry run. A `core.hooksPath` given through `-c`, `--config-env` or the `GIT_CONFIG_*` variables skips every hook. A hook that is not executable is ignored, with a hint, and the commit goes through. A hook runs at the top of the work tree with `GIT_INDEX_FILE` set, and in a linked worktree `GIT_DIR` too. A relative `core.hooksPath` holds in a linked worktree. The `PreToolUse` hook refused each skip it was given, alone and in a compound command, and let a commit through whose message names the flags. With `"if": "Bash(git *)"` it was not run for `time git …`, `env … git …` or git named by its path; with a filter of its own it refused those. A command inside `bash -c` went through both, and a flag that comes out of a variable skips the hook as one written out does. | DEV-9, DEV-10 |

### 23.3 Known weak points

- Human review is the main defence against a plausible but wrong change, and review quality falls as volume rises. The review-queue limit and the size budget are blunt tools.
- There is one human, so the person who approves the issue is also the person who approves the code, and their own pull requests bypass the ruleset.
- Reviewer agents are lenient by default and unevaluated until M3.
- The agent CLI's protocol and flags change often. The pinned version range and the contract test contain this but do not remove it.
- In `subscription` mode a token valid for a year sits in the environment of a process that may be subverted, and the CLI runs without bare mode. The mitigations (model-requests-only scope, egress allowlist, literal-token scan, lock-down contract test) reduce this but do not remove it. Bare mode is announced to become the default for `-p`, which may force a change to this mode.
- The plan's limits are shared with the operator's own interactive use and assume ordinary individual usage; a busy daemon may exhaust them and stall both.
- Lights-out rests on class predicates and automated evidence. For user interfaces and service behaviour this specification does not yet define that evidence, so those kinds of work stay reviewed until it does.
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
- `07-gaps`: contradictions between the reports and how they were resolved. Where this specification departs from one of its resolutions, 23.1 or 13.6 records why.
- `08-fact-check`: 66 load-bearing claims checked against primary sources (56 confirmed, 9 partly, 1 wrong; corrections are applied here).
- `09-spec-review`: the 59 findings of the adversarial review of draft 1, each with the reviewer's proposed fix and the sceptic's verdict. This draft addresses each one, not always with the proposed fix.
- `10-spec-recheck`: the 47 problems a recheck found in draft 2 (unresolved findings and contradictions the rewrite introduced), addressed in draft 3.
- `14-server-facts`: what the operator's server is, observed on 2026-11-20 and set against the host requirements of 4.4. Reports 11 to 13 are on `research/*` branches and are cited from `probes.md`.
- `15-session-sizes`: how large the operator's interactive sessions on the factory repository were, in model calls, minutes and API-equivalent dollars, measured on 2026-11-21 for the session limits of section 14 (D-22).
