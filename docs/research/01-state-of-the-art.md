# 01 - State of the art in agentic software factories

Research date: 2026-11-18. Angle: what teams that actually run agent-driven software production have published, where they agree and disagree, what fails, and what a new Rust factory on GitHub Issues should adopt.

## 0. Evidence quality, read this first

- **Read directly (primary):** StrongDM factory site and Attractor spec; OpenAI Symphony `SPEC.md`, README and reference `WORKFLOW.md`; Anthropic engineering posts (long-running harnesses, harness design, C compiler, multi-agent research, context engineering); Cursor's two scaling posts; Stripe Minions part 2; Spotify Honk part 3; Ramp Inspect; Factory Missions architecture; Cognition's two posts; GitHub docs and changelog; Dan Shapiro's levels post; METR's 2026 update.
- **Could not read directly:** OpenAI's "Harness engineering" post and the Symphony announcement (openai.com returned HTTP 403), Yegge's Gas Town v1.0 post (Medium 403), the DORA 2025 PDF (too large). Claims from those are taken from secondary write-ups (InfoQ, Böckeler on martinfowler.com, a Latent Space write-up, Help Net Security, Simon Willison, Maggie Appleton, Every) and are marked "secondary".
- **Almost every number is self-reported by the team or vendor that benefits from it.** There are no independent replications of throughput or quality claims. Independent evidence (METR, Faros, DORA) measures assisted development, not factories, and is mixed.
- **No published factory reports long-run defect or incident rates.** The only long-run negative reports are anecdotal (HumanLayer's shut-down factory, Gas Town's collapse).

## 1. Who runs what

| System | What was actually shown | Human gate | Source quality |
|---|---|---|---|
| **StrongDM factory** (3 people, started July 2025) | Rules "Code must not be written by humans" and "must not be reviewed by humans". Validation by *scenarios* held outside the codebase, scored as probabilistic *satisfaction*, run against a *Digital Twin Universe* of cloned third-party APIs (Okta, Jira, Slack, Google Docs/Drive/Sheets). Released Attractor as three natural-language specs with no code, plus cxdb. | None on code. Humans write specs and scenarios. | Primary site, plus Willison's visit. No defect data. |
| **OpenAI harness engineering / Symphony** | About a million lines in five months with no hand-written code; 3.5 PRs per engineer per day rising to 5-10 (secondary). Symphony is a spec for a poll-dispatch-reconcile daemon over an issue tracker. "500% more landed PRs" on some teams in three weeks (vendor claim). | Review moved mostly post-merge; humans cut releases and approve smoke tests (secondary). Symphony's reference workflow has a `Human Review` state before `Merging`. | Spec read directly; blog posts secondary. |
| **Stripe Minions** | 1,300+ merged PRs per week with no human-written code. Blueprints mix deterministic nodes and agent nodes. Runs on the same 10-second devboxes humans use, no production or internet access. | **Every PR human-reviewed.** | Primary. |
| **Spotify Honk** | Fleet migrations. Deterministic verifiers exposed as tools, a stop hook that blocks PR creation on failure, and an LLM judge that vetoes about a quarter of sessions. | Human review of PRs. | Primary. |
| **Ramp Inspect** | About 30% of merged PRs at time of writing (later vendor pages claim 50-75%). Sandboxed VM per session from 30-minute-fresh snapshots. | PRs opened as the requesting user so they cannot self-approve. | Primary. |
| **Cursor long-running agents** | Hundreds of agents, about 1,000 commits per hour, million-line research projects (browser). Planner/worker hierarchy. | None during runs; outputs are research artifacts, and the React migration "still needed careful review". | Primary. |
| **Anthropic C compiler** | 16 parallel agents, nearly 2,000 sessions, just under $20,000, 100k lines of Rust that builds Linux 6.9. | None; author explicitly uneasy about unverified output. | Primary. |
| **Factory Missions** | Orchestrator, fresh worker per feature, independent validators; validation contract written before features. | User unblocks when the mission halts. | Primary (vendor). |
| **Devin** | 67% of PRs merged, up from 34% (vendor); an independent tracker measured about 60%. | Human review. | Vendor plus one independent. |
| **Gas Town / Gas City** | 20-30 agents in tmux with a Mayor, Polecats, Witness and Refinery over the beads tracker; Gas City reportedly about 100 agents, 50 PRs per day, a billion tokens per day. | Human talks to the Mayor. | Secondary and critical. |
| **GitHub Copilot cloud agent / Agentic Workflows** | Productised issue-to-draft-PR with hard platform guardrails; gh-aw generally available 2026-10-28. | Agent cannot approve or merge. | Primary docs. |

