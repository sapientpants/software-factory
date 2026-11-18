# 07 - Gaps, contradictions and resolutions

Date: 2026-11-18. Role: completeness critic. Input: reports 01-06 in this directory.

This report does three things: lists what a buildable spec still needs that reports 01-06 do not supply, resolves the places where those reports contradict each other, and researches the most important gaps. Bracketed numbers refer to the Sources section.

## 0. Evidence notes

- Read directly: Symphony `SPEC.md` and the reference `WORKFLOW.md` [1][2], Claude Code headless, permissions, gateway and legal pages [3][4][5][6], GitHub REST docs for pull requests and git refs [7][8], issue-form syntax [9], Actions retention [10], the App manifest flow [11], Factory's Agent Readiness docs [12], Renovate onboarding [13], Anthropic's agent-evals article [14], mabl's multi-repo write-up [15], axum docs [16], rustc bootstrapping [17].
- Local read-only probes on the operator's machine (`gh api` GET requests, `--version` calls). These are observations of this environment, not general facts.
- Search summaries only (primary page not read or returned 403): OpenAI's SWE-bench Verified post [18], the agent-PR review studies [19][20][21], `createCommitOnBranch` file-mode limits [22][23], App approval counting [24], the duplicate-ref error text [25], issue-form label behaviour [26], Claude Code transcript cleanup [27].
- Not verifiable with the tools I had: GitHub plan-availability banners for rulesets and merge queue (the docs render them outside the fetched text). I replaced that with a live probe (section 3, R1).

## 1. What reports 01-06 leave unspecified

| # | Gap | Why the spec needs it |
|---|---|---|
| G1 | Deployment profile: personal account vs organisation, plan, single operator vs team, macOS vs Linux | Decides which GitHub-enforced gates exist at all; five of six reports list it as an open question |
| G2 | Bootstrapping: how the factory builds itself | Report 01 flags Gas Town's instability from self-building; no report says which binary operates on the factory's own repo or how upgrades happen |
| G3 | Operator CLI surface and daemon control API | Report 02 lists eight verbs; report 05 leaves CLI-to-daemon transport open; no setup, onboarding, cost, eval or maintenance commands |
| G4 | Configuration layering and trust | Report 05 says repository config is untrusted for security keys, but nobody defines the layers, precedence or which keys live where |
| G5 | Onboarding an existing repository | Report 01 asks for a readiness contract; nobody specifies the flow, the artefacts created or a staged rollout |
| G6 | Multi-repository support | Open question in 01; data-model consequences unspecified |
| G7 | Human review ergonomics | Reports cap agent concurrency but not the human review queue, although 06 cites review time as the bottleneck |
| G8 | Escalation and notification UX | 02 defines a `needs_human` state; nobody defines categories, who is notified, or how |
| G9 | Data retention and cleanup | Run directories, transcripts, branches, workspaces and journals have no lifetime |
| G10 | Evaluating the factory itself | 01 says "re-evaluate on a small fixed task set"; nobody defines the suite, graders or promotion rule |
| G11 | Delivery roadmap with exit criteria | None of the reports orders the work |
| G12 | Schema versioning and upgrade with in-flight runs | Status-comment marker is "v1" with no compatibility rule; no drain procedure |
| G13 | Approval inheritance for factory-created issues | 03 wants planner-created sub-issues; 04 and 06 want `state:ready` set by a human. Unreconciled |

## 2. Contradictions between researchers, with proposed resolutions

