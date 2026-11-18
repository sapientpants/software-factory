# 05 - Rust implementation best practice for the software factory

Research date: 2026-11-18. Angle: how to build a long-running orchestrator daemon plus CLI in Rust that drives
subprocess coding agents, git and the GitHub API.

Method: crate versions and dates come from the crates.io API queried today; repository health from the GitHub API;
behaviour claims from official docs or from source code I read (Codex CLI, jj, vibe-kanban, the official Claude
Agent SDK for Python). Where I rely on memory or on thin evidence, I say so.

## 1. Summary

Build one Cargo workspace (edition 2024, resolver 3, toolchain pinned at 1.100) that produces a single binary.
Use tokio with explicit structured concurrency, shell out to the `git` CLI for everything that mutates a
repository, talk to GitHub through `octocrab` behind a narrow trait, and drive Claude Code as a subprocess over
its `stream-json` protocol with a thin adapter of our own. Keep runtime state in a local SQLite journal that can
be rebuilt from GitHub and git. Three findings should shape the spec more than any crate choice:

1. **There is no official Rust SDK for either the Claude API or the Claude Agent SDK.** The official route for
   other languages is "run the CLI as a subprocess". The `claude-agent-sdk` crate on crates.io links to a
   GitHub repository under `anthropics/` that does not exist.
2. **`gix` cannot yet push, stage files, reset, run hooks, or remove worktrees.** The Rust tools that manage
   worktrees in production (jj, vibe-kanban) shell out to `git` for mutations.
3. **Claude Code's built-in sandbox covers shell commands only.** Its file tools, MCP servers and hooks run
   outside it, so the factory needs its own OS-level boundary around the whole agent process.

## 2. Recommended stack

| Concern | Choice (version verified today) | Why |
| --- | --- | --- |
| Runtime | `tokio` 1.54.1, `tokio-util` 0.7.20 | `CancellationToken` + `TaskTracker` is the documented shutdown pattern |
| Child processes | `tokio::process` + `process-wrap` 10.0.1 (`tokio1`) | Process groups/sessions and kill-on-drop as composable wrappers |
| Git | `git` CLI >= 2.42 via one wrapper crate; `gix` 0.90.0 optional, read-only | `gix` lacks push, staging, reset, hooks, worktree removal |
| GitHub | `octocrab` 0.54.9 | Typed REST, GraphQL, App/installation auth, header-aware retry |
| Agent | Own adapter over `claude -p` `stream-json` | No official Rust SDK; community crates are stale |
| MCP | `rmcp` 3.11.0 (official) | Expose factory tools to agents |
| Multi-vendor agents | `agent-client-protocol` 5.4.0, later, behind our own trait | Protocol v1 stable, crate churns fast |
| State | `rusqlite` 0.43.0 (`bundled`, SQLite 3.54.1) + `rusqlite_migration` 2.7.0 | Single-writer journal; inspectable with `sqlite3` |
| Config | `serde` 1.0.229 + `toml` 1.2.1, hand-rolled layering, `schemars` 1.2.3 | What Codex and jj do; `figment` has had no release since May 2024 |
| CLI | `clap` 4.6.8 (derive), `clap_complete` | Standard |
| Errors | `thiserror` 2.0.21 in libraries, `anyhow` 1.0.104 in the binary | Typed, classifiable errors where the orchestrator must decide |
| Telemetry | `tracing` 0.1.44, `tracing-subscriber` 0.3.23, `opentelemetry` 0.33.1, `tracing-opentelemetry` 0.34.0 | 0.34 depends on `opentelemetry ^0.33` |
| Tests | `cargo-nextest` 0.9.152, `insta` 1.49.0, `wiremock` 0.6.5, `proptest` 1.12.0, `assert_cmd` 2.2.2 | See section 12 |
| Supply chain | `cargo-deny` 0.21.0, `cargo-shear` 1.14.1 or `cargo-machete` 0.10.1 | See section 13 |
| Release | `cargo-dist` 0.34.0 | Released 2026-11-02; installers, attestations, SBOM |