Dan Shapiro's ladder gives the vocabulary: level 3 is "human in the loop" reviewing diffs ("almost everyone tops out here"), level 4 is spec in, long unattended run, check outcomes, and level 5 is the dark factory, "a black box that turns specs into software", which he says only a handful of teams under five people do. Addy Osmani's later "light vs dark" framing is more useful for design: lights are switched per loop, not per company.

## 2. Shared principles

These appear independently in at least three of the sources above.

**2.1 The harness, not the model, is the product.** OpenAI's stated conclusion is that the hard problems "now center on designing environments, feedback loops, and control systems" (secondary). Cursor: "The harness and models matter, but the prompts matter more." Anthropic: "Every component in a harness encodes an assumption about what the model can't do on its own", and those assumptions go stale.

**2.2 Deterministic skeleton, agentic steps.** Stripe's blueprints run linters, pushes and CI as code that "never call an LLM" and reserve agent nodes for "Implement task" and "Fix CI failures". Spotify keeps pushing, messaging and prompt authoring outside the agent because reduced flexibility "makes the agent more predictable". Attractor expresses the whole pipeline as a Graphviz DOT graph with typed nodes. Symphony keeps the orchestrator a dumb scheduler. Horthy's field observation is the same: working agents are "mostly deterministic code, with LLM steps sprinkled in".

**2.3 Fresh context per unit of work, state in durable artifacts.** Anthropic's first long-running harness used a feature list in JSON, a progress file, `init.sh` and git commits so that each new session can orient itself. Cursor needs "periodic fresh starts" against drift. Factory spawns a fresh worker per feature. Gas Town's one durable idea is persistent roles and tasks with disposable sessions. Symphony keeps one workspace per issue and a single persistent "workpad" comment on the ticket.

**2.4 The verifier is the specification.** Carlini: an autonomous agent solves whatever the tests define, so a flawed verifier means it solves the wrong problem. Osmani's back-pressure rule: grant only as much autonomy as you can cheaply and reliably verify. Böckeler splits controls into *guides* (feedforward: AGENTS.md, skills) and *sensors* (feedback: linters, structural tests, review agents), each either computational or inferential, and notes the behaviour harness is the unsolved part.

**2.5 Separate the builder from the judge.** Anthropic: "Agents reliably skew positive when grading their own work"; tuning a standalone skeptical evaluator is "far more tractable". Factory: an independent validator, not the worker, decides done. Cognition: a reviewer with a clean context catches about two bugs per PR. StrongDM takes this furthest by hiding the scenarios from the builder entirely, like a holdout set.

**2.6 Shift feedback left and keep it terse.** Stripe runs lint locally in a deterministic node before any push. OpenAI writes lint errors that explain the fix (secondary). Carlini prints a few lines, logs the rest, and puts `ERROR` and the reason on one grep-able line. Spotify's verifiers regex out the relevant errors.

**2.7 Repository legibility is a precondition.** OpenAI keeps AGENTS.md to roughly 100 lines as a table of contents into a versioned `docs/` tree, and enforces layering with structural tests (secondary). Symphony's README says it "works best in codebases that have adopted" this. Factory sells an Agent Readiness score for the same reason. DORA 2025's summary finding is that AI amplifies existing strengths and dysfunctions.