| # | Conflict | Resolution |
|---|---|---|
| C1 | **Run state.** 01: in memory, rebuilt from GitHub. 02/05: SQLite journal. 04: lease and heartbeat in the status comment. | Three tiers. GitHub holds what must survive loss of the host: coarse state label, PR, check runs, and a status-comment marker with run id, attempt count, cumulative spend and last failure fingerprint, written on stage transitions only. SQLite holds journal, outbox, leases and heartbeats and is rebuildable (`sf rebuild`). Do not write heartbeats to GitHub: writes are rate-limited and Symphony itself keeps them in memory. Pure in-memory is inadequate because Symphony loses retry state on restart and lists persisting it as a TODO [1]. |
| C2 | **Claim.** 02: local lock is authoritative. 04: atomic git ref plus lease in the comment. | v1 is single-daemon: local unique index plus `flock` is authoritative. Still create the branch ref before swapping the label; if creation fails because the ref exists and the local store has no run for it, halt dispatch for that issue and report a second writer. No distributed lease protocol in v1. |
| C3 | **Who commits.** 02: agent commits locally in a worktree. 05: `.git` read-only, factory commits. 06: extract a patch, apply to a clean clone. | In a linked worktree, hooks and config live in the shared common directory, so an agent-writable `.git` is write access to state the host and other runs use. Give each run a standalone clone that borrows objects from a read-only mirror (`git clone --reference`), let the agent commit there for its own checkpointing, and have the host build the published commit with its own git directory pointed at the workspace as work tree, so the workspace's `.git/config` and hooks are never read. Needs a prototype test. |
| C4 | **Branch name.** `factory/<issue>-<slug>` (02), `factory/issue-<n>` (04), `factory/<issue>-<run>` (06). | `factory/issue-<n>`. Stable when the title changes, gives one open PR per head, and doubles as the claim fence. Attempt and run id go in commit trailers and the marker. |
| C5 | **State vocabulary.** Symphony's six states (01), five `state:*` labels (04), eight internal stages (02). | Two levels. Labels are the human contract (04's five). The internal stage is shown only in the status comment and check runs. Symphony "Merging" maps to in-review with an approved PR; "Rework" maps to the implement stage, attempt n+1. |
| C6 | **Rework.** 01: discard branch and PR, restart from main. 02/03: bounded in-place fix rounds. | Both, as separate loops. Automated findings (verify, CI, reviewer agent): in place, at most two rounds. Human "changes requested": one in-place round with the submitted review as input. Reset (close PR, delete branch, fresh attempt) only on an explicit `factory:restart` label, on repeated merge conflict, or when round caps are spent. Symphony's reset rule is a prompt convention in its sample `WORKFLOW.md`, not a spec requirement [2], so evidence for either choice is thin. |
| C7 | **Sandbox strength.** 02: the agent CLI's own sandbox. 05: factory-owned OS sandbox. 06: VM boundary by default. | One `Sandbox` trait with ordered levels (`none` < `os` < `vm`) and a policy minimum per mode: attended `sf run` on a repo whose issue authors are trusted may use `os`; the unattended daemon requires `vm` unless the operator config lowers it explicitly; `none` is never accepted by the daemon. |
| C8 | **LLM credential.** 02/05: provider key in the agent's environment. 06: no key in the sandbox, gateway injects it. | 06's design works with stock Claude Code: it honours `ANTHROPIC_BASE_URL` plus a gateway credential, and in bare mode reads `apiKeyHelper` only from `--settings` [3][4][5]. Ship a `direct_key` mode for early milestones and require `gateway` mode with a per-run token for daemon operation. |
| C9 | **Repository instructions.** 05: always `--bare`. 01/03: AGENTS.md is essential. 06: load instructions from the base branch. | Always `--bare`. The host reads AGENTS.md from the base commit (`git show <base>:AGENTS.md`) and passes it with `--append-system-prompt-file`. Confirmed: bare mode skips CLAUDE.md, hooks, skills, plugins and `.mcp.json`; without it a `-p` run executes project hooks and connects `.mcp.json` servers with no trust prompt [3][4]. |
| C10 | **Trigger authorisation.** 04: `author_association` and label permissions. 06: repository permission API. | Permission API, applied to the actor of the `labeled` event read from the issue events API (a polling reconciler has no webhook sender). Never list `state:ready` in an issue form's `labels`: forms apply listed labels at creation, and a secondary source reports this happens even for read-only authors [9][26]. |
| C11 | **Reviewers.** 02: one reviewer session. 03: three (spec, architecture, security). | Per-dimension verdict fields in the schema from day one; number of sessions is policy. Anthropic recommends grading each dimension with a separate judge [14], so default to separate spec-conformance and quality/security calls for Tier 1-2 and one call for Tier 0. |
| C12 | **Test-author role** (03) has no stage in 02's machine. | Add an optional `author_tests` stage between plan and implement, same branch, sequential, so "one writer at a time" still holds. Its output paths become protected for the implement stage. |
| C13 | **Where verification runs.** 02: daemon. 04: Actions. | Both, with different jobs. Daemon-run verify (fresh sandbox, published commit) is fast feedback and the only gate on repositories without rulesets. Actions check runs are the gate of record where rulesets can require them. |
| C14 | **Autonomy naming.** Shapiro levels (01) vs A0-A4 (06). | Use A0-A4; default A2. |
| C15 | **Publish path.** 04: `createCommitOnBranch` by default. 06: doubts it can express everything. | Third-party tools report it writes every file as mode 100644: no executable bit, symlinks or submodules [22][23]. Make `git push --force-with-lease=<ref>:<expected>` over HTTPS the single v1 path; offer the GraphQL path only as an option when Verified badges are required and the change set is plain files. |
| C16 | **Separation of duties.** 04 asks whether the human who approved the issue may approve the PR. | With one human operator that rule deadlocks. Make it a profile setting: off for `solo`, on for `team`. |