## 3. Workspace and crate layout

Use a virtual workspace with `[workspace.package]`, `[workspace.dependencies]` and `[workspace.lints]` so every
crate inherits edition, versions and lints. Split by *seam*, not by layer, so each external system has one crate
that owns it and one trait the core depends on:

- `factory-core`: domain types and the run state machine as a pure function
  `(State, Event) -> (State, Vec<Effect>)`. No I/O, no tokio.
- `factory-proc`: spawning, process groups, line streaming, timeouts, kill escalation.
- `factory-git`: the only crate allowed to run `git`; worktree manager.
- `factory-github`: `octocrab` behind a `GitHub` trait.
- `factory-agent`: `AgentDriver` trait, Claude stream-json codec and driver.
- `factory-sandbox`: policy type plus Seatbelt and bubblewrap backends.
- `factory-store`: SQLite journal and projections.
- `factory-daemon`: scheduler, supervisors, reconciliation loops.
- `factory-cli`: the `sf` binary (`sf daemon run` starts the daemon in the foreground).
- `factory-testkit`: fake agent binary, GitHub fixtures, temp-repo helpers.
- `xtask`: repo automation.

Codex CLI is the closest large reference: a Rust workspace of roughly 200 crates on edition 2024 with
`[workspace.lints.clippy]` denying `unwrap_used`, `expect_used`, `await_holding_lock` and `disallowed_methods`.
vibe-kanban (Rust, orchestrates coding agents in worktrees) uses resolver 3 and separate `git`,
`worktree-manager`, `executors` and `db` crates. Start with about ten crates; do not copy the 200.

## 4. Async runtime and process supervision

**Structured concurrency.** Tokio's own shutdown guide prescribes three steps: detect the signal, cancel through
a cloned `CancellationToken`, then `tracker.close(); tracker.wait().await` on a `TaskTracker`. Make the token a
tree: root token, one child token per run, one per agent process. Forbid bare `tokio::spawn` outside the
supervisor module with clippy's `disallowed_methods`. Treat `JoinError::is_panic()` as a failed run, not a
daemon crash.

**Child processes.** Tokio's docs are explicit that a dropped `Child` keeps running by default, that
`kill_on_drop` exists, and that you should "avoid dropping a `Child` handle" and instead `wait()` or `kill()`.
`kill_on_drop` only targets the direct child. Agents spawn shells that spawn builds, so spawn every agent as a
process-group or session leader with `process-wrap` (`ProcessGroup::leader()` or `ProcessSession`, plus
`KillOnDrop`) and signal the group. `process-wrap` is the stated successor of `command-group` (last release
November 2023). A group does not catch a grandchild that calls `setsid`; on Linux use bubblewrap with a PID
namespace for that, on macOS accept the gap and record it.

**Shutdown order for Claude Code.** The official Python SDK closes stdin, waits 5 s, sends SIGTERM, waits 5 s,
then SIGKILL; the comment explains that SIGTERM without the grace period can lose the last assistant message.
The headless docs add: SIGTERM exits with code 143, leaves the turn unfinished, terminates the process tree of
any running Bash command and runs `SessionEnd` hooks; to end the turn cleanly, send SIGINT or an `interrupt`
control request first. Copy this sequence.

**Streaming.** Use pipes, not a PTY: headless agent CLIs do not need a terminal. Read stdout with
`FramedRead` + `LinesCodec::new_with_max_length` and drain stderr concurrently, otherwise a full pipe deadlocks
the child. The official SDK caps a buffered message at 1 MiB by default and raises on overflow; pick a cap,
treat overflow as a typed error, and write raw lines to disk before parsing. Keep `portable-pty` 0.9.0 (last
release February 2025, used by Codex) in reserve for a tool that only works interactively.

**Timeouts.** Three independent limits per run: wall clock, idle (no stream event for N minutes), and the
agent's own `--max-turns` / `--max-budget-usd`. Note the docs' warning that spend "can pass the cap".