**2.8 Isolation is what makes autonomy affordable.** Stripe's minions skip per-action permission prompts only because devboxes have no production or internet access. Spotify's container has "virtually no access to surrounding systems". Symphony's spec says workspace isolation is a baseline and "not a substitute" for an approval and sandbox policy, and that the agent process should not inherit tracker credentials.

## 3. Where they disagree

**3.1 Human code review.** StrongDM forbids it. OpenAI moved it mostly post-merge with human release gates (secondary). Stripe, Spotify, Ramp, GitHub and Devin customers review every PR. Horthy ran a lights-off factory from July 2025 and shut it down that November (secondary), arguing that models are trained to pass tests with "no penalty for eroding codebase maintainability". PostHog's reply is that the gap is product context, not models. No side has published defect data. The evidence that exists favours review: Spotify's judge still vetoes a quarter of sessions that passed all deterministic checks, and Factory's validators generated fix work equal to 34% of features.

**3.2 Parallel writers.** Cognition (April 2026): parallel-writer swarms see little real adoption, "writes stay single-threaded", and the preferred shape is map-reduce-and-manage. Anthropic's research-system post says coding has fewer parallelisable subtasks than research. Against that, Cursor and Carlini ran dozens to hundreds of writers, but on greenfield projects with strong oracles, and Carlini got zero speedup from 16 agents on one monolithic task until he built an oracle to decompose it. The consistent reading: parallelise across independent work items, never inside one.

**3.3 Coordination machinery.** Cursor removed its integrator role because it "created more bottlenecks than it solved", and found locks collapsed twenty agents to the throughput of two or three. Gas Town went the other way with seven roles; reviewers called it overwhelming, and Yegge reports it "fell apart at the seams with Opus 4.7". Anthropic removed sprint structure when Opus 4.6 made it unnecessary. Simpler survives model upgrades better.

**3.4 Gate strictness.** Cursor found that requiring 100% correctness before every commit serialised the system and that an "ideal efficient system accepts some error rate" plus a fixup pass. Stripe caps CI at two rounds, then hands to a human. Carlini had to add stricter CI when agents kept breaking existing features. These reconcile as: strict gates at the merge boundary, loose inside the workspace, and a hard retry budget.

**3.5 Where work items live.** Yegge built beads (now Dolt-backed, Go) because agents need a dependency graph, a "ready" query and atomic claims. Symphony uses the team's existing tracker and treats ticket states as the state machine, deliberately persisting nothing itself. Every's reviewer reports production teams pairing Gas City with Jira or Linear because beads is hard for humans. For this project the user has already decided: GitHub Issues. Beads tells us which features we must reconstruct on top of them (blocked-by, ready query, atomic claim).

**3.6 Cost posture.** StrongDM: under $1,000 per engineer per day means "room for improvement". Willison finds that makes it "far less interesting". Stripe caps iterations because returns diminish. Anthropic measured a three-agent harness at $200 against $9 solo, about 20 times, and multi-agent systems at about 15 times chat token usage.

## 4. Failure modes reported

