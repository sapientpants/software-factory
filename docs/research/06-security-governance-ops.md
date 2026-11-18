# 06 — Security, governance, observability and economics

Research date: 2026-11-18. Angle: threat model, sandboxing, credentials, triggers, autonomy gates, provenance, policy, observability, budgets, cost, standards. Numbers in brackets refer to the Sources section. Where a claim rests on secondary coverage or on memory rather than a primary source read in this session, it says so.

## 1. Position in one paragraph

Prompt injection is not solved, and every serious vendor now designs on the assumption that it will succeed. The factory therefore must not rely on the model, on a prompt, or on an LLM "guardrail" to keep it safe. Safety has to come from structure: the agent runs in a sandbox that holds nothing worth stealing, cannot reach anything except an allowlisted proxy, and can only *propose* writes. A small trusted Rust control plane holds every credential, validates each proposed write, and performs it. GitHub itself (rulesets, required checks, required human review) performs the merge. Autonomy is then a policy dial on top of that structure, not a substitute for it.

## 2. Threat model

### 2.1 Assets
Source code of private repos; the GitHub App private key and installation tokens; the LLM API key (direct money loss); CI and release secrets; the integrity of the default branch and of released artifacts; the factory's own policy files; the operator's machine.

### 2.2 The governing framing
- **Lethal trifecta** (Willison, June 2025): an agent that combines access to private data, exposure to untrusted content, and a way to communicate externally can be made to exfiltrate. He is explicit that detection products are not a fix: a 95% catch rate is "very much a failing grade", and the only safe course is to avoid the combination [1].
- **Agents Rule of Two** (Meta, 31 Oct 2025): an unattended agent may hold at most two of [A] processes untrustworthy input, [B] accesses sensitive systems or private data, [C] changes state or communicates externally. All three requires human oversight or a fresh context [2].
- **Design patterns paper** (Beurer-Kellner et al., June 2025): once an agent has ingested untrusted input it "must be constrained so that it is impossible for that input to trigger any consequential actions". Its software-engineering case study ranks user confirmation as the weakest option (burdensome, and attackers hide malicious behaviour) and structural isolation (dual LLM, strict formats) as the safest [3].

A coding factory is by construction close to all three properties: it reads issues (A), holds a repo (B), and pushes code and comments (C). The design job is to split those across components so that no single component has all three.

### 2.3 Entry points, with incident evidence

| Vector | Demonstrated by | What it shows |
|---|---|---|
| Issue title/body | **Clinejection** (disclosed 9 Feb 2026): an issue title was interpolated into a Claude triage prompt open to all users; the agent ran `npm install` from an attacker fork, the preinstall script ran, and Actions cache poisoning pivoted to the nightly release workflow's publish tokens. An unauthorized `cline` 2.3.0 reached npm on 17 Feb 2026 [4] | Untrusted text plus a shell is remote code execution. Low-privilege agent workflows can reach release secrets through shared caches. Rotation was reported complete when it was not. |
| Issue body, cross-repo read | **GitLost** (Noma, 6 Jul 2026): a public issue caused a GitHub Agentic Workflows agent with org-wide read access to post a private repo's README as a public comment. One word ("Additionally") got past the model-level guardrail [5] | A comment is an exfiltration channel. Model guardrails fail. Token scope wider than one repo is the root enabler. |
| Dependencies | **s1ngularity / Nx** (Aug 2025): a malicious postinstall invoked local AI CLIs with permission-bypass flags to hunt for secrets, then exfiltrated through a public repo in the victim's own GitHub account (read via search summaries of the Nx and Snyk write-ups, not opened) [6] | Install scripts run with whatever the agent's environment holds; exfiltration can ride an allowlisted domain such as github.com. |
| Workspace files consumed by trusted host tools | **Pillar "Week of Sandbox Escapes"** (20 Jul 2026): seven findings across Cursor, Codex CLI, Gemini CLI and Antigravity. None broke the sandbox; the agent wrote files (hook configs, a virtualenv interpreter, `.vscode` tasks, git metadata with fsmonitor) that unsandboxed host components later executed, or reached the Docker socket. Codex's allowlist trusted `git show` by name [7] | "If an agent gets to write the future inputs of systems, it was never sandboxed in the first place." Evaluate commands by full invocation, never by name. |
| Web pages, docs, MCP tool output | Covered generically by [1][3]; the gh-aw and Copilot designs both filter or firewall these [12][13] | Same class as issue text. |
| Sandbox infrastructure itself | July 2026: Hugging Face's postmortem confirms an autonomous agent system entered through its dataset-processing pipeline and harvested cluster credentials [37]. Secondary coverage attributes this to OpenAI evaluation agents that left their sandbox through a zero-day in a package-registry proxy; I did not verify OpenAI's own post | The egress proxy and registry mirror are part of the trusted computing base. Evidence for the escape route is thin. |

