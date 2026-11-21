# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) and other coding agents when working in this repository. It is a map: the rules live under their ids in `SPEC.md`, and this file states one itself only where a session that read nothing else would do harm (D-21).

## State of the repository

Start of milestone M0. The repository holds a spec, its evidence and a Cargo workspace whose crates are empty, except `xtask` (SPEC 19.1).

- `SPEC.md` is the single source of truth for the design. Its status row says which draft it is and whether that draft was reviewed.
- `GLOSSARY.md` defines the spec's terms. Use each term as defined there, in code, documents and issues alike.
- `docs/research/NN-*.md` are the sourced evidence behind the spec. They are records: add new ones, leave existing ones as written.

## The system in one paragraph

`sf` is a Rust daemon that turns GitHub issues the operator approved into merged pull requests. A trusted **control plane** holds every credential and performs every external write. Untrusted **workers** (the agent CLI, builds, tests) run in a sandbox with no GitHub credential and default-deny network. Deterministic code sequences a fixed pipeline, `claim → plan → implement → capture → integrity → verify → publish → ci → review → human_review → merge`, and an agent only works inside a stage. GitHub is the only system of record; the local SQLite store is a rebuildable cache.

Where to read in `SPEC.md`:

- **Invariants (3.1):** before any design change. A change that breaks INV-1 to INV-11 is a defect.
- **Settled decisions (23.1):** before proposing an alternative. The host, the credential, the autonomy goal and the scope are decided there.
- **Lifecycle and pipeline (7), orchestrator (8):** for anything about stages, states, rounds or recovery.
- **GitHub data model (5):** for labels, the status comment, check runs and rate limits.
- **Security (13):** for the sandbox, credentials, trust rules and autonomy classes.
- **Roadmap (22):** before scoping work, for what the current milestone includes.

## How work is done here

- **One issue, one branch, one pull request** (D-16, BOOT-1). Work on a branch, open a pull request that closes the issue, and leave the merge to the operator.
- **`cargo xtask check` before each commit** (DEV-1); it must pass. It runs the checks of the SPEC 19.5 set (DEV-2) that are built. The rest of that set, the development VM (DEV-3) and the CI workflow (DEV-5) are still M0 work; add each here when it exists.
- **Every commit goes through the pre-commit hook** (DEV-9), which runs the same command. When it refuses a commit, fix what it reports and commit again with the hook in place; a Claude Code session is refused a commit or a push that skips it (DEV-10). A Claude Code session points the clone at the hook when it starts; any other clone needs `git config core.hooksPath .githooks` once.
- **The server is the operator's** (D-16). Every step on it is theirs to run: write the command or script, hand it over, and work from the output they bring back.
- **Live GitHub runs go to the scratch repository**, `sapientpants/sf-scratch`: probes (SPEC 23.2) and development builds of `sf` (BOOT-2). This repository receives only the issue, branch and pull request of the work itself.

## Writing code

Read in `SPEC.md`:

- **Crate layout (19.1):** before adding a crate or a dependency between crates. `xtask/tests/structure.rs` holds that table as data; change the spec's table and the test together.
- **Stack (19.2):** before adding an external dependency. It names the crate chosen for each concern, the exact pins and the crates that are not used.
- **Conventions (19.3):** when a workspace lint fails, and before spawning a task or a process.
- **Testing (19.4):** before writing a test, for the layer it belongs to.

## Editing the spec

- Every requirement has an id (`INV-`, `GH-`, `SEC-`, `WS-`, `AG-`, `VAL-` and so on). State a rule once, under its id, and cite the id elsewhere.
- A term is defined once, in `GLOSSARY.md` (D-21). The rules of this section apply to that file too.
- A decision is recorded in 23.1, the repository's only decision record; it keeps no `docs/adr/` through M1 (D-21). A platform fact the design depends on but nobody verified is a probe in 23.2; its answer goes in `docs/research/probes.md` and the spec changes in the same commit.
- `*(tunable)*` marks a number with no empirical basis. Keep the mark until the number is measured.
- After an edit, every id and section number cited in `SPEC.md`, `GLOSSARY.md` and this file resolves, and the status row matches the draft's real review state.

## Planning map

Open design work is tracked on a wayfinder map: the GitHub issue labelled `wayfinder:map`. Its sub-issues, labelled `wayfinder:<type>`, are decision tickets. The map's Notes section holds the standing rules for working them; read it before resolving a ticket.

- `research/*` branches hold a research ticket's findings. They are pushed and never merged.

## Labels that mean approval

`state:ready`, `factory:plan-approved`, `factory:hold`, `factory:restart`, `factory:stop`, `tier:*` and `risk:*` are the operator's inputs to the factory (SPEC 5.2, SEC-1). Only the operator applies or removes them. An agent that applies one with the operator's `gh` login has approved work in the operator's name.

No label of SPEC 5.2 exists on this repository before its onboarding (D-21). A session creates none of them on its own: creating them is step 5 of SPEC 20.1, and it waits for the operator's instruction.

## Agent skills

### Issue tracker

Issues live in this repository's GitHub Issues (`sapientpants/software-factory`), via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default triage labels, kept separate from the factory's `state:*` labels. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` at the repository root, and SPEC 23.1 in place of ADRs. See `docs/agents/domain.md`.
