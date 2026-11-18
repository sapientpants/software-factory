# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) and other coding agents when working in this repository.

## State of the repository

Start of milestone M0. The repository holds a spec, its evidence and a Cargo workspace of empty crates (SPEC 19.1).

- `SPEC.md` is the single source of truth for the design. Its status row says which draft it is and whether that draft was reviewed.
- `docs/research/NN-*.md` are the sourced evidence behind the spec. They are records: add new ones, leave existing ones as written.
- Checks, each of which must pass before a change is done: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `cargo test --workspace --locked`. The rest of the SPEC 19.5 set (DEV-2), the `cargo xtask check` command that runs it (DEV-1), the development VM (DEV-3) and the CI workflow are still M0 work; add each here when it exists.
- `xtask/tests/structure.rs` holds the crate table of SPEC 19.1 as data. Adding a crate or a dependency between crates means changing the spec's table and that test together.

## The system in one paragraph

`sf` is a Rust daemon that turns GitHub issues the operator approved into merged pull requests. A trusted **control plane** holds every credential and performs every external write. Untrusted **workers** (the agent CLI, builds, tests) run in a sandbox with no GitHub credential and default-deny network. Deterministic code sequences a fixed pipeline, `claim → plan → implement → capture → integrity → verify → publish → ci → review → human_review → merge`, and an agent only works inside a stage. GitHub is the only system of record; the local SQLite store is a rebuildable cache.

Where to read in `SPEC.md`:

- **Invariants (3.1):** before any design change. A change that breaks INV-1 to INV-11 is a defect.
- **Settled decisions (23.1):** before proposing an alternative. The host, the credential, the autonomy goal and the scope are decided there.
- **Lifecycle and pipeline (7), orchestrator (8):** for anything about stages, states, rounds or recovery.
- **GitHub data model (5):** for labels, the status comment, check runs and rate limits.
- **Security (13):** for the sandbox, credentials, trust rules and autonomy classes.
- **Crate layout and stack (19), roadmap (22):** before writing code or scoping work.

## Editing the spec

- Every requirement has an id (`INV-`, `GH-`, `SEC-`, `WS-`, `AG-`, `VAL-` and so on). State a rule once, under its id, and cite the id elsewhere.
- A decision is recorded in 23.1. A platform fact the design depends on but nobody verified is a probe in 23.2; its answer goes in `docs/research/probes.md` and the spec changes in the same commit.
- `*(tunable)*` marks a number with no empirical basis. Keep the mark until the number is measured.
- After an edit, every cited id and section number resolves, and the status row matches the draft's real review state.

## Planning map

Open design work is tracked on a wayfinder map: the GitHub issue labelled `wayfinder:map`. Its sub-issues, labelled `wayfinder:<type>`, are decision tickets. The map's Notes section holds the standing rules for working them; read it before resolving a ticket.

- `research/*` branches hold a research ticket's findings. They are pushed and never merged.
- Live GitHub probes run on the scratch repository named on the map, never on this one.

## Labels that mean approval

`state:ready`, `factory:plan-approved`, `factory:hold`, `factory:restart`, `factory:stop`, `tier:*` and `risk:*` are the operator's inputs to the factory (SPEC 5.2, SEC-1). Only the operator applies or removes them. An agent that applies one with the operator's `gh` login has approved work in the operator's name.

## Agent skills

### Issue tracker

Issues live in this repository's GitHub Issues (`sapientpants/software-factory`), via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default triage labels, kept separate from the factory's `state:*` labels. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repository root. See `docs/agents/domain.md`.