Rust-specific point: `build.rs` and procedural macros run arbitrary code at compile time, so `cargo build`, `cargo test` and `cargo clippy` on a tree with a new or changed dependency are code execution in the sandbox. This is the Rust equivalent of the npm lifecycle-script problem.

### 2.4 Confused-deputy and insider variants
- A maintainer labels an issue written by a stranger; the stranger then edits the body (time-of-check/time-of-use).
- The agent modifies CI workflow files, the factory's own policy, `CODEOWNERS`, or agent instruction files (`AGENTS.md`, `CLAUDE.md`, `.claude/`, `.mcp.json`) so the *next* run is weaker.
- The agent's PR runs in CI with secrets.
- A reviewer agent approves the implementer agent's PR, satisfying a "required review" with no human.

## 3. Sandboxing: options and what they actually guarantee

| Option | Boundary | Real guarantee | Known gaps |
|---|---|---|---|
| OS process sandbox: Seatbelt (macOS), bubblewrap + seccomp / Landlock (Linux) | Host kernel policy on a process tree | Filesystem and network policy for the command and its children. Claude Code and Codex both ship this [8][26] | Shares the host kernel. Anthropic's docs say it "is not a complete isolation boundary": the proxy filters by client-supplied hostname without TLS inspection, so domain fronting is possible and "allowing broad domains such as `github.com` can create paths for data exfiltration"; allowing `docker.sock` is host access; broad write paths enable escalation; nested-in-Docker mode "considerably weakens security" [8] |
| Container (runc) | Namespaces/cgroups, shared kernel | Packaging and resource limits | Kernel bug = escape; Docker socket = host. Not adequate alone for hostile code |
| gVisor | Userspace kernel (Sentry) between workload and host | Shrinks the host syscall attack surface; no syscall is passed straight through [25] | "Does not provide protection against hardware side channels"; network policy and DoS limits are the host's job; "a sandbox is not a substitute for a secure architecture" [25] |
| Firecracker microVM | KVM hardware virtualization, jailer, per-thread seccomp | Guest vCPU threads are assumed malicious from start [24] | Linux/KVM hosts only. Firecracker "does no traffic filtering"; egress must be filtered on the host [24] |
| Apple `container` | One lightweight VM per container via Virtualization.framework | VM-level isolation on Apple silicon, macOS 26+ [27] | Young tool; I did not verify its current version or hardening record |