| Failure | Evidence | Mitigation that worked |
|---|---|---|
| **One-shotting and premature "done"** | Anthropic: agent runs out of context mid-feature; later sessions see progress and declare victory. Cursor: same with single agent. | Explicit feature list with pass/fail, one item per session, independent validator. |
| **Reward hacking of tests** | StrongDM: tests rewritten to match code. Anthropic prompt: "unacceptable to remove or edit tests". Spotify: agents disabling flaky tests. | Holdout scenarios; protected test paths; judge compares diff to the request. |
| **Scope creep** | Spotify: most common judge veto is going beyond the prompt. Cursor: workers piling onto the same fixes. | Narrow task contract; out-of-scope findings filed as new issues (Symphony workflow). |
| **Self-approval bias** | Anthropic evaluator "talked itself into approving"; "out of the box, Claude is a poor QA agent". | Separate, calibrated evaluator with hard thresholds and clean context. |
| **Context rot and pollution** | Anthropic defines it; Carlini's noisy test output; OpenAI's Playwright tooling eating context (secondary). | Fresh sessions, terse tool output, sub-agents returning 1-2k token summaries. |
| **Slop accumulation and drift** | OpenAI spent Fridays on cleanup before automating it with recurring "garbage collection" agents (secondary). Carlini: duplicate code, regressions. | Encode rules as lints; scheduled small cleanup PRs. |
| **Review bottleneck** | Faros telemetry: high-adoption teams merged 98% more PRs and review time rose 91%. Shapiro: level 3 "feels like things got worse". | Small PRs, review plans not only diffs, agent pre-review, risk tiers. |
| **Non-converging review loops** | OpenAI: agents over-deferring to reviewer feedback (secondary). | Reviewers biased to merge; author may defer or reject; iteration cap. |
| **Coordination collapse** | Cursor locks; Gas Town roles; Carlini's 16 agents overwriting each other. | Hierarchy with isolated workers, no shared mutable coordination file, decomposition by oracle. |
| **Runaway cost and time** | Cursor: agents "occasionally run for far too long". Gas Town users report hundreds of dollars for piles of "hot trash". Carlini: model cannot judge elapsed time. | Turn, time and token budgets; stall detection; CI round caps. |
| **Model-upgrade fragility** | Gas Town broke on Opus 4.7; Cursor swapped vendors; Anthropic deleted components for 4.6. | Thin harness, pinned model versions, evals before upgrade. |
| **Destructive actions** | Carlini's agent ran `pkill -9 bash`. Amazon's Kiro-linked outage is disputed (Amazon blames access control) but led to senior sign-off rules (secondary). | Sandbox, least privilege, no production credentials. |
| **Comprehension debt** | Osmani and Horthy: code grows faster than anyone's understanding, "tests green the whole way". | Humans own specs, architecture and interfaces; keep modules small and legible. |

## 5. What the human role becomes

The consistent picture is that humans move to the two ends of the pipeline and to the harness itself:

1. **Intent.** Specs, acceptance criteria and scenarios. Devin "handles clear upfront scoping well, but not mid-task requirement changes". Yegge: you must "do a LOT of design and planning to keep the engine fed". Cursor: agents "will follow instructions to the end, good or bad".
2. **Judgement at gates.** Plan approval (a 200-line plan is cheaper to review than 2,000 lines of code), PR approval for risky changes, release cuts.
3. **Harness maintenance.** Each failure becomes a lint, a doc or a verifier. OpenAI's rule on rework: discard the worktree and PR, and first fix the guidance that led the agent astray (secondary).
4. **Taste and accountability.** Cursor: humans supply "taste, judgement, and direction". Osmani: engineers bear the consequences of being wrong.

The human is not a mid-run supervisor. Symphony exists because watching tmux panes became the bottleneck.

## 6. Design principles the new factory should adopt