## 3. Open questions resolved

**R1. Plan and ownership of the first target (G1).** Probed read-only with the operator's `gh` login. The account is a personal (User) account. On a private repository it owns, `GET /repos/{o}/{r}/rulesets` and `GET .../branches/main/protection` both return 403 "Upgrade to GitHub Pro or make this repository public to enable this feature." On a public repository of the same account the rulesets call returns an active ruleset. The one organisation the account belongs to is on the Free plan. Consequences: if the factory repository is private on this account, GitHub enforces no review, no required checks and no push restriction, and the App's Contents-write token can push to the default branch. The spec must either require a public repository or a paid plan for "enforced" mode, or define a "factory-enforced" degraded mode that the daemon detects (that 403 is the probe) and reports loudly. Merge queue is not available on personal repositories per report 04, so `FactorySerial` or `HumanMerges` is the v1 merge strategy.

**R2. Local toolchain.** `git` 2.56.1 and `claude` 2.1.311 are installed; `codex` is not. The Claude Code version is above the documented thresholds for `--permission-prompts` (2.1.259), full bare-mode semantics (2.1.286) and Manual-mode default for `-p` (2.1.302) [3]. A Claude-only v1 matches the machine.

**R3. Async merge API.** Confirmed in the REST reference: `PUT /repos/{owner}/{repo}/pulls/{pull_number}/merge-async` with optional `sha`, `merge_method`, `merge_action` (`default`, `direct_merge`, `merge_queue`), `commit_title`, `commit_message`, `bypass_rules`; 202 returns a UUID; `GET .../merge-async/{uuid}` reports `pending`, `merged`, `enqueued` or `failed`; results are kept 24 hours; 409 means another merge request is pending. The classic endpoint is no longer recommended [7]. The factory must never send `bypass_rules: true`, and must persist the UUID in its journal because GitHub forgets it after a day.

**R4. Authentication mode.** Bare mode never reads OAuth credentials; it needs `ANTHROPIC_API_KEY`, an `apiKeyHelper` in `--settings`, or cloud-provider credentials, and Anthropic says bare "will become the default for `-p` in a future release" [3]. The legal page says OAuth is for "ordinary use of Claude Code", that plan limits "assume ordinary, individual usage", and that developers building products should use API keys [6]. Reports of a separate subscription credit pool for programmatic use are secondary and I could not confirm them. Spec position: API key or cloud credential is the supported path; subscription login is unsupported.

**R5. Duplicate ref creation.** The docs still list 201, 409 and 422 without saying which applies [8]. Field reports show `422 - Reference already exists` [25]. Treat any non-201 as "not claimed" and match on status, not message text.

**R6. App approvals.** Community and vendor documentation say an approving review from a GitHub App counts toward required approvals (CodeRabbit and CodeAnt advertise this), while `github-actions[bot]` approvals do not [24]. Not confirmed in GitHub's docs. Spec consequence: forbidding the factory App from approving is not enough; a second installed App could satisfy the requirement, so the onboarding check should list installed Apps with pull-request write and warn.

**R7. Issue-form rendering.** The docs say only that responses "are converted to markdown" [9]. The exact layout is undocumented, so the parser must be tolerant, versioned and paired with a lint command that tells the author what failed.

**R8. Symphony scope.** One workflow file and one tracker scope per process is implied, scheduler state is in memory, and the operator surface is an optional loopback HTTP API: `GET /api/v1/state`, `GET /api/v1/<issue>`, `POST /api/v1/refresh` [1]. That API is a usable baseline for G3.

**R9. CLI transport.** axum 0.8.9 implements its `Listener` trait for `tokio::net::UnixListener` [16], so HTTP/JSON over a Unix socket needs no extra dependency.

## 4. Gap research and recommendations