**Restart recovery.** Persist pid, pgid and process start time when spawning. On startup, kill leftover groups,
run `git worktree prune`, and reconcile against GitHub before scheduling anything.

## 5. Git from Rust

Shell out to `git` for every mutation. Evidence:

- `gix` crate-status marks as not done: push (both `gix` and `gix-protocol`), adding index entries, reset,
  hook execution, signed commits, rebase, and "move, remove, and repair linked worktrees". Fetch, clone, status,
  diff, commit-from-tree, merge analysis and *creating* linked worktrees are done.
- jj 0.30.0 (June 2025) removed the libgit2 code path for fetch and push and the `git.subprocess` setting with
  it; jj 0.46.0 requires `git` >= 2.42 because `jj workspace add` calls `git worktree add --orphan`. jj depends
  on `gix` 0.90.0 for reading.
- vibe-kanban's `git/src/cli.rs` explains its choice: the CLI refuses to clobber uncommitted changes where
  libgit2 does not, respects sparse checkout, and they saw libgit2 corrupt repositories shared between WSL and
  Windows. They keep libgit2 for read-only graph queries.

Design consequences for the wrapper crate:

- One typed function per operation, parsing `--porcelain` / `-z` output only. `git worktree list --porcelain`
  is documented as stable across versions.
- Create worktrees with `git worktree add --lock --reason <run-id> -b <branch> <path> <base>`; `--lock` at
  creation is documented as race-free. A branch cannot be checked out in two worktrees. Removing an unclean
  worktree needs `--force`, a locked one needs it twice.
- Refs, objects and config are shared between worktrees; `HEAD` and `index` are per-worktree. Serialize
  `fetch`, `worktree add` and `worktree remove` per repository with an in-process mutex.
- Run git non-interactively (`GIT_TERMINAL_PROMPT=0`), with a scrubbed environment, and pass credentials per
  invocation rather than writing them into `.git/config` or remote URLs. (Standard git behaviour, from memory.)
- Check `git --version` at startup and refuse below the floor.

`git2` 0.21.0 is maintained, but it adds a C dependency and the safety gaps above. Do not add it.

## 6. GitHub client

`octocrab` 0.54.9 (released 2026-10-31) is the only actively maintained general client. `octorust` last
released March 2025, `roctogen` August 2025, `hubcaps` 2020.

Verified in its source and README: default features include `retry`, `timeout`, `tracing`, `jwt` and rustls;
`installation()`, `installation_and_token()` and `installation_token_with_buffer()` exist for GitHub Apps;
`graphql()` exists; there is a raw `get/post/...` escape hatch; `RetryConfig` can wait on GitHub's rate-limit
headers for 403/429; there is an `etag` module and a cache middleware. The README also says "There's no rate
limiting so be careful" about pagination and that webhook models are "still beta".

So octocrab reacts to limits but does not budget them. GitHub's documented limits are 5,000 requests/hour per
installation token (up to 12,500), 100 concurrent requests, 900 points/minute for REST where writes cost 5, and
about 80 content-creating requests/minute and 500/hour. Put a small scheduler in `factory-github`: a
concurrency semaphore, serialized writes, and a reconciliation loop that uses conditional requests. Keep
octocrab types out of the core; map to our own. Use GraphQL or raw routes with our own serde structs for
anything the typed models lack.

Keep the default `jwt-aws-lc-rs` backend: the README notes that `jwt-rust-crypto` pulls `rsa` 0.9 with
RUSTSEC-2023-0071. The cost is a C toolchain at build time (vibe-kanban pins `aws-lc-sys` exactly).

## 7. Agent and LLM integration

