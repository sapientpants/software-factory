# 02 — Orchestration architecture and runtime

Research date: 2026-11-18. Angle: how the factory's control plane should be built, with a concrete recommendation for a single-binary Rust daemon/CLI.

Evidence labels: **[demonstrated]** = spec text, working system, or measured data; **[asserted]** = vendor self-report; **[secondary]** = third-party summary only (primary returned 403 or was not located); **[judgment]** = my own inference, not sourced.

---

## 1. Summary recommendation

Build the control plane as **deterministic Rust code, not as an LLM**:

1. A **coarse, typed per-issue state machine** (`claim -> plan -> implement -> verify -> review -> merge -> done`, plus `needs_human` and `failed`). Stage bodies are agent sessions or deterministic commands; transitions are code.
2. **Checkpointed execution on embedded SQLite** (journal + outbox + leases). GitHub stays the system of record; SQLite holds execution bookkeeping that can be rebuilt from GitHub plus git.
3. **One writer per work item** in a factory-owned git worktree. Planner and reviewer are separate fresh-context, read-only sessions that run sequentially around the writer.
4. **An internal `AgentRunner` trait** with native adapters for `claude -p` (stream-json) and `codex exec --json`, then a generic Agent Client Protocol (ACP) adapter. Neither ACP nor a vendor SDK is the internal interface.
5. **A serialized merge stage per repository** that uses GitHub's merge queue where available and a factory-run "update, re-verify, merge" loop otherwise.

---

## 2. Pipeline model: state machine vs graph vs free-form loop

### What the reference systems do

**OpenAI Symphony** (spec + Elixir reference, Apache-2.0, April 2026) is an explicit orchestrator state machine around a tracker. Claim states are `Unclaimed, Claimed, Running, RetryQueued, Released`; a run attempt ends as `Succeeded, Failed, TimedOut, Stalled` or `CanceledByReconciliation`. A tick does: reconcile running issues, validate config, fetch candidates, sort, dispatch while slots remain. The agent gets one prompt (the body of an in-repo `WORKFLOW.md`) and drives the ticket itself; ticket/PR writes are explicitly *not* orchestrator logic. **[demonstrated]** (SPEC.md)