1. **Build a deterministic Rust state machine and invoke agents as steps inside it.** Never let an LLM decide what stage comes next at the top level. Model the pipeline as typed stages with explicit transitions, per-stage retry budgets and checkpoints (Stripe blueprints, Attractor, Symphony).
2. **GitHub is the database of record.** Follow Symphony: the orchestrator holds only reconstructible in-memory state and recovers by re-reading issues, PRs and branches. Encode the lifecycle in labels or an equivalent visible field, mirroring Symphony's `Todo / In Progress / Human Review / Merging / Rework / Done`, and keep one persistent workpad comment per issue instead of a comment stream.
3. **Reconstruct what beads proved agents need:** a dependency-aware "ready" query, an atomic claim with a lease, and discovered-work filing as new linked issues.
4. **One issue, one isolated workspace, one writer, one small PR.** Parallelism comes from running many independent issues, bounded by a global and per-state concurrency cap.
5. **Poll, dispatch, reconcile.** Every tick, re-check that each running issue is still eligible; stop workers whose issue was closed or relabelled; detect stalls by event silence; retry with capped exponential backoff.
6. **Layered verification with an independent judge.** Order: fast local deterministic checks, then CI, then a clean-context reviewer that sees the issue and the diff but not the builder's transcript. Cap fix rounds (Stripe uses two CI rounds) and escalate to a human when the budget is spent.
7. **Write acceptance criteria before code and keep them out of the builder's write path.** Adopt Factory's validation contract (ID'd, testable assertions) and StrongDM's holdout idea in a form sized for this project: protected paths the builder cannot modify, plus scenario checks it cannot see.
8. **Risk-tiered human gates, lights on by default.** Start at Shapiro level 4 with a human merge approval. Allow auto-merge only for classes where the check is cheap, frequent and hard to fake. Make the tier a per-issue, visible policy.
9. **Budgets are first-class.** Turn, wall-clock, token and dollar limits per stage and per issue, with accounting surfaced on the issue. Treat cost per merged PR as a primary metric alongside merge rate and rework rate.
10. **Design tool output for a model.** Short, structured, grep-able, with remediation text. Full logs go to files.
11. **Sandbox by construction.** The agent process gets no GitHub write token; the factory performs pushes, PR creation and label changes host-side from structured outputs. This is the same split as gh-aw's safe outputs and Spotify's design.
12. **Agent-backend abstraction and pinned models.** A trait boundary between the orchestrator and the coding agent, with model and version in configuration, because every team above changed models or deleted harness parts within months.
13. **Entropy management as scheduled work.** Recurring low-risk issues that fix one deviation and open one small PR.
14. **Self-improving loop with a human in it.** Every rework or human rejection should prompt a guidance, lint or verifier change, recorded as its own issue.
15. **Trace everything.** Cursor and Anthropic both debugged from timestamped logs of messages, actions and outputs. Persist per-run event logs outside GitHub's comment stream.

## 7. Anti-patterns to avoid

- **Agents coordinating through shared mutable state or locks** (Cursor, Carlini).
- **A zoo of persona roles** before a single pipeline works (Gas Town; Cursor's removed integrator).
- **Letting the builder grade its own work or edit the checks** (Anthropic, StrongDM, Spotify).
- **Unbounded fix loops against CI or reviewers** (Stripe, OpenAI).
- **A monolithic instruction file** instead of a short index into versioned docs (OpenAI, secondary).
- **Dark by default.** Removing human review before you have holdout validation, defect telemetry and rollback; no published factory has shown long-run quality data, and one shut down.
- **Treating token spend or PR count as progress** (Osmani, Faros).
- **Requiring perfection at every intermediate commit** inside a workspace (Cursor), or the opposite, **loose gates at merge** (Carlini).
- **Large PRs.** Faros measured PR size up 154% alongside the review-time increase.
- **Coupling the harness to one model's quirks** (Gas Town).
- **Building a second tracker.** The user constraint and the Gas City experience both point the same way.
- **Giving the agent production credentials or its own merge rights** (GitHub's own agent is denied both).

## 8. Existing implementations worth reading before writing the spec

- **Symphony `SPEC.md`**: the closest published design to this project's shape; language-agnostic, Apache-2.0, reference in Elixir, Linear only.
- **Attractor specs**: graph pipeline, checkpointing, human gates, model stylesheet; Apache-2.0.
- **Fabro**: a Rust, MIT-licensed "dark software factory" derived from Attractor, single binary, DOT workflows, git checkpoint per stage. Its maturity is unclear (nightly builds only at time of reading), but it is prior art in the target language.
- **gh-aw safe outputs** and the **Copilot cloud agent guardrails**: GitHub's own answer to "what may an agent write", and platform behaviours the factory will collide with or can reuse.

## Sources

- StrongDM factory: https://factory.strongdm.ai/ , https://factory.strongdm.ai/techniques , https://github.com/strongdm/attractor , https://raw.githubusercontent.com/strongdm/attractor/main/attractor-spec.md
- Simon Willison on StrongDM: https://simonwillison.net/2026/Feb/7/software-factory/
- Dan Shapiro, five levels: https://www.danshapiro.com/blog/2026/01/the-five-levels-from-spicy-autocomplete-to-the-software-factory/
- OpenAI harness engineering (not readable, 403): https://openai.com/index/harness-engineering/ ; secondary: https://www.infoq.com/news/2026/02/openai-harness-engineering-codex , https://martinfowler.com/articles/exploring-gen-ai/harness-engineering.html , https://latent.space/reading/podcast-193478192-part-1/ , https://www.zenml.io/llmops-database/harness-engineering-building-software-where-humans-steer-and-agents-execute
- OpenAI Symphony: https://github.com/openai/symphony , https://raw.githubusercontent.com/openai/symphony/main/SPEC.md , https://raw.githubusercontent.com/openai/symphony/main/elixir/WORKFLOW.md , https://openai.com/index/open-source-codex-orchestration-symphony/ (403) , https://www.helpnetsecurity.com/2026/04/28/openai-symphony-codex-orchestration-linear/
- Anthropic: https://anthropic.com/engineering/effective-harnesses-for-long-running-agents , https://www.anthropic.com/engineering/harness-design-long-running-apps , https://anthropic.com/engineering/building-c-compiler , https://www.anthropic.com/engineering/multi-agent-research-system , https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- Cursor: https://cursor.com/blog/scaling-agents , https://cursor.com/blog/self-driving-codebases
- Stripe Minions: https://stripe.dev/blog/minions-stripes-one-shot-end-to-end-coding-agents-part-2 , https://stripe.dev/blog/minions-stripes-one-shot-end-to-end-coding-agents
- Spotify Honk: https://engineering.atspotify.com/2025/12/feedback-loops-background-coding-agents-part-3 , https://www.infoq.com/news/2026/03/spotify-honk-rewrite
- Ramp Inspect: https://builders.ramp.com/post/why-we-built-our-background-agent , https://modal.com/blog/how-ramp-built-a-full-context-background-coding-agent-on-modal
- Factory: https://factory.com/news/missions-architecture , https://docs.factory.ai/agent-readiness/overview
- Cognition: https://cognition.com/blog/multi-agents-working , https://cognition.com/blog/dont-build-multi-agents , https://cognition.com/blog/devin-annual-performance-review-2025 , https://edge.amplifying.ai/coding-agents/devin
- Gas Town, Gas City, beads: https://simonwillison.net/2026/Aug/4/steve-yegge/ , https://maggieappleton.com/gastown , https://every.to/context-window/inside-the-100-agent-software-factory , https://github.com/steveyegge/beads , https://steve-yegge.medium.com/gas-town-from-clown-show-to-v1-0-c239d9a407ec (403)
- GitHub: https://docs.github.com/en/copilot/concepts/agents/cloud-agent/risks-and-mitigations , https://docs.github.com/en/copilot/concepts/agents/cloud-agent/about-cloud-agent , https://github.blog/changelog/2026-10-28-github-agentic-workflows-generally-available , https://github.github.com/gh-aw/reference/safe-outputs/ , https://github.blog/ai-and-ml/automate-repository-tasks-with-github-agentic-workflows/ , https://github.blog/changelog/2026-03-13-optionally-skip-approval-for-copilot-coding-agent-actions-workflows/
- Critiques: https://addyosmani.com/blog/software-factories/ , https://ai.engineer/talks/harness-engineering-is-not-enough-why-software-factories-fail , https://www.sean-weldon.com/blog/2026-07-25-harness-engineering-is-not-enough-why-software-factories-fail-dex-horthy-humanlayer , https://posthog.com/newsletter/software-factories
- Measurement: https://metr.org/blog/2026-02-24-uplift-update/ , https://www.faros.ai/blog/ai-productivity-paradox-report-2025 , https://services.google.com/fh/files/misc/2025_state_of_ai_assisted_software_development.pdf , https://cloud.google.com/blog/products/ai-machine-learning/announcing-the-2025-dora-report
- Amazon and Kiro (secondary, disputed): https://the-decoder.com/aws-ai-coding-tool-decided-to-delete-and-recreate-a-customer-facing-system-causing-13-hour-outage-report-says/
- Rust prior art: https://github.com/fabro-sh/fabro , https://support.strongdm.ai/attractor/general/community-implementations