### 4.1 Bootstrapping (G2)

No source describes a pinned-release scheme for an agent harness that modifies itself; my searches found only general dogfooding advice. The closest established practice is compiler bootstrapping: rustc builds in-tree source with a pinned prebuilt stage0 compiler and treats the rebuilt compiler as the one to ship [17]. Applied here:

1. **Stage 0 (human-built).** A person working with an interactive agent builds the walking skeleton. The factory does not exist yet.
2. **Stage 1 (pinned operator).** The factory binary that works on the factory's own repository is always an installed release, never a build from the tree it is modifying. Upgrading it is a human action.
3. **Promotion check.** A candidate release becomes the operating binary only after it passes the factory eval suite (4.8). Keep the previous binaries for rollback and record the factory version in every run marker.
4. **Self-modification guard.** Paths that define the factory's own gates (policy, prompts, verifier and sandbox code, `.github/**`, release configuration) are never auto-merged, in addition to 06's list.
5. **A second target early.** Report 01 records that Gas Town was only ever used on itself. Run a small unrelated repository through the factory from the first daemon milestone.

### 4.2 Operator CLI and control surface (G3)

Proposed surface, one binary `sf`, every command with `--json` and stable exit codes:

- Setup: `sf init`, `sf app create` (GitHub App manifest flow: POST a manifest, exchange the code at `/app-manifests/{code}/conversions` for id, PEM and webhook secret within one hour; works for personal accounts and organisations [11]), `sf repo add|check|remove`, `sf doctor [--probe]`.
- Operate: `sf daemon`, `sf run <issue> [--stage s] [--dry-run]`, `sf status [--watch]`, `sf show <issue>`, `sf logs <run>`, `sf pause|resume [--repo]`, `sf cancel`, `sf retry`, `sf drain`.
- Inspect: `sf cost`, `sf metrics`, `sf policy check|explain`, `sf issue lint <n>`.
- Maintain: `sf gc`, `sf rebuild`, `sf export <run>`, `sf eval run|replay`, `sf self-update`.

The CLI talks to the daemon over HTTP/JSON on a Unix socket with mode 0600, modelled on Symphony's optional API [1]; no TCP listener by default. Read-only commands fall back to reading the journal when the daemon is down. On the GitHub side, control is labels (`factory:hold`, `factory:restart`) plus a small set of slash commands in comments from authorised users. `sf doctor --probe` should run the unverified GitHub behaviours (R5, R6, App as assignee, signed API commits) against a scratch repository and record the answers.

### 4.3 Configuration layers (G4)

1. **Operator config** (`~/.config/sf/config.toml`, trusted): App id and key reference, LLM credential mode, sandbox backend and minimum level, global budgets, repository allow-list, autonomy ceiling per repository, retention.
2. **Repository policy** (`.factory/policy.toml` plus prompt files), read from the default-branch head only, never from a PR head or a workspace. Hot-reload with last-known-good, as Symphony requires [1]. It may only tighten the operator's ceilings.
3. **Per-issue settings** (risk tier, size budget) through labels or fields, honoured only when set by an authorised actor.

Both files carry a schema version. Symphony treats hooks as "fully trusted configuration" [1]; here repository-supplied setup commands run only inside the sandbox.

### 4.4 Onboarding a repository (G5)