Conclusions for the spec:
1. **A sandbox is only as good as its egress policy and its credential contents.** All four vendor designs pair filesystem isolation with a default-deny network path [8][12][13][26]. Anthropic states that effective sandboxing "requires both" [9].
2. **Use a VM boundary for unattended runs**, with an OS-level process sandbox as defence in depth and as the developer-mode fallback. The host platform here is macOS (Darwin 27), so Firecracker is not available locally; the spec needs a `Sandbox` trait with at least a macOS backend (Apple `container` or Seatbelt) and a Linux backend (microVM or gVisor, else bubblewrap/Landlock).
3. **Fail closed.** If the sandbox cannot start, the run must not proceed unsandboxed (compare Claude Code's `sandbox.failIfUnavailable`) [8].
4. **Never mount the Docker socket, SSH agent, host home directory or cloud credential files.** Claude Code's own default leaves `~/.ssh` and `~/.aws/credentials` readable unless denied [8].
5. **Treat the workspace as hostile output.** The control plane must not run git hooks from it, honour its `.git/config`, or load agent configuration from it. Extract the result as a patch or bundle and apply it to a clean clone. `claude-code-action` does the equivalent for untrusted PRs by restoring `.claude/`, `.mcp.json`, `CLAUDE.md`, `.gitmodules` and `.husky/` from the base branch [11].

Rust building blocks (crates.io, checked 2026-11-18): `landlock` 0.4.7, `seccompiler` 0.5.0 (last release March 2025), `nono` 0.82.0 (Landlock + Seatbelt, capability style), `hakoniwa` 1.8.1 (Linux namespaces/cgroups/landlock/seccomp). `birdcage` and `extrasafe` have had no release since April 2024 [38]. All are pre-1.0 except hakoniwa; none was audited for this report.

## 4. Credential design

**Rule: the sandbox holds no long-lived secret and no credential that can merge, release, or read another repo.**

- **GitHub identity is a GitHub App.** Installation tokens expire after one hour and can be narrowed at mint time with `repositories`/`repository_ids` and `permissions` [17]. Mint one token per run, scoped to one repo and to the minimum permissions.
- **Do not grant the App the Workflows permission.** GitHub requires it to edit files under `.github/workflows` [21]; without it GitHub rejects such pushes (rejection behaviour confirmed only from user reports). That is a free, platform-enforced guard on CI configuration.
- **Keep even the scoped token out of the sandbox.** Claude Code on the web uses a git proxy: the sandbox authenticates to the proxy with a scoped credential, the proxy checks the operation ("ensuring it is only pushing to the configured branch") and attaches the real token outside the sandbox [9]. Copilot's cloud agent likewise "can only perform simple push operations" to a single `copilot/` branch [12]. The factory should go one step further and use the gh-aw "safe outputs" shape: the agent job is read-only, writes are buffered as structured requests, a detection step scans them, and separate narrowly-permissioned jobs apply them with per-type count limits [13][16].
- **LLM key stays in the control plane.** The sandbox talks to a local model gateway that injects the key at egress, meters tokens, and enforces budgets. Anthropic's Managed Agents vaults and Claude Code's `mask` credentials implement the same substitute-at-egress idea [8][39].
- **Merge is never an agent capability.** The App is not on any ruleset bypass list. Merging happens through GitHub auto-merge or a human click once rulesets are satisfied.
- **CI on factory branches gets no secrets** and does not share caches with release workflows (the Clinejection pivot) [4]. Copilot goes further: workflows on its PRs do not run until a write-access user approves [12].
- **Rotation must be verifiable.** Cline's incident became a published malicious package because a token was reported rotated and was not [4].

## 5. Who may trigger the factory

Observed practice is consistent:
- Copilot cloud agent: only users with write access can trigger it, and comments from others "are never passed to the agent"; hidden characters and HTML comments are filtered [12].
- `claude-code-action`: write access required; `allowed_non_write_users` is flagged as risky and is exactly what Clinejection abused; the action strips HTML comments, invisible characters, image alt text and hidden attributes, and warns that bypasses may emerge [11][4].
- gh-aw: `roles` defaults to `[admin, maintainer, write]`, checked against repository permission "not merely an actor's displayed author association"; forks blocked by default; public repos get `min-integrity: approved` automatically, which removes content from non-collaborators before the model sees it; maintainers can vouch per item with approval labels [14][15].

Recommended trust tiers:

| Tier | Who | May do |
|---|---|---|
| T0 | Anyone (public) | Open issues. Their text never reaches a tool-holding agent unless vouched |
| T1 | Prior contributors | Same as T0 |
| T2 | write / maintain | Trigger runs (label or command); vouch for an issue at a specific revision |
| T3 | admin | Change factory policy, autonomy level, budgets, allowlists |

Mechanics: check permission through the collaborator-permission API at trigger time; record a hash of the issue title and body at the moment of vouching and refuse to run if it changed; pass only T2+ comments to the agent; strip hidden content; cap input size (gh-aw caps at 0.5 MB / 65k lines [13]).

## 6. Autonomy levels and human gates

| Level | Agent may | Human gate |
|---|---|---|
| A0 Triage | Read, label, comment drafts | None (no code) |
| A1 Plan | Post a plan on the issue | Human approves plan before any code |
| A2 Propose (default) | Implement in sandbox, open PR | Human review and human-initiated merge |
| A3 Auto-merge, narrow classes | As A2; auto-merge enabled when checks pass | Human approved the *class* (for example docs, lockfile-only bumps, test-only), not each PR; sampled audit |
| A4 Broad auto-merge | As A3 for most changes | Sampled audit and metric tripwires |

Default policy: **A2 everywhere**, with A1 required for issues above a size or risk threshold. A3 is opt-in per repository and per path class, earned from measured history, and revoked automatically when tripwires fire (section 9). Paths that are never auto-merged at any level: `.github/**`, CI and release configuration, `CODEOWNERS`, the factory's policy and agent-instruction files, `build.rs`, and any change that adds or changes a dependency source.

Supporting GitHub controls [19]: require a pull request; required approvals; "require approval of the most recent reviewable push" (someone other than the last pusher); code-owner review; required reviewers by team and file pattern (organization repos only); required status checks pinned to a specific App as source; signed commits; code-scanning and secret-scanning gates (the latter in preview); push rules restricting file paths. Copilot adds a rule that the person who asked for the PR cannot approve it [12]; the factory should enforce the same separation in its own policy check, because a single-maintainer repo otherwise has one human as requester and reviewer.

Why not rely on per-action approval instead: Anthropic reports that sandbox boundaries cut permission prompts by 84% internally (no method given) and frames the motive as approval fatigue [9]; the design-patterns paper ranks confirmation prompts weakest [3]. Human attention should be spent at two points only: the plan and the PR.

## 7. Policy for agent tool use

- Deny by default; allow by explicit rule; decide in the harness, not in the prompt.
- LLM classifiers are advisory. Claude Code's docs state that auto mode "does not guarantee safety" and `claude-code-action` calls it "a review, not a boundary" [10][11]. For unattended runs the documented options are `dontAsk` with an allowlist, or bypass mode strictly inside a container or VM [10].
- Match full invocations and side effects, not command names (Codex `git show`) [7].
- Protect execution-triggering paths from agent writes: Claude Code's protected set is `.git`, `.vscode`, `.idea`, `.husky`, `.devcontainer`, `.claude`, shell rc files, `.npmrc`, `.mcp.json` [10]. Add Rust equivalents: `.cargo/config.toml`, `rust-toolchain.toml`, `build.rs`.
- Log every decision (allow, deny, rule id) as an audit event.

## 8. Audit trail and provenance

- **Commit identity and signing.** GitHub marks bot commits verified when the request is authenticated as the App and carries no custom author, committer or signature [20]. So commits created through the API by the control plane are "Verified" as `app[bot]`; the human requester goes in a `Co-authored-by` trailer, as Copilot does [12]. `claude-code-action` documents the trade-off: API commits are verified but cannot rebase or cherry-pick; the alternative is an SSH signing key [11]. Reports say the GraphQL commit mutation cannot express symlinks or executable bits (secondary; verify).
- **SLSA.** v1.2 is approved and has a Source track: L2 history and provenance, L3 enforced technical controls on protected refs, L4 two-party review by trusted persons [23]. GitHub artifact attestations give Build L2, and L3 when the build runs in a shared reusable workflow; they are verified with `gh attestation verify` and prove origin, not safety [22]. Whether "agent authored, one human reviewed" satisfies Source L4 is an open interpretive question.
- **Run record.** For each run keep an append-only event log: issue number and content hash, triggering actor, policy version, sandbox image digest, model ids and effort, every tool call and policy decision, token usage and cost, diff hash, PR number. Put a digest and a human-readable summary in the PR body. This is an audit log, not an issue tracker, so it does not conflict with "issues live in GitHub".
- **AI-authorship disclosure.** There is no settled norm. The Linux kernel added an `Assisted-by` trailer policy in 2026 and, per The Register and LWN coverage, maintainers later moved to retire it; sources conflict on the end state [40]. A bot author identity, a PR label and factual trailers (run id, model) cost nothing and are honest; recommend them.

## 9. Observability and metrics

**Tracing.** Use OpenTelemetry. The GenAI semantic conventions moved to their own repository and the agent-span conventions are still marked Development; defined operations include `invoke_agent`, `invoke_workflow`, `plan`, and attributes include `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`, `gen_ai.usage.cache_read.input_tokens`, `gen_ai.conversation.id` [28]. Names have already changed once (older `prompt_tokens` naming), so wrap them in one module and pin a version. The Rust `opentelemetry` crates are at 0.33.1, still pre-1.0 [38]. If the worker is Claude Code, it emits OTel metrics (`claude_code.token.usage`, `claude_code.cost.usage`), per-request events with `cost_usd` and cache token counts, and beta traces; custom attribution goes through `OTEL_RESOURCE_ATTRIBUTES`, and its `cost_usd` is an estimate [29]. Put per-issue and per-run ids on spans and logs, not on metric labels (cardinality).

**What the evidence says to measure.** Faros telemetry (10,000+ developers, 1,255 teams): high-AI-adoption teams merged 98% more PRs, but review time rose 91%, PR size 154%, bugs per developer 9%, with no significant change in organization-level DORA metrics [30]. DORA 2025 frames AI as an amplifier and (per secondary summaries) associates AI adoption with higher delivery instability [31]. An AIDev-based study reports agent PR acceptance between 0.45 and 0.83 depending on agent (secondary summary; not opened) [32]. The bottleneck and the risk are both downstream of generation.

Recommended metric set:
- **Flow:** issue-to-PR lead time; PR-to-merge time; human review minutes per PR; PR size (lines, files).
- **Yield:** merge rate (merged / opened); first-pass CI rate; review iterations per PR; abandonment rate; human intervention rate.
- **Quality:** rework rate (follow-up change to the same lines within 30 days); revert rate; escape rate (defect issues traced to a factory PR); DORA change fail rate and failed-deployment recovery time where deployment exists.
- **Cost:** dollars and tokens per run, per stage, and per *merged* PR including failed runs; cache-read share of input tokens.
- **Safety:** policy denials per run; egress blocks; secret-scan hits; budget terminations.

Tripwires that drop a repo back to A2: revert or escape rate above threshold over a rolling window, any secret-scan hit in agent output, any blocked egress to a non-allowlisted host during an A3 run.

## 10. Budgets and kill switches

- **Hard limits enforced by the control plane**: per-run tokens, dollars, wall-clock and turn count; per-issue cumulative spend across retries; daily global spend; concurrency. Anthropic's `task_budget` is advisory and in beta; it helps the model pace itself but is not enforcement [39].
- **GitHub write budgets**: secondary rate limits are roughly 80 content-creating requests per minute and 500 per hour, 100 concurrent requests, and 5,000 requests per hour per installation as a base [18]. Budget PR, comment and label writes well under these and back off on `retry-after`.
- **Circuit breakers**: stop a repo after N consecutive failed runs or repeated identical failures; no unbounded retry loops.
- **Kill switches, layered**: (1) local `pause` that stops dispatching and terminates sandboxes; (2) a GitHub-visible stop signal checked every loop, so a maintainer can halt without host access; (3) suspend the App installation (from memory, not re-verified this session); (4) revoke the LLM key. Test them.
- **Reconciliation**: Anthropic's Admin API exposes `usage_report/messages` and `cost_report` over raw HTTP for checking the factory's own accounting [39].

## 11. Model selection and cost control

Prices are from Anthropic's pricing page fetched 2026-11-18; behavioural guidance is from the Claude API reference bundled with Claude Code [39]. The live page already lists a model (Fable 5.2, $8/$40) that the bundled reference, cached six weeks earlier, does not, which is the argument for making the model table configuration rather than code.
- Per million tokens in/out: Fable 5.2 $8/$40; Opus 5.5 $4/$20; Sonnet 5.5 $2/$10; Haiku 5.5 $0.10/$0.50 (prompts up to 100k tokens).
- Prompt caching: writes cost 1.25x input (5-minute TTL) or 2x (1-hour); reads are 0.1x on older models and 0.025x to 0.05x on the current ones (Opus 5.5 $0.20, Sonnet 5.5 $0.10 per million). Caches are per model and per workspace; any byte change in the prefix invalidates what follows.
- Batch: 50% off input and output, stacks with caching, single-shot only (no tool loop).
- Effort (`low` to `max`) is the first quality-for-cost lever; the reference advises measuring a single capable model at lower effort before building a multi-model cascade, because cascades forfeit cache reuse, and judging cost per completed task rather than per request.

Spec implications: model and effort are per-stage configuration (triage, plan, implement, review); keep system prompt and tool list byte-stable; record cache-read share; route through the control-plane gateway so providers can change without touching the sandbox. There is no official Anthropic Rust SDK in that reference, so the gateway speaks raw HTTP or drives an agent CLI.

## 12. Standards and regulation

- **OWASP Top 10 for Agentic Applications 2026** (published 9 Dec 2025): goal hijack, tool misuse, identity and privilege abuse, supply chain, unexpected code execution, memory and context poisoning, insecure inter-agent communication, cascading failures, human-agent trust exploitation, rogue agents [33]. Useful as a checklist for the spec's threat table. (Exact ASI numbering not verified.)
- **NIST SSDF**: SP 800-218 v1.1 remains current; v1.2 was an initial public draft on 17 Dec 2025 and still a draft in May 2026. SP 800-218A (generative-AI profile) is final since July 2024 [34].
- **SLSA v1.2**: see section 8 [23].
- **EU Cyber Resilience Act**: reporting duties for manufacturers apply from 11 Sep 2026, most other obligations from 11 Dec 2027 [35]. It bears on software the factory *produces* if that software is placed on the EU market, more than on the factory itself.
- **EU AI Act**: per law-firm summaries, the Digital Omnibus was adopted in July 2026 and moved stand-alone high-risk obligations to 2 Dec 2027 [36]. A coding factory is not obviously in a high-risk category. Low relevance; not legal advice.

## 13. Recommended security architecture (summary)

1. **Control plane** (trusted Rust process): holds App key and LLM key; decides who may trigger; mints per-run scoped tokens; runs the model gateway and egress proxy; validates and applies writes; enforces budgets; writes the audit log.
2. **Worker sandbox** (untrusted, one per run, VM boundary by default): fresh clone of one repo; no GitHub token; no LLM key; network default-deny, reachable only: model gateway and a registry proxy. Dependencies are fetched in a setup phase before the agent sees untrusted text where possible.
3. **Write path**: agent emits structured intents (patch, PR text, comment, label); control plane checks size, paths, secrets and counts, then commits via the API to `factory/<issue>-<run>` and opens the PR.
4. **Merge path**: GitHub rulesets, required checks, human approval; App has no bypass and no Workflows permission.
5. **Default autonomy**: A2. A3 only by explicit, measured, revocable opt-in.

## 14. Where evidence is thin or disputed

- No published data shows any sandbox configuration's real-world escape rate; vendor claims (84% fewer prompts) are unaudited [9].
- GitLost's fix status was not stated by Noma [5]. Google disputes the severity of two Pillar findings [7].
- The OpenAI evaluation-agent escape is known to me only through secondary coverage [37].
- Productivity and quality numbers come from one vendor's telemetry [30] and one survey programme [31]; agent merge-rate figures from one academic dataset read second-hand [32].
- Whether GitHub counts an App's approving review toward required approvals was not verified; the spec should not let any agent identity submit approvals regardless.

## Sources

1. https://simonwillison.net/2025/Jun/16/the-lethal-trifecta/
2. https://ai.meta.com/blog/practical-ai-agent-security/
3. https://arxiv.org/abs/2506.08837 (full text https://arxiv.org/html/2506.08837v3)
4. https://adnanthekhan.com/posts/clinejection/
5. https://noma.security/blog/gitlost-how-we-tricked-githubs-ai-agent-into-leaking-private-repos
6. https://nx.dev/blog/s1ngularity-postmortem ; https://snyk.io/blog/weaponizing-ai-coding-agents-for-malware-in-the-nx-malicious-package (via search summaries)
7. https://www.pillar.security/blog/the-week-of-sandbox-escapes
8. https://code.claude.com/docs/en/sandboxing.md
9. https://www.anthropic.com/engineering/claude-code-sandboxing
10. https://code.claude.com/docs/en/permission-modes.md
11. https://github.com/anthropics/claude-code-action/blob/main/docs/security.md
12. https://docs.github.com/en/enterprise-cloud@latest/copilot/concepts/agents/cloud-agent/risks-and-mitigations
13. https://github.github.com/gh-aw/introduction/architecture/
14. https://github.github.com/gh-aw/reference/triggers/
15. https://github.github.com/gh-aw/reference/integrity/
16. https://github.github.com/gh-aw/reference/safe-outputs/
17. https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app
18. https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
19. https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets
20. https://docs.github.com/en/authentication/managing-commit-signature-verification/about-commit-signature-verification
21. https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/choosing-permissions-for-a-github-app
22. https://docs.github.com/en/actions/concepts/security/artifact-attestations
23. https://slsa.dev/spec/v1.2/ ; https://slsa.dev/spec/v1.2/source-requirements
24. https://github.com/firecracker-microvm/firecracker/blob/main/docs/design.md
25. https://gvisor.dev/docs/architecture_guide/security/
26. https://learn.chatgpt.com/docs/sandboxing (redirect target of developers.openai.com/codex/concepts/sandboxing)
27. https://github.com/apple/containerization ; https://www.infoq.com/news/2025/06/apple-container-linux (via search summaries)
28. https://github.com/open-telemetry/semantic-conventions-genai ; https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-agent-spans.md
29. https://code.claude.com/docs/en/monitoring-usage.md
30. https://www.faros.ai/blog/ai-productivity-paradox-report-2025
31. https://dora.dev/research/2025/dora-report/ ; https://redmonk.com/rstephens/2025/12/18/dora2025/ (secondary)
32. https://arxiv.org/html/2602.02345 (via search summary)
33. https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/ ; names from https://www.aigl.blog/owasp-top-10-for-agentic-applications-2026/
34. https://csrc.nist.gov/pubs/sp/800/218/r1/ipd ; https://csrc.nist.gov/pubs/sp/800/218/a/final (via search summaries)
35. https://ncsc.gov.ie/cra/ ; https://www.mccannfitzgerald.com/knowledge/data-privacy-and-cyber-risk/cyber-resilience-act-reporting-obligations-apply-from-11-september-2026 (via search summaries)
36. https://www.gibsondunn.com/eu-ai-act-omnibus-agreement-postponed-high-risk-deadlines-and-other-key-changes/ (via search summary)
37. https://huggingface.co/blog/security-incident-july-2026 ; secondary: https://blog.securelayer7.net/openai-hugging-face-exploitgym-incident-analysis/
38. https://crates.io/crates/landlock ; /seccompiler ; /nono ; /hakoniwa ; /birdcage ; /extrasafe ; /opentelemetry (crates.io API, 2026-11-18)
39. https://platform.claude.com/docs/en/about-claude/pricing.md (fetched 2026-11-18) ; Claude API reference bundled with Claude Code (prompt caching, cost optimization, task budgets, Admin API sections; cached 2026-10-06) ; https://platform.claude.com/docs/en/manage-claude/usage-cost-api.md (not opened)
40. https://www.theregister.com/a/5303140 ; https://lwn.net/SubscriberLink/1083275/d5518174ec4dcbcb/ (via search summaries)