A lesson OpenAI reports from running it: agents "don't work well as fixed nodes in a state machine" because models outgrow the templates; they moved to giving agents goals plus tools. **[secondary, asserted]** (The Decoder's report of the OpenAI post; the OpenAI page returned 403 to me.)

**StrongDM Attractor** (three prose "NLSpecs", no code) is a general graph engine: a Graphviz DOT digraph whose node shapes select handlers (`box`=LLM task, `hexagon`=`wait.human`, `diamond`=conditional, `component`/`tripleoctagon`=fan-out/fan-in, `parallelogram`=tool command). Edge selection is deterministic (condition, preferred label, suggested IDs, weight, lexical order). It has `goal_gate` nodes that must succeed before exit, `retry_target` fallbacks, and a checkpoint after every node. Top-level traversal is single-threaded, and the pipeline spec defines **no loop detection for graph cycles**. **[demonstrated]** (attractor-spec.md)

**Anthropic's guidance** distinguishes workflows ("LLMs and tools are orchestrated through predefined code paths") from agents ("LLMs dynamically direct their own processes") and says to add complexity only when needed. Their June 2026 "dynamic workflows" post puts loops, barriers and stop conditions in code and each judgment in a model call, citing three single-context failure modes: *agentic laziness* (stopping at 35 of 50 items), *self-preferential bias* when judging own work, and *goal drift* through lossy summarization. **[design rationale; no controlled measurements]**

### Assessment

| Option | Strength | Weakness for this project |
|---|---|---|
| Free-form orchestrator agent (Gas Town "Mayor", Cognition's manager-of-Devins) | Flexible | Non-deterministic control plane, hard to make crash-safe or auditable. Cognition reports managers were over-prescriptive and children assumed shared state that did not exist **[asserted]**. |
| General DAG/graph engine (Attractor) | User-authorable pipelines, model routing per node | Large semantic surface (edge selection, fidelity modes, fan-in) to implement and test before the first issue ships; spec leaves loop detection and merge handling out. |
| Explicit typed state machine (Symphony-like, but richer) | Small, exhaustively testable, maps 1:1 onto GitHub labels, cheap to make durable | Less flexible; adding a stage is a code change. |

**Recommendation.** Ship v1 as a typed state machine whose *stage contract* is borrowed from Attractor, so a graph front-end can be added later without touching the runtime: every stage returns `Outcome { status: Success | PartialSuccess | Retry | Fail | NeedsHuman, notes, context_updates }`, and stage definitions (prompt file, model, tool policy, limits, gates) are data in a versioned in-repo policy file. Keep the machine **coarse**, per Symphony's lesson: it sequences lifecycle stages and enforces gates; it does not script how the agent works inside a stage.

---

## 3. Role separation: when multi-agent actually helps

The evidence converges on a narrow rule: **parallelize intelligence, serialize writes.**

- Cognition, "Multi-Agents: What's Actually Working" (2026-04-22): patterns that work are (a) a clean-context reviewer that sees only the diff and rediscovers context, (b) escalation to a stronger model as a tool, (c) map-reduce-and-manage. Parallel writers "make implicit choices" that conflict. They claim their reviewer finds on average 2 bugs per PR, about 58% severe. **[asserted, vendor]**
- CooperBench (arXiv 2601.13295, Stanford/SAP, 652 tasks): two agents cooperating on shared code succeed far less often than one agent doing both tasks (about 25% vs about 50% in the project summary; the ICLR listing frames it as about a 30% average drop). **[demonstrated benchmark; I read summaries, not the tables]**
- Anthropic, "Harness design for long-running application development" (2026-03-24): a planner / generator / evaluator split. A standalone skeptical evaluator was easier to tune than a self-critical generator; out of the box Claude is a weak QA agent that talks itself into approving; the planner should specify *what and why, not how*, because technical detail in the plan cascades errors; on Opus 4.6 the sprint decomposition was removed and the evaluator became "unnecessary overhead" for tasks inside the model's solo capability. Cost contrast: solo 20 min / $9 (broken result) vs full harness 6 h / $200. "Every component in a harness encodes an assumption about what the model can't do on its own." **[demonstrated on a handful of runs]**

**Recommendation.** Roles are sequential stages with separate contexts, not chatting peers:

- **Planner** (read-only tools): turns the issue into a plan with acceptance criteria, posted as an issue comment. Optional human approval gate. Skippable by label for small items, because every component costs tokens and latency.
- **Implementer**: the only writer; owns the branch end to end, including fixing review findings and resolving conflicts.
- **Verifier**: not an agent. The daemon runs the repo's build/test/lint commands itself. Agents self-report success unreliably (Anthropic observed features marked passing without testing).
- **Reviewer** (read-only, fresh context, optionally a different provider): receives issue + plan + diff, returns structured findings via JSON schema. Findings go back to the implementer for a bounded number of rounds.
- Large issues are decomposed into **sub-issues that become separate work items**, not into sibling agents sharing one branch.

Make planner and reviewer individually switchable in policy so they can be removed when a model upgrade makes them dead weight.

---

## 4. Durable execution and crash recovery

### What exists

- **Symphony: in-memory only.** "No retry timers are restored from prior process memory." Recovery is a fresh poll and re-dispatch into preserved workspaces. **[demonstrated]** It works because the tracker is the source of truth, but attempt counters are lost, so a poison issue can be retried indefinitely across restarts **[judgment]**.
- **Attractor:** `checkpoint.json` after every node (`current_node`, `completed_nodes`, `node_retries`, context). On resume, a node that used `full` fidelity degrades to `summary:high` "since in-memory sessions cannot be serialized". **[demonstrated in spec]**
- **Kilroy** (Go implementation of Attractor): adds a git commit per node on a run branch and an event database (CXDB); "git branch is code history; CXDB is run history". Resume from logs, CXDB, or the run branch. **[demonstrated in README]**
- **Durable-execution engines:** the Temporal Rust SDK went GA in September 2026 (`temporalio-sdk` 1.2.0); `restate-sdk` is at 0.13.0. Both need a separate server process, which breaks the single-binary constraint **[judgment from product architecture; not re-verified]**. `obelisk` (embedded) has ~200 recent downloads; too immature.

### Recommendation: checkpointed state machine, not deterministic replay

Stages are few and long (minutes to hours), so Temporal-style replay buys little. Use **SQLite in WAL mode** (`rusqlite` 0.43 or `sqlx` 0.9) with four tables:

- `runs` (one row per issue attempt: state, stage, attempt, lease, session ref, branch, cost totals),
- `events` (append-only journal of every transition and every normalized agent event),
- `outbox` (intended external side effects with an idempotency key),
- `leases`.

Rules:

1. **Every transition is one transaction** that appends the event, updates `runs`, and enqueues outbox rows.
2. **Every external effect is find-or-create.** Branch name is deterministic (`factory/<issue>-<slug>`); PRs are found by head branch before creation; comments carry a hidden marker (`<!-- factory:run=<id> step=<n> -->`) and are searched before posting.
3. **Record intent before acting.** Pre-generate the agent session ID and persist it before spawning (Claude Code accepts `--session-id <uuid>`), so a crash between spawn and first event is recoverable.
4. **Startup reconciliation** for every non-terminal run: is the child process alive (pid + start time), does the worktree exist, is the branch pushed, does a PR exist, what do the issue's labels say? Then resume, requeue, or release.
5. **GitHub wins on conflict.** If a human closes the issue or removes the label, the run is cancelled on the next reconcile (Symphony's reconciliation rule).

### Leases and claims

GitHub labels and assignees are **not** a compare-and-swap primitive; community write-ups describe the check-then-label race and work around it with nonce comments **[secondary]**. For a single daemon the correct lock is local: a unique partial index on `(repo, issue) WHERE active`, plus an `flock` on the data directory so two daemons cannot share a store. Labels, assignee and a claim comment are *advisory visibility* for humans. Leases carry `expires_at`, renewed by a heartbeat tied to agent event activity.

If multi-host operation is ever needed, a candidate lock is atomic creation of a git ref (`refs/factory/claims/<issue>`). GitHub's REST docs list 201/409/422 for "Create a reference" but do not state the behaviour for an existing ref; test before relying on it.

---

## 5. Scheduling and concurrency

Symphony's defaults are a reasonable starting vocabulary: poll every 30 s; `max_concurrent_agents` 10 with per-state overrides; sort by priority, then age, then identifier; continuation re-check 1 s after a clean exit; failure backoff `min(10000 * 2^(attempt-1), 300000)` ms; stall timeout 300 s since last event; turn timeout 1 h; `max_turns` 20. **[demonstrated in spec]**

Recommendations:

- Poll + reconcile loop on tokio; webhooks are an optimization, never required for correctness.
- Three nested limits: global, per-repository, per-stage (`implement: N`, `review: M`, `merge: 1`).
- **Default concurrency low (2 to 3 per repo).** A study of 33,596 agent-authored PRs (arXiv 2607.04697) replayed three-way merges for co-active PR pairs: 19.8% of same-agent pairs and 41.7% of cross-agent pairs conflicted textually; about 42% of conflicts were structural (modify/delete, add/add). **[demonstrated]** Throughput gained from parallelism is paid back at merge time.
- Admission control on spend: per-run and per-day budgets, and back-off when runners report rate limits (Claude Code `system/api_retry` with `error: rate_limit`; Codex `usageLimitExceeded`).
- Dispatch only unblocked items; persist attempt counts across restarts.
- Hot-reload the policy file with last-known-good fallback and a per-tick preflight (Symphony).

---

## 6. Workspace isolation

| Mechanism | Isolation | Cost | Notes |
|---|---|---|---|
| git worktree | Files and branch only | Lowest | Shares `.git`, so a misbehaving agent can touch other refs, hooks, config. Claude Code's docs confirm the sharing and that `-p` runs do not clean up their worktrees or locks. |
| Local clone per run | Independent refs/config | Low to medium | `git clone --local` hardlinks objects. Symphony uses a per-issue directory populated by an `after_create` hook. |
| Container / VM | Process, network, filesystem | Highest | Needed when issue text is untrusted or services need ports; Claude Code's sandbox is incompatible with Docker inside it. |

**Recommendation.** A `Workspace` trait with `worktree` (default), `clone`, and later `container` providers. The factory, not the agent CLI, creates workspaces: a factory-owned bare mirror per repo, one worktree per run outside any human checkout. Adopt Symphony's invariants: agent `cwd == workspace path`; path stays under the workspace root; directory names sanitized to `[A-Za-z0-9._-]`. Lifecycle hooks (`after_create`, `before_run`, `after_run`, `before_remove`) with timeouts cover dependency install and cache warm-up.

Two further rules:

- **The agent never holds GitHub write credentials.** It commits locally; the daemon pushes the one branch it owns and creates the PR. Symphony's spec recommends host-side tools so "the child process never reads raw tracker credentials"; Attractor's local environment scrubs `*_API_KEY`, `*_SECRET`, `*_TOKEN` from the child environment.
- **Turn on the OS sandbox** inside the workspace (Claude Code: Seatbelt on macOS, bubblewrap on Linux, with `sandbox.failIfUnavailable` so it cannot silently fall back; Codex: `--sandbox workspace-write`, default is read-only).

Rust-specific **[judgment]**: every worktree gets a cold `target/`; a shared `CARGO_TARGET_DIR`, `sccache`, or a copy-on-write clone of a warmed target each need measuring.

---

## 7. Long-running work and context limits

- Anthropic, "Effective harnesses for long-running agents": an initializer session creates a JSON feature list (agents may only flip `passes`), a progress log, and `init.sh`; each later session reads git log + progress file, does one feature, verifies, commits, updates the log. "Compaction isn't sufficient." **[demonstrated on one app class]**
- The 2026 follow-up shows this is model-dependent: Sonnet 4.5 needed context *resets* with structured handoffs; Opus 4.5 and later ran one continuous session with automatic compaction for 2+ hours. **[small n]**
- Claude Code docs: compaction can drop early instructions, so persistent rules belong in `CLAUDE.md`; and for continuity, "Don't rely on session resume. Capture the results you need ... and pass them into a fresh session's prompt. This is often more robust".
- Symphony: continuation turns reuse one live thread up to `max_turns`.

**Recommendation.** Treat an agent session as a cache, never as state.

1. Durable run state lives in **git commits on the branch plus a run directory of handoff files** (`plan.md`, `progress.md`, `handoff.json`) kept outside the repo tree and exposed to the agent as an extra readable/writable directory; summaries are mirrored to the issue/PR for humans.
2. **Every stage must be restartable from branch + handoff files alone.** Session resume (`claude --resume <id>`, `codex exec resume <id>`, ACP `session/resume`) is an optimization used for short feedback loops (CI failure, review findings) and silently falls back to a fresh session with the handoff when the session is missing, the context is near its limit, or the process crashed.
3. Stage prompts end with an explicit close-out protocol: commit, update `progress.md`, emit a schema-validated result (`--json-schema` for Claude Code, `--output-schema` for Codex).
4. Make "fresh context per attempt" vs "continue session" a per-stage policy knob, since the right answer changes with each model generation.

---

## 8. Retry, escalation, loop detection

Layered, cheapest first:

| Layer | Trigger | Action | Precedent |
|---|---|---|---|
| Transport | 429, 5xx, network | Exponential backoff with jitter; never retry 400/401/403/config errors | Attractor retry classes (200 ms x2, cap 60 s) |
| In-session loop | Same tool-call signature (name + args hash) repeating with period 1 to 3 inside a window of 10 | Steer once, then interrupt | Attractor coding-agent-loop spec; Gemini CLI and OpenHands use similar detectors **[secondary]** |
| Stall | No events for N seconds | Kill process group, retry | Symphony 300 s; Kilroy example 600 s |
| Budget | Turns, USD, wall clock | Stop the attempt | Claude Code `--max-turns`, `--max-budget-usd` (client-side estimate, can overshoot by one response) |
| No-progress cycle | Same failing-check fingerprint or same diff hash on consecutive attempts; review/fix round cap reached | Escalate | **[judgment]**; Attractor has no cycle detection, so this must be designed in |
| Poison item | Persisted attempts >= max (default 3) | `failed` + human | Symphony loses this on restart |
| Systemic | K consecutive runs fail with the same class (auth, provider outage) | Pause all dispatch | **[judgment]** |

Hard-stop thresholds for repetition are contested (an OpenHands issue asks for configurable ones because legitimate polling trips them); prefer warn-then-stop.

**Escalation is a durable state, not an error.** `needs_human` sets a label, posts one comment (what was tried, the specific question, how to resume), releases the slot, and waits for a human reply or label change. Headless runs must deny permission and user-input requests rather than await them (`--permission-prompts none` on Claude Code; Symphony fails such turns).

---

## 9. Merge conflicts and merge queues

Facts:

- GitHub's merge queue builds `gh-readonly-queue/{base}` branches, requires CI to run on the `merge_group` event, removes PRs whose group fails or times out, and offers build concurrency 1 to 100. `gh pr merge` enqueues when the base requires a queue (or enables auto-merge); the GraphQL `enqueuePullRequest` mutation enqueues strictly. **[demonstrated in docs]**
- Availability is reported as organization-owned public repos, or private repos on Enterprise Cloud **[secondary: search snippet of the docs; the pages I fetched did not state it]**. A personal-account repository may therefore have no native queue.
- GitHub stacked PRs entered public preview at the end of July 2026 **[secondary]**.

**Recommendation.** A per-repository **merge coordinator with concurrency 1** behind a `MergeStrategy` enum:

- `NativeQueue`: when branch rules require a queue, enqueue and watch for removal.
- `FactorySerial`: otherwise, for an approved and green PR: pre-check with `git merge-tree --write-tree` against current base; if clean, update the branch, wait for CI, merge; repeat for the next PR.
- `HumanMerges`: stop at "ready".

On conflict, re-enter `implement` with a fresh context whose handoff contains the item's plan and summaries of the PRs that landed in between; re-verify, re-review if the resolution is non-trivial, bound the attempts, then escalate. Use `--force-with-lease` only on factory-owned branches.

Prevention matters more than cure: small PRs, short-lived branches, rebase before review. Having the planner declare a file footprint so the scheduler avoids overlapping items is plausible, but I found no measured evidence for it.

---

## 10. Invoking coding agents behind a provider-neutral interface

| Route | What it offers | Problem |
|---|---|---|
| **Claude Code headless** (`claude -p`) | `--output-format stream-json` events (`system/init`, `system/api_retry`, final `result` with subtype, `session_id`, `total_cost_usd`); `--input-format stream-json`; `--json-schema`; `--session-id`, `--resume`; `--max-turns`, `--max-budget-usd`; `--permission-mode`, `--permission-prompts none`; `--bare` | Flags move quickly (behaviour changes at v2.1.259, .286, .302). `--bare` skips CLAUDE.md, hooks, MCP and requires an API key; it "will become the default for `-p`". Without it, `-p` runs repo hooks and `.mcp.json` with no trust dialog. |
| **Claude Agent SDK** | Same loop as a library | Python and TypeScript only; it "runs the Claude Code binary". The docs tell other languages to run the CLI as a subprocess. |
| **Codex `exec --json`** | JSONL events (`thread.started`, `turn.started/completed/failed`, `item.*`, `error`), `--output-schema`, `codex exec resume <id>`, `CODEX_API_KEY` | Read-only sandbox by default; `--full-auto` is no longer supported. |
| **Codex app-server** | JSON-RPC over stdio: `thread/start`, `thread/resume`, `turn/start`, `turn/steer`, `turn/interrupt`, approval requests, `codex app-server generate-json-schema` for typed bindings | More surface; websocket transport is "experimental and unsupported". This is what Symphony uses. |
| **ACP** | One JSON-RPC protocol for ~50 agents; `session/new`, `session/prompt`, `session/update` (`tool_call`, `plan`, `usage_update`), `session/cancel`, capability-gated `session/load`/`resume`; official Rust crate | Editor-oriented. Claude and Codex are reached through adapters (`claude-agent-acp`, `codex-acp`) that add a process and lag features. The Rust crate hit 1.0.0 on 2026-06-25 and is at **5.4.0** today: four major bumps in under five months. |
| **Direct model APIs + own loop** | Full control (Attractor's coding-agent-loop spec; Kilroy's `api` backend) | You re-implement provider-specific edit tools, truncation, caching, compaction and sandboxing that vendor harnesses improve monthly. |

**Recommendation.** Define the neutral interface yourself:

```rust
trait AgentRunner {
    fn capabilities(&self) -> Capabilities; // resume, steer, structured_output, budget, reports_cost
    async fn start(&self, spec: SessionSpec) -> Result<SessionHandle>;
    async fn resume(&self, session: SessionRef, input: Prompt) -> Result<SessionHandle>;
}
// SessionHandle: Stream<Item = AgentEvent> + steer() + interrupt() + wait()
// AgentEvent: Started | Message | ToolCall{name, args_hash} | ToolResult
//           | Usage{tokens, cost} | Compaction | RateLimited | Finished
```

Adapters in order: `ClaudeCliRunner` (stream-json), `CodexExecRunner` (`exec --json`, upgrade to app-server when steering is needed), then `AcpRunner` for everything else. The engine degrades by capability (no steer means interrupt and re-prompt; no cost reporting means estimate from tokens).

Process handling: own process group (`process-wrap` 10.x); SIGINT first (Claude Code leaves the turn unfinished on SIGTERM, exit 143), then SIGTERM, then SIGKILL; minimal environment; lines up to 10 MB (Symphony); raw stream teed to `runs/<id>/events.jsonl`. Pin and record agent CLI versions, feature-detect from `system/init.capabilities`, and ship a fake-agent binary for tests (Kilroy's `test_shim`) plus a `doctor` preflight.

---

## 11. Proposed shape of the binary

One crate workspace, one binary `factory`:

- `factory daemon` — poll, reconcile, schedule, supervise.
- `factory run <issue>` — the same engine for one item in the foreground (development and debugging).
- `factory status | logs <run> | retry | cancel | resume | doctor`.

Modules: `tracker` (GitHub Issues adapter), `scheduler`, `engine` (state machine + journal, single writer), `store` (SQLite), `workspace`, `agent` (runners), `gates` (deterministic checks), `merge`, `policy` (in-repo config and prompts), `observe` (logs; optional status endpoint, never required for correctness).

One tokio task per run, supervised by the scheduler; all state changes funnel through the engine. Issue labels mirror engine state so humans can see and steer from GitHub.

Alternatives rejected, in one line each: porting Symphony unchanged (in-memory state, Codex-only, agent holds tracker credentials); implementing full Attractor first (a general graph engine is a second product); Temporal or Restate (a second server to operate); an orchestrator agent as control plane (not reproducible or cheaply crash-safe); an own agent loop on raw model APIs (highest maintenance).

---

## 12. Where evidence is thin or contested

- Productivity numbers (Symphony's sixfold merged-PR increase, Cognition's review catch rates, StrongDM's no-human-review factory) are vendor self-reports.
- Role-separation results come from small-n experiments and shift with each model release.
- OpenAI ("let them cook") and Attractor (explicit graphs) disagree on how much process to encode; the recommendation sits between them deliberately.
- Fabro (a Rust Attractor implementation) and Gas Town were only reachable through secondary pages; nothing here depends on them.
- Merge-queue availability and git-ref-as-lock semantics need verification against the live GitHub API.

---

## Sources

Orchestrators and specs
- Symphony SPEC.md — https://raw.githubusercontent.com/openai/symphony/main/SPEC.md
- Symphony README — https://raw.githubusercontent.com/openai/symphony/main/README.md
- OpenAI Symphony announcement (403 for me) — https://openai.com/index/open-source-codex-orchestration-symphony/
- The Decoder on Symphony — https://the-decoder.com/openai-says-human-attention-is-the-bottleneck-so-it-built-a-system-to-let-agents-manage-themselves/
- Attractor pipeline spec — https://raw.githubusercontent.com/strongdm/attractor/main/attractor-spec.md
- Attractor coding-agent-loop spec — https://raw.githubusercontent.com/strongdm/attractor/main/coding-agent-loop-spec.md
- Kilroy — https://github.com/danshapiro/kilroy
- Simon Willison on StrongDM's factory — https://simonwillison.net/2026/Feb/7/software-factory/
- OpenAI harness engineering (403 for me) — https://openai.com/index/harness-engineering/

Agent design evidence
- Anthropic, Building effective agents — https://www.anthropic.com/engineering/building-effective-agents
- Anthropic, Effective harnesses for long-running agents — https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents
- Anthropic, Harness design for long-running application development — https://www.anthropic.com/engineering/harness-design-long-running-apps
- Anthropic, A harness for every task: dynamic workflows — https://claude.com/blog/a-harness-for-every-task-dynamic-workflows-in-claude-code (redirects to claude.dev)
- Cognition, Multi-Agents: What's Actually Working — https://cognition.com/blog/multi-agents-working
- Cognition, Don't Build Multi-Agents (not fetched; cited for context) — https://cognition.ai/blog/dont-build-multi-agents
- CooperBench (search summaries only) — https://arxiv.org/abs/2601.13295
- AI Agent Pull Requests on GitHub (conflict rates) — https://arxiv.org/abs/2607.04697
- Secondary write-up of the above — https://codex.danielvaughan.com/2026/07/28/agent-pr-merge-conflicts-concurrent-coding-agents-codex-cli-worktree-isolation-coordination-defence/
- OpenHands stuck detector — https://docs.openhands.dev/sdk/guides/agent-stuck-detector and https://github.com/OpenHands/software-agent-sdk/issues/762

Agent invocation
- Claude Code headless — https://code.claude.com/docs/en/headless
- Claude Code CLI reference — https://code.claude.com/docs/en/cli-reference
- Claude Agent SDK overview — https://code.claude.com/docs/en/agent-sdk/overview
- Agent loop (limits, compaction, result subtypes) — https://code.claude.com/docs/en/agent-sdk/agent-loop
- Sessions — https://code.claude.com/docs/en/agent-sdk/sessions
- Worktrees — https://code.claude.com/docs/en/worktrees
- Sandboxing — https://code.claude.com/docs/en/sandboxing
- Codex non-interactive mode — https://learn.chatgpt.com/codex/non-interactive-mode (developers.openai.com/codex redirects here)
- Codex app-server — https://learn.chatgpt.com/docs/app-server
- ACP agents list — https://agentclientprotocol.com/overview/agents
- ACP session setup — https://agentclientprotocol.com/protocol/session-setup
- ACP prompt turn — https://agentclientprotocol.com/protocol/prompt-turn
- ACP SDK 1.0 announcement — https://agentclientprotocol.com/announcements/sdk-1-0-releases
- ACP Rust crate docs — https://docs.rs/agent-client-protocol/latest/agent_client_protocol/
- claude-agent-acp adapter — https://github.com/zed-industries/claude-agent-acp

GitHub and runtime
- Managing a merge queue — https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue
- Merging a pull request with a merge queue — https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/merging-a-pull-request-with-a-merge-queue
- Merge queue API changelog — https://github.blog/changelog/2023-04-19-pull-request-merge-queue-public-beta-api-support-and-recent-fixes
- Git refs REST API — https://docs.github.com/en/rest/git/refs
- Stacked PRs public preview (search result only) — https://github.blog/changelog/2026-07-31-stacked-pull-requests-are-now-in-public-preview/
- Temporal Rust SDK GA (search result only) — https://temporal.io/blog/build-durable-applications-rust-temporal-rust-sdk-now-generally-available
- Labels as locks (secondary) — https://agentpatterns.ai/workflows/labels-as-locks-pipeline/
- crates.io API, queried 2026-11-18, for `agent-client-protocol` 5.4.0, `rusqlite` 0.43.0, `sqlx` 0.9.1, `tokio` 1.54.1, `process-wrap` 10.0.1, `temporalio-sdk` 1.2.0, `restate-sdk` 0.13.0, `obelisk` 0.41.5 — https://crates.io/api/v1/crates/<name>