Renovate's flow is the best-tested pattern for an automation that enters an existing repository: it opens a "Configure Renovate" PR, "will not make any changes to your repository or raise any further Pull Requests until after you merge" it, and treats closing it as opting out [13]. For readiness scoring, Factory's Agent Readiness model evaluates nine pillars across five gated levels (80% of a level's criteria unlock the next), with repository-scoped and per-application criteria [12]. It is a vendor model with no published outcome data, so use it as a checklist source.

Recommended flow: `sf repo add` runs a readiness report (blocking checks: one-command setup and test, deterministic checks, AGENTS.md, default-branch protection status; advisory score for the rest), then opens one onboarding PR containing the policy file, issue forms, a CI workflow template and an AGENTS.md skeleton if missing. Nothing else happens until it merges. After merge the repository advances through modes: `observe` (lint issues, post plans, no PRs), `propose` (A2), then auto-merge classes. Readiness gaps are filed as `state:triage` issues.

### 4.5 Multi-repository (G6)

Symphony implies one workflow per process [1]. mabl's account of running agents across 79 repositories used a central shared-rules repository, a per-repository instruction file and a hand-maintained 850-line coordination graph; cross-repository PRs still merge separately, and its numbers are self-reported [15]. A benchmark of multi-repository tasks reportedly shows 11-43% full success (secondhand) [28].

Recommendation: make the data model multi-repository from the start (repository id in every key, per-repository policy, budgets and concurrency, one daemon serving every repository in the operator allow-list), but keep one issue to one repository to one PR. Cross-repository work is expressed by a human as linked issues. No cross-repository workspace in v1.

### 4.6 Human review ergonomics and escalation (G7, G8)

Evidence: PR size is the dominant early predictor of review effort for agent PRs (AUC 0.957 for the model, 0.933 for size alone, weaker on repository-disjoint splits) [19]; the most frequent rejection pattern in about 33k agent PRs is reviewer abandonment [20]; a vendor dataset shows review cycles rising from 1.27 for PRs under 10 lines to 3.54 above 1,000 [21]. All three come from search summaries.

Recommendations:
- **Review-queue WIP limit.** Stop starting implement stages when the number of factory PRs awaiting human review in a repository reaches a cap (suggest 3). This makes the factory a pull system limited by human attention. No other report has this control.
- **Size budget** enforced before the PR is marked ready; over budget means decompose.
- **Fixed PR body**: outcome, acceptance criterion to evidence table, risk tier, where to look first, what was left out, follow-up issues, cost.
- **Native review flow**: request review from the human who approved the issue; act on a submitted review, not on individual comments. GitHub's own agent iterates on `@copilot` mentions and does not count the requester's approval [29].
- **Escalation**: one comment per escalation with a category (clarification, spec conflict, budget, verification, merge conflict, policy denial, infrastructure), what was tried, one question, and the resume command; one mention of the approver; a pinned dashboard issue per repository for the digest. Nudge a stale review once, then stop.

### 4.7 Data retention (G9)

| Artefact | Location | Proposed default | Basis |
|---|---|---|---|
| Agent event stream, transcripts | Run directory on the host | 30 days full; 180 days for failed or reverted runs | Claude Code sweeps its own transcripts by `cleanupPeriodDays` (30 by default per user reports) [27]; point the agent's config directory into the run directory so the factory owns the lifetime |
| Journal | SQLite | Indefinite; prune event detail after 180 days | Small; needed for metrics |
| Status comment, PR body, check runs | GitHub | Permanent | System of record |
| CI logs and artefacts | GitHub Actions | 90 days (GitHub default; 1-90 public, 1-400 private) [10] | Do not rely on them as the audit trail |
| Async merge result | GitHub | 24 hours [7] | Persist the outcome locally |
| Workspaces | Host | Remove 24 hours after a terminal state | Symphony keeps them indefinitely [1]; disk grows with Rust targets |
| Branches | GitHub | Delete on merge; 14 days after close without merge | |

Never upload transcripts to a public repository. Provide `sf gc` and `sf export`.

### 4.8 Evaluating the factory (G10)

Anthropic's guidance: start with 20-50 tasks drawn from real failures, each with a reference solution and unambiguous graders; use deterministic graders for correctness and calibrated model graders for quality; run multiple trials from a clean environment; report pass@1 and pass^k; keep a regression suite near 100% and a capability suite with headroom [14]. Public benchmarks are a poor acceptance test: OpenAI reportedly stopped reporting SWE-bench Verified in February 2026 after finding flawed tests in 59.4% of audited problems and evidence of contamination, and the proposed successor is itself disputed [18].

Recommendation: three layers. (1) Deterministic harness tests with the fake agent. (2) A factory eval suite built from the factory's own closed issues and merged PRs (`sf eval replay` resets to the parent commit, runs the issue, grades with the merged tests), run on every harness, prompt or model change and required for binary promotion. (3) Production metrics from reports 03 and 06 with tripwires. No source supports specific pass thresholds; choose and tune them.

### 4.9 Roadmap (G11)

| Milestone | Scope | Exit criterion |
|---|---|---|
| M0 Foundations | Workspace, CI, AGENTS.md, issue forms, labels, App, GitHub probe suite on a scratch repository | Probe answers recorded; decisions in section 5 made |
| M1 Walking skeleton | `sf run <issue>` attended, one repository: clone, Claude adapter, local verify, push, draft PR, status comment | Five real issues merged by a human |
| M2 Daemon | Reconcile loop, journal, budgets, `needs-human`, status API, crash recovery | 48 hours unattended on a second repository; self-hosting starts on a pinned release |
| M3 Validation | Readiness lint, plan gate, reviewer, evidence manifest, protected paths, eval suite v1 | Baseline eval numbers recorded |
| M4 Hardening | VM sandbox, egress proxy, model gateway | Required before any repository with untrusted issue authors |
| M5 Scale-out | Onboarding flow, multi-repository, metrics, retention | Second repository onboarded by PR |
| M6 Merge automation | Serial merge, A3 classes, tripwires | Chosen escape-rate threshold met |

### 4.10 Versioning and approval inheritance (G12, G13)

No source covers either; these are design proposals.

- **Versioning.** The status-comment marker, the journal schema and the policy file each carry a version. Readers preserve unknown fields, so an older binary can still read a marker written by a newer one after a rollback. `sf drain` stops new dispatch and waits for running stages to checkpoint before an upgrade.
- **Approval inheritance.** Issues the factory creates (planner sub-issues, discovered work, maintenance) start in `state:triage`. The exception is a sub-issue created from a plan a human approved: the approval records a hash of the plan, and children created from that exact plan may start as `state:ready`. Any edit to the plan after approval voids the inheritance.

## 5. Still open (need the user)

1. Is the factory repository public or private, and will the account be upgraded? This decides enforced versus factory-enforced mode (R1).
2. API key or cloud credential for the agent, and the per-issue and per-day budget.
3. Which VM backend on macOS, and is Linux a target at all in v1?
4. Which second repository to use from M2.
5. Thresholds: review-queue cap, eval promotion bar, A3 promotion criteria.

## Sources

1. https://raw.githubusercontent.com/openai/symphony/main/SPEC.md
2. https://raw.githubusercontent.com/openai/symphony/main/elixir/WORKFLOW.md
3. https://code.claude.com/docs/en/headless
4. https://code.claude.com/docs/en/permissions
5. https://code.claude.com/docs/en/llm-gateway
6. https://code.claude.com/docs/en/legal-and-compliance
7. https://docs.github.com/en/rest/pulls/pulls?apiVersion=2026-03-10 and https://github.blog/changelog/2026-10-01-github-async-merge-api-generally-available
8. https://docs.github.com/en/rest/git/refs?apiVersion=2026-03-10
9. https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/syntax-for-issue-forms
10. https://docs.github.com/en/organizations/managing-organization-settings/configuring-the-retention-period-for-github-actions-artifacts-and-logs-in-your-organization
11. https://docs.github.com/en/apps/sharing-github-apps/registering-a-github-app-from-a-manifest
12. https://docs.factory.com/agent-readiness/overview.md and https://factory.ai/news/agent-readiness
13. https://docs.renovatebot.com/getting-started/installing-onboarding/
14. https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents
15. https://www.mabl.com/blog/how-we-built-a-system-for-ai-agents-to-ship-real-code-across-75-repos
16. https://docs.rs/axum/latest/axum/serve/trait.Listener.html
17. https://rustc-dev-guide.rust-lang.org/building/bootstrapping/what-bootstrapping-does.html
18. https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/ (HTTP 403; via search summary)
19. https://arxiv.org/abs/2601.00753 (search summary)
20. https://arxiv.org/abs/2601.15195 (search summary)
21. https://www.greptile.com/blog/rise-of-the-overnight-agents (search summary; vendor data)
22. https://github.com/DataDog/commit-headless (search summary)
23. https://github.com/github/gh-aw/pull/26298 (search summary)
24. https://github.com/orgs/community/discussions/181487 and https://docs.coderabbit.ai/pr-reviews/approval-policies (search summary)
25. https://gitlab.com/gitlab-org/gitlab-ce/-/issues/17766 (search summary)
26. https://dev.to/devactivity/unlocking-developer-productivity-why-github-needs-consistent-labeling-for-read-role-users-5h7e (search summary)
27. https://github.com/anthropics/claude-code/issues/23710 and https://dev.classmethod.jp/en/articles/claude-code-conversation-history-retention/ (search summary)
28. https://thenewstack.io/qodo-cross-repo-code-review/ (secondhand report of the WideSWE benchmark; search summary)
29. https://docs.github.com/en/copilot/how-tos/use-copilot-agents/coding-agent/review-copilot-prs
