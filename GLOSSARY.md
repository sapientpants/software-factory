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
One model call of an agent session, made by its main loop or by a subagent. A session's turn limit counts them, which the agent CLI's own `--max-turns` does not (BUD-6).

**Main loop**:
The line of model calls that the agent CLI makes for the messages an agent session is given, as distinct from a subagent's.

**Subagent**:
A second line of model calls that the main loop starts through the agent CLI's Agent tool, within the same agent session (AG-20). BUD-10 says how it runs and how its calls are counted.

**Workspace**:
The directory tree in which the implementer works. It lives for one attempt, or until a base update rebuilds it (12.4).

**Candidate**:
A commit the host builds from the workspace (9.4). Gates run on candidates; only candidates are pushed.

**Policy**:
The repository-owned configuration in `.factory/` on the default branch.

**Path pattern**:
A glob that names files of a repository by their path. VAL-5b says which lists hold path patterns and how one matches, VAL-5c what a pattern may hold and VAL-5d which spellings of a path are one name. VAL-5e names the rules of the integrity check that use one outside those lists.

**Exact path**:
The whole path of one file from the repository root, which the host took from a tree or from a package manifest such as `Cargo.toml`. It holds no wildcard, unlike a path pattern, and it is compared with a path of a diff after the fold of VAL-5d (VAL-5e).

**Locked path**:
The exact path of a file that `author_tests` wrote for a run (11.7). A change to it by the implementer fails the integrity check (VAL-5e).

**Check tool**:
A program that a repository's setup commands or checks run and that is no part of its toolchain (WS-7): `cargo-nextest` or `typos`, for example. WS-7a says how one reaches a worker.

**Instruction set**:
The files of a target repository that an agent session is given unasked: `AGENTS.md` and the files that the host admits by following instruction references from it, at the policy commit (AG-2a, AG-2b).

**Instruction reference**:
A name in a file of the instruction set that the host follows to another file of the repository. AG-2b says which names are one. It is not the reference of GH-5a, which is a mention of an issue or a pull request and keeps the bare word.

**Repository instructions**:
The text of the instruction set as the host gives it to an agent session (AG-2a, 10.4).

**Snapshot**:
The issue title, body and trusted comments captured at claim or resume, as sanitised (SEC-4) and given to agents. SEC-3 says which comments it holds.

**Target repository**:
A repository the factory is onboarded to (20) and works on.

**Factory repository**:
The repository that holds `sf` and its specification, `SPEC.md`. It is also a target repository, the first that an installed build of `sf` works on (BOOT-1).

**Scratch repository**:
`sapientpants/sf-scratch`, where live probes (23.2) and development builds of `sf` (BOOT-2) run.

**Factory's App**:
The GitHub App that an installed build of `sf` authenticates as (5.7). What the factory writes to a target repository is written under its identity.

**Scratch App**:
The GitHub App that a development build of `sf` (BOOT-2) and the end-to-end tests (19.4) authenticate as. It is installed on the scratch repository and on no other.