**Claude Code over stream-json is the primary driver.** Anthropic lists official client SDKs for Python,
TypeScript, C#, Go, Java, PHP and Ruby, and Agent SDK packages for Python and TypeScript only. The Agent SDK
overview says: "To drive the same agent loop from a language other than Python or TypeScript, run the CLI as a
subprocess". The Python SDK launches
`claude --output-format stream-json --verbose ... --input-format stream-json`; that command line, and its
`control_request` / `control_response` / `control_cancel_request` messages (subtypes `initialize`, `interrupt`,
`can_use_tool`, `hook_callback`, `mcp_message`, `set_permission_mode`, `set_model`), are the de facto protocol.
The user-message shape is documented:
`{"type":"user","message":{"role":"user","content":...},"parent_tool_use_id":null}`. The control messages are
documented only through the SDKs, so treat them as unstable.

Documented behaviours to build on. All flags below except `--max-turns` appear in `claude --help` of the
locally installed 2.1.311; `--max-turns` is in the CLI reference and is passed by the official SDK, but is not
listed in `--help`:

- The stream starts with `system/init` (model, tools, MCP servers, a `capabilities` array meant for feature
  detection) and ends with a `result` message carrying `subtype`, `session_id`, `total_cost_usd`, usage and
  `permission_denials`. `system/api_retry` events report retries.
- `--bare` skips discovery of hooks, skills, plugins, MCP servers, auto memory and CLAUDE.md, is "the
  recommended mode for scripted and SDK calls", needs `ANTHROPIC_API_KEY`, and will become the default for `-p`.
  Without it, a `-p` session runs the project's hooks and `.mcp.json` servers with no trust dialog. Hooks never
  run in bare mode.
- `--session-id <uuid>`, `--resume`, `--fork-session`, `--max-turns`, `--max-budget-usd`, `--permission-mode`,
  `--permission-prompts none`, `--allowedTools`, `--tools`, `--mcp-config` with `--strict-mcp-config`,
  `--settings`, `--json-schema`.
- Anthropic "does not allow third party developers to offer claude.ai login or rate limits for their products";
  plan for API-key authentication.

**Do not depend on community Claude SDK crates.** `claude-agent-sdk` 0.1.1 (September 2025, 4,350 downloads) is
owned by an individual account and its repository link under `anthropics/` returns 404. `claude-agent-sdk-rs`
0.6.4 and `claude-code-agent-sdk` 0.1.39 last released February 2026, `cc-sdk` 0.8.1 April 2026. The CLI gains
protocol features with minimum-version notes almost weekly; a thin adapter of our own, with tolerant parsing
(unknown message types and fields preserved, not rejected), is cheaper than tracking a stale wrapper.

**Agent Client Protocol as the second driver, not the core abstraction.** `agent-client-protocol` 5.4.0 was
published today; protocol v1 is stable and v2 is a draft behind unstable features. Gemini CLI, Copilot (public
preview), Goose, OpenCode and Cursor speak it natively; Claude and Codex need adapters
(`zed-industries/claude-agent-acp`, `agentclientprotocol/codex-acp`). vibe-kanban currently depends on two
versions of the crate at once (0.8 and `=3.4.0`), which shows the churn. Define our own `AgentDriver` trait and
add an ACP driver when a second vendor is needed.

**MCP.** `rmcp` 3.11.0 is the official SDK and implements the 2026-07-28 spec. Use it to give agents factory
tools (report status, ask for a decision) and optionally a permission-prompt tool. Codex pins it to an exact
version and git revision; pin exactly too.

**Direct model calls** (triage, summarising, judging). Prefer one-shot `claude -p --bare --json-schema` through
the same adapter. If raw API access is needed, write a small `reqwest` client. `rig-core` 0.46.0 states "Rig is
pre-1.0, so releases break APIs" and has MSRV 1.97.1; `genai` 0.7.5 is a lighter multi-provider option. Neither
belongs in the core: the factory orchestrates agent CLIs, it does not implement agent loops.

## 8. Local state

GitHub Issues and git are the system of record; local state is a journal and cache (leases, attempts, session
ids, costs, event log). Requirements: crash-safe, readable by CLI and daemon at once, inspectable by a human.

