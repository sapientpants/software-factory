# Software Factory

The terms of `sf`, the daemon that turns GitHub issues the operator approved into merged pull requests. `SPEC.md` uses each term as defined here, and section 2 of the spec points at this file (D-21). Section numbers and ids cited below are those of `SPEC.md`.

## Language

**Operator**:
The person who installs and runs the factory and owns its credentials.

**Authorised actor**:
A GitHub account of type User whose permission on the repository is write, maintain or admin when checked (SEC-1).

**Approver**:
The authorised actor who applied `state:ready` to an issue. With one human, this is the operator.

**Control plane**:
The trusted `sf` daemon process. Holds all credentials and performs all external writes.

**Host**:
The machine the control plane runs on: in operation, the operator's Linux server (D-3, 4.4). Where the specification has the host act, the control plane acts on that machine.

**Worker**:
The untrusted side: an agent process or build command and everything it spawns, inside a sandbox.

**Work item**:
An open GitHub issue carrying exactly one `state:*` label.

**Run**:
The factory's handling of one work item from claim to a terminal state. Identified by a run id (UUIDv7). Terminal states: `done` (pull request merged, or for a tier 2 parent all children closed as completed) and `cancelled` (issue closed without a merged pull request, issue moved to `state:triage` by a human, or `sf cancel`). A run in `needs-human` is not terminal.

**Attempt**:
One pass of a run that starts from a fresh branch off the default branch. A run has one attempt unless it is reset (7.6).

**Stage**:
One step of the internal pipeline (7.3).

**Stage execution**:
One execution of a stage: one agent session, one capture, or one verify command sequence, together with the `setup` commands that precede it (WS-4). Each gets its own sandbox instance.

**Round**:
One trip back to the implement stage after a failed gate.

**Turn**:
One model call of an agent session. A session's turn limit counts them (BUD-6).

**Workspace**:
The directory tree in which the implementer works. It lives for one attempt, or until a base update rebuilds it (12.4).

**Candidate**:
A commit the host builds from the workspace (9.4). Gates run on candidates; only candidates are pushed.

**Policy**:
The repository-owned configuration in `.factory/` on the default branch.

**Check tool**:
A program that a repository's setup commands or checks run and that is no part of its toolchain (WS-7): `cargo-nextest` or `typos`, for example. WS-7a says how one reaches a worker.

**Snapshot**:
The issue title, body and trusted comments captured at claim or resume and given to agents.

**Target repository**:
A repository the factory is onboarded to (20) and works on.

**Factory repository**:
The repository that holds `sf` and its specification, `SPEC.md`. It is also a target repository, the first that an installed build of `sf` works on (BOOT-1).

**Scratch repository**:
`sapientpants/sf-scratch`, where live probes (23.2) and development builds of `sf` (BOOT-2) run.