Use SQLite in WAL mode. `rusqlite` 0.43.0 with `bundled` ships SQLite 3.54.1. Run one writer thread that owns
the connection and applies "append event + update projection" in one transaction; readers open their own
connections. `sqlx` 0.9.1 is the credible alternative (Codex and vibe-kanban both use sqlx with SQLite). It
adds async pooling and compile-time checked queries at the cost of offline query metadata and MSRV 1.94. This
is a judgment call; either works.

Store bulky artefacts as files (`runs/<run-id>/transcript.ndjson`, stderr, diffs) and keep paths in the
database. Hand-roll the event log (one append-only table); the event-sourcing crates are small or stale
(`cqrs-es` 0.5.0, `eventually` 2020). `redb` 4.6.0 is healthy but key-value only. Durable-execution SDKs
(`temporalio-sdk` 1.2.0, `restate-sdk` 0.13.0) need a server and break the single-binary goal. `sled` has had
no stable release in years.

## 9. Config, CLI, errors

**Config.** TOML with `serde` and `deny_unknown_fields`. Layers: defaults, user file, repository file,
environment, flags. Codex and jj both hand-roll layering on `toml` / `toml_edit`. `figment` 0.10.19 was last
released in May 2024; `config` 0.15.27 is maintained if a library is preferred. Generate a JSON Schema with
`schemars`. Treat the repository-level file as untrusted for security-relevant keys, because agents can edit
it. Hold secrets in `secrecy::SecretString` (octocrab already does) and never in config files.

**CLI.** `clap` derive, one binary, a `--json` output mode on every read command, documented exit codes.

**Errors.** Library crates expose `thiserror` enums that classify failures (transient, permanent, needs human,
rate limited with reset time), since retry policy depends on the class. Use `anyhow` only at the binary edge.
`miette` 7.6.0 has had no release since April 2025; use it only if rich config diagnostics are wanted.

## 10. Observability

`tracing` everywhere, with `run_id`, issue number and attempt as span fields; JSON logs to a file, human logs
to stderr. Add OpenTelemetry export behind a feature flag. In `opentelemetry-rust` 0.33, logs and metrics are
Stable and traces are Beta.

The GenAI semantic conventions are still in **Development** status and moved in May 2026 to their own
repository, `open-telemetry/semantic-conventions-genai`. Current attribute names include
`gen_ai.operation.name` (`invoke_agent`, `execute_tool`, `chat`), `gen_ai.agent.name`,
`gen_ai.conversation.id`, `gen_ai.request.model`, `gen_ai.provider.name` and `gen_ai.usage.input_tokens` /
`output_tokens`. Keep the names in one module so a rename is a one-file change.

Claude Code exports its own telemetry with `CLAUDE_CODE_ENABLE_TELEMETRY=1`. In `-p` sessions it reads
`TRACEPARENT` and `TRACESTATE` from its environment, so setting `TRACEPARENT` per run nests the agent's spans
under the factory's run span (tracing is beta and also needs `CLAUDE_CODE_ENHANCED_TELEMETRY_BETA=1`). It
reports `claude_code.token.usage` and `claude_code.cost.usage`. Cost in the `result` message and in these
metrics is a client-side estimate.

## 11. Sandboxing

Claude Code's docs state: "The sandbox covers shell commands only. Claude's file tools, MCP servers, and hooks
run outside it." The factory therefore wraps the whole agent process.

- **macOS:** Seatbelt through `sandbox-exec` with a generated profile. Both Codex and Anthropic's
  `sandbox-runtime` do this.
- **Linux:** bubblewrap plus seccomp. Codex's docs say Linux "uses `bwrap` plus `seccomp` by default";
  `sandbox-runtime` uses bubblewrap with the network namespace removed. In containers where user namespaces are
  blocked, fall back to `landlock` 0.4.7 (ABI up to v9, best-effort by default) and `seccompiler` 0.5.0 in
  `pre_exec`, which Codex still depends on.
- **Network:** deny by default and route through an allowlisting proxy on the host, as `sandbox-runtime` does.
- **Policy:** Codex makes `<writable_root>/.git` read-only even in workspace-write mode. Adopt the same rule and
  let the factory, not the agent, create commits and push. Spawn agents with `env_clear()` and an explicit
  allowlist; never give the agent the GitHub token.

Define `Sandbox::wrap(Command, &Policy) -> Command` with `seatbelt`, `bwrap` and an explicit, logged `none`
backend. `@anthropic-ai/sandbox-runtime` is an npm package labelled "Beta Research Preview". `nono` 0.82.0 is a
Rust-native candidate (Apache-2.0, pre-1.0, 307 open issues); evaluate it, do not assume it. Avoid `gaol`
(2019), `birdcage` (April 2024) and `extrasafe` (April 2024).

Evidence is thin on nesting: Bazel's documentation says a process already inside a macOS sandbox cannot nest
`sandbox-exec`. If the outer boundary is Seatbelt, Claude's inner Bash sandbox probably has to be off. Test it.

## 12. Testing

- **Pure core.** Because `factory-core` is a pure state machine, property tests (`proptest`) can assert
  invariants such as "at most one active run per issue" over random event sequences. This gives most of the
  value of deterministic simulation for a single-node system. Use `tokio::time::pause` for timers.
- **Fake agent.** Ship a `fake-agent` binary in the testkit that replays scripted NDJSON and misbehaves on
  demand: hangs, ignores SIGTERM, emits a partial or oversized line, exits 143, spawns a grandchild. Supervision
  code is then tested against real processes. Record real transcripts as fixtures.
- **GitHub.** `wiremock` 0.6.5 against octocrab's configurable base URI, including 403/429 with rate-limit
  headers and 304 responses. `wiremock` last released August 2025; `httpmock` 0.9.0 is the fresher alternative.
- **Git.** Real `git` in temp directories with a local bare repository as origin. No mocks.
- **Snapshots.** `insta` for parsed event streams, rendered PR bodies and CLI output.
- **Runner.** `cargo-nextest`: process per test, retries, slow-test termination, test groups, JUnit, leak
  detection. Doctests are not supported; run `cargo test --doc` separately.
- **Contract test.** A scheduled, opt-in job that runs the real `claude` binary on a trivial prompt to catch
  protocol drift.

`turmoil` 0.7.2 simulates hosts, network and filesystem inside one process; it does not exercise real
subprocesses, so it is not the main tool here. `madsim` last released October 2025.

## 13. Lint, supply chain, release, toolchain policy

- `rustfmt`; `[workspace.lints]` denying `unwrap_used`, `expect_used`, `await_holding_lock`; `clippy.toml`
  `disallowed-methods` for `std::process::Command::new` and `tokio::spawn` outside their owning crates;
  `unsafe_code = "forbid"` everywhere except the sandbox crate.
- `cargo-deny` 0.21.0 for advisories, licences, bans and sources (it covers what `cargo-audit` 0.23.0 does);
  `cargo-shear` or `cargo-machete` for unused dependencies; commit `Cargo.lock` and build with `--locked`.
- `cargo-dist` 0.34.0: config in `dist-workspace.toml`; shell, PowerShell, Homebrew, npm and MSI installers;
  optional `github-attestations` (described as experimental, default off), `cargo-auditable`, `cargo-cyclonedx`
  and `install-updater`. Set `publish = false` on all crates; this is a binary, not a library.
- Edition 2024 is the latest (Rust 1.85, February 2025); the Rust blog shows no newer edition. Rust 1.100.0 was
  announced 2026-11-12. As an application, set `rust-version` to the pinned toolchain and bump it by PR.
  Dependency floors today: `rusqlite` 1.89, `gix`/`rmcp`/`agent-client-protocol` 1.88, `sqlx` 1.94.
- Dependency etiquette: sqlx 0.9.1 (2026-11-17) added an `AI_POLICY.md` rejecting issues and PRs "authored
  primarily or entirely by agents". The factory must never file upstream issues or PRs on third-party
  repositories on its own.

## 14. Crates to avoid

| Crate | Reason |
| --- | --- |
| `claude-agent-sdk` (crates.io) | Individual owner; repository link under `anthropics/` is 404 |
| `git2` for mutations | Weaker working-tree safety than the CLI; C dependency |
| `gix` for push, staging, worktree removal | Not implemented |
| `command-group` | Superseded by `process-wrap` |
| `figment` | No release since May 2024 |
| `sled`, `madsim`, `gaol`, `birdcage`, `extrasafe`, `hubcaps` | Stale |
| `octorust`, `roctogen` | Low activity compared with `octocrab` |
| `serde_yaml` | Marked deprecated by its author |
| `rig-core` as a core dependency | Pre-1.0 with breaking releases; wrong abstraction level |

## 15. Limits of this research

- I did not run a live `claude -p` session; stream shapes come from docs and SDK source.
- Codex's `codex exec --json` event schema has no official reference that I could find; only third-party
  parsers, which disagree on field names.
- Git environment hardening advice (section 5) and `sandbox-exec` being deprecated by Apple are from memory.
- The GitHub API quota was exhausted during the session, so octocrab's default retry count and cache behaviour
  were not read in full.

## Sources

- crates.io API (`https://crates.io/api/v1/crates/<name>`), queried 2026-11-18, for every version, date,
  MSRV and download figure above
- https://code.claude.com/docs/en/headless
- https://code.claude.com/docs/en/cli-reference
- https://code.claude.com/docs/en/agent-sdk/overview
- https://code.claude.com/docs/en/agent-sdk/streaming-vs-single-mode
- https://code.claude.com/docs/en/monitoring-usage
- https://code.claude.com/docs/en/sandboxing
- https://platform.claude.com/docs/en/cli-sdks-libraries/overview
- https://github.com/anthropics/claude-agent-sdk-python (`src/claude_agent_sdk/_internal/transport/subprocess_cli.py`, `_internal/query.py`)
- https://github.com/anthropics/sandbox-runtime
- https://crates.io/crates/claude-agent-sdk
- https://github.com/GitoxideLabs/gitoxide/blob/main/crate-status.md
- https://github.com/jj-vcs/jj/blob/main/CHANGELOG.md
- https://github.com/jj-vcs/jj/blob/main/Cargo.toml
- https://github.com/BloopAI/vibe-kanban (`Cargo.toml`, `crates/git/src/cli.rs`, `crates/executors/Cargo.toml`)
- https://github.com/openai/codex/blob/main/codex-rs/Cargo.toml
- https://learn.chatgpt.com/docs/agent-approvals-security
- https://git-scm.com/docs/git-worktree
- https://github.com/XAMPPRocky/octocrab (README, `Cargo.toml`, `src/service/middleware/retry.rs`, `src/lib.rs`)
- https://docs.rs/octocrab/latest/octocrab/
- https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
- https://github.com/modelcontextprotocol/rust-sdk
- https://github.com/agentclientprotocol/rust-sdk
- https://agentclientprotocol.com/overview/agents
- https://github.com/0xPlaygrounds/rig
- https://tokio.rs/tokio/topics/shutdown
- https://docs.rs/tokio/latest/tokio/process/struct.Command.html
- https://github.com/watchexec/process-wrap
- https://github.com/rusqlite/rusqlite
- https://github.com/transact-rs/sqlx/blob/main/CHANGELOG.md
- https://github.com/open-telemetry/opentelemetry-rust
- https://github.com/open-telemetry/semantic-conventions-genai (`docs/gen-ai/README.md`, `gen-ai-agent-spans.md`)
- https://opentelemetry.io/docs/specs/semconv/gen-ai/
- https://docs.rs/landlock/latest/landlock/
- https://github.com/nolabs-ai/nono
- https://nexte.st/
- https://github.com/tokio-rs/turmoil
- https://axodotdev.github.io/cargo-dist/book/reference/config.html
- https://github.com/axodotdev/cargo-dist/releases
- https://blog.rust-lang.org/
- https://bazel.virtuslab.com/book/5~8~3/ (nested macOS sandbox; secondary source)
