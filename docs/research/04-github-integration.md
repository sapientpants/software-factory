# 04 - GitHub as the system of record

Research date: 2026-11-18. Angle: how the software factory should model its work queue, state, identity, event intake, PR flow and validation on GitHub, and which Rust client stack to use. Every claim carries a source number from the Sources section. Where I could not verify something against a live primary source it is marked **(unverified)** or **(inference)**.

## 1. Summary

1. GitHub now has much more native structure than a year ago: sub-issues, blocked-by dependencies, issue types, typed issue fields, and stacked pull requests are all generally available, and issue types/fields reached personal accounts on 2026-11-11 [9][10][12][13][14][31]. A native granular issue status (Backlog/Todo/In progress/In review) exists in the REST API but is still preview and enrolment-only [6][15]. The factory should use the GA features for structure and keep **labels** as the authoritative coarse state, because labels work on every plan and account type, are filterable in one REST call, and can only be set by users with triage/push access [6].
2. GitHub has **no compare-and-swap on issues** (no `If-Match` on issue update [6]). The only atomic primitives are in git: creating a ref, a fast-forward-only ref update, and `expectedHeadOid` on `createCommitOnBranch` / merge [20][21][22][27]. Claiming must therefore be done by a single dispatcher, fenced by a git ref, not by "add assignee and hope".
3. Authenticate as a **GitHub App**. Installation tokens last 1 hour and can be narrowed per repository and per permission [16]. Only Apps can create check runs [19]. Do not grant the App the `Workflows` permission, so agents cannot change CI definitions [18].
4. The binding rate limit for a factory is not the 5,000 requests/hour primary limit but the **secondary limits**: 100 concurrent requests, 900 REST points/minute (writes cost 5), and roughly 80 content-creating requests/minute and 500/hour [1][2]. All GitHub traffic must go through one governor.
5. GitHub **never redelivers failed webhooks automatically** and only keeps deliveries for 3 days [4][5]. Build a level-triggered reconcile loop first; treat webhooks as a latency optimisation.
6. Validation on GitHub Actions is fine, but agent-authored PRs are same-repository PRs, so they do not get the fork sandbox. Validation workflows must hold no secrets and a read-only token [24][25][26].

## 2. Work queue and state machine on Issues

### 2.1 What each GitHub feature is good for

| Feature | Status (Nov 2026) | Use in the factory | Why |
|---|---|---|---|
| Labels | Universal | **Authoritative coarse state** (`state:*`), holds, risk | One REST filter (`labels=`), `issues.labeled` webhook, and labels are silently dropped unless the actor has push access, so they are an authorization boundary [6][8] |
| Issue types | GA; orgs and personal accounts; up to 25 types; defaults Bug/Task/Feature [11][14] | Classification (`Feature`, `Bug`, `Task`, plus a custom `Epic`) | Typed, filterable (`type=` on list issues) [6]; not a state |
| Issue fields | GA for orgs 2026-07-02, personal accounts 2026-11-11; up to 25 fields; text/number/date/single-select/multi-select; default Priority, Effort, Start date, Target date [12][13][14] | Priority and Effort (human-set); optional machine fields | Typed and searchable; set via `issue_field_values` on create/update [6]. New, so keep the factory functional without them |
| Native issue status | **Preview, enrolled accounts only**. Create accepts Backlog/Todo/In progress/In review; update also Done/Not planned/Duplicate [6][15] | Do not depend on it; make `state:*` map 1:1 so it can be adopted later | "Ignored when issue statuses aren't available in the repository" [6] |
| Sub-issues | GA; 100 per parent, 8 levels [10][9]; REST endpoints and `sub_issues` webhook [9][8] | Decomposition: epic to tasks | Note the API takes the sub-issue's database `id`, not its number [9] |
| Dependencies | GA since 2025-08-21; 50 per relationship type; `is:blocked`, `blocked-by:` search; REST, GraphQL `addBlockedBy`, `issue_dependencies` webhook [7][31][8] | Scheduling order | Readiness = open, `state:ready`, no open blocker |
| Assignees | Up to 10 added per call; non-assignable users silently ignored [28] | Human ownership only | Whether an App bot is assignable is **unverified**; assignment is not atomic |
| Milestones | Universal | Optional release/batch grouping | Sub-issues inherit parent milestone and project [32] |
| Projects v2 | REST API since Sept 2025; 50 fields per project; `projects_v2_item` webhooks are **organization-only and public preview** [32][33][8] | Optional read-only dashboard projection | Not in the issue itself, extra API calls, no webhooks for user-owned projects |

Disagreement worth noting: several GitHub docs pages still describe issue fields as "public preview" while the changelog announces GA [12][13]. Trust the changelog, but feature-detect at startup.

### 2.2 Recommended label taxonomy

Exactly one `state:*` label on any issue the factory manages. No `state:*` label means "not factory work".

| Label | Meaning | Set by | Native status equivalent |
|---|---|---|---|
| `state:triage` | Filed, spec not yet accepted | human or factory intake | Backlog |
| `state:ready` | Human-approved for autonomous work (the gate) | **human with triage+ only** | Todo |
| `state:in-progress` | A run holds the lease and is planning/implementing | factory | In progress |
| `state:in-review` | PR open; waiting on checks, review or merge | factory | In review |
| `state:needs-human` | Factory stopped: question, repeated failure, policy stop | factory | (none) |
| closed / completed | PR merged via `Closes #n` | GitHub | Done |
| closed / not planned | Rejected | human | Not planned |

Orthogonal labels: `factory:hold` (per-issue kill switch, checked before every side effect), `risk:low|medium|high` (drives whether auto-merge is allowed), `needs:spec`, `needs:decision` (reason for `state:needs-human`). Classification goes in issue **type**, priority in the **Priority** issue field with a `priority:p0..p3` label fallback.

Transitions:

```
triage --(human adds state:ready)--> ready
ready --(dispatcher claims: lock ref + label swap)--> in-progress
in-progress --(draft PR ready, validation green)--> in-review
in-progress --(question / budget exhausted / N failures)--> needs-human
in-review --(changes requested or checks red)--> in-progress   (same branch, new attempt)
in-review --(PR merged)--> closed:completed
needs-human --(human answers and re-adds state:ready)--> ready
any --(factory:hold added)--> frozen in place; (issue closed)--> terminal, lease released
```

Rules: use the add-label and remove-label endpoints, never `PATCH issue {labels}`, because that replaces the whole set and would clobber concurrent human edits [6]. Only honour `state:ready` when the `issues.labeled` event's sender has write/triage access; since GitHub drops labels from users without push access this holds by construction for label events [6], but slash-command comments must be checked against `author_association` explicitly.

Prior art: Prow's Tide selects merge candidates by GraphQL search over PR state and blocks merges through specially labelled issues [36]; Renovate uses a Dependency Dashboard issue whose checkboxes drive actions [37]. Both show the "labels and issue bodies as control surface" pattern at scale; neither stores a lease in an issue.

### 2.3 Where machine state lives

| Store | Recommendation | Evidence / limits |
|---|---|---|
| `state:*` label | Coarse state, authoritative | above |
| One "factory status" comment per issue, edited in place | Run ledger: human-readable summary plus a hidden `<!-- factory:state v1 {json} -->` block (run id, attempt, branch, PR number, lease expiry, last error) | Trust the marker only on comments whose author is the factory's own bot account, otherwise anyone can spoof state. Body limit about 65,536 characters (secondary source, **unverified** against official docs) [38]. Comments can now be pinned via API (octocrab exposes `pin_comment`; `issue_comment` has `pinned` actions) [8][35] |
| Append-only event comments | Only for things a human must see: question, PR opened, gave up | Each one counts toward the content-creation limit and notifies subscribers [1][6] |
| Issue body sections | Human-owned spec. The factory should not rewrite the body, except an optional generated "Acceptance criteria" block between markers | Avoids edit wars with humans |
| Check runs on the PR head SHA | Validation verdict per commit: `factory/validate`, `external_id` = run id, summary/text up to 65,535 chars each, 50 annotations per request, up to 3 action buttons that fire `check_run.requested_action` | Apps only [19]. Rulesets can pin a required check to a specific source App, so nothing else can satisfy it [17] |
| Commit trailers | Provenance in git: `Factory-Issue: #n`, `Factory-Run: <id>`, `Co-authored-by:` | Squash merge discards individual commit authorship [39], so repeat provenance in the PR body |
| Git ref | Mutual exclusion (claim) | see 5 |
| State branch (orphan `factory/state`) | Not in v1. Consider later for bulky run archives | Needs ruleset exclusions and CI path filters; a second store to reconcile |
| Git notes | Avoid | Not fetched by default and not surfaced in the GitHub UI (**unverified**, from memory) |
| Projects v2 fields | Projection only | see 2.1 |

Principle: everything the factory needs to resume after losing its local disk must be recoverable from labels + status comment + refs + PRs + check runs. A local SQLite cache is allowed, but it is a cache.

## 3. Authentication, identity and attribution

**Use a GitHub App, not a PAT.** GitHub's own guidance: Apps have fine-grained permissions, short-lived tokens, are not tied to a user or a seat, have centralized webhooks and rate limits that scale [40]. Mechanics:

- App JWT: RS256, `exp` at most 10 minutes ahead, `iat` backdated 60 seconds [41].
- Installation token: `POST /app/installations/{id}/access_tokens`, expires after 1 hour, can be narrowed with `repositories`/`repository_ids` (max 500) and `permissions` [16].
- Since 2026-04-27 GitHub is rolling out a new stateless token format `ghs_APPID_JWT`; do not assume 40-character tokens anywhere (regexes, redaction, DB columns) [16].

**Recommended permissions.** Repository: Metadata read, Contents write, Issues write, Pull requests write, Checks write, Actions read, Commit statuses read, Merge queues read (required for the `merge_group` event [8]). Organization: Projects write only if the projection is enabled. **Not granted: Workflows** (editing `.github/workflows` requires it [18]) and Administration. Exact permission names for sub-issue, dependency and issue-field endpoints are not stated in the docs pages I read; discover them with the `X-Accepted-GitHub-Permissions` response header [18].

**Keep the App out of every ruleset bypass list.** Rulesets allow Apps as bypass actors [17]; the factory must be subject to the same rules as a human.

**Commit attribution and signing.** Two publish paths:

1. `git push` over HTTPS with an installation token, author set to `<bot-user-id>+<app-slug>[bot]@users.noreply.github.com` [42]. Commits are attributed to the bot but **unsigned**.
2. Create commits through the API. GitHub marks bot commits verified only when the request is authenticated as the App and carries no custom author, committer or signature [23]. GraphQL `createCommitOnBranch` is "automatically signed by GitHub if supported", authored by the credential owner, and takes `expectedHeadOid`, which makes the publish a compare-and-swap [20][21].

Recommendation: agents commit locally in their sandbox with no GitHub credential; the orchestrator publishes. Default to path 2 so commits are verified and a "require signed commits" rule can be used; GitHub warns that unsigned commits in a PR can block a squash merge under that rule even though GitHub signs the final commit [17]. Keep path 1 as fallback for very large or binary changes (payload limits of `createCommitOnBranch` are **unverified**). GitHub's own Copilot cloud agent is built the same way: it "cannot directly run `git push`", its commits are signed, it pushes to a single `copilot/` branch, and it "cannot mark its pull requests as Ready for review and cannot approve or merge" [29].

**Reviews.** A PR author cannot approve its own PR, so the factory App cannot self-approve. Community sources say an approving review from a different App does count toward required approvals [43] (medium confidence). Code owners must be individuals or teams with write access [44], so a code-owner requirement keeps a human in the loop (**inference**: the docs do not explicitly exclude Apps).

## 4. Event intake

Facts:

- Respond 2xx within 10 seconds or the delivery counts as failed; payloads over 25 MB are not delivered [3][8].
- "GitHub does not automatically redeliver failed deliveries" [4]. Redelivery is manual or scripted via `GET /app/hook/deliveries` and `POST /app/hook/deliveries/{id}/attempts` (JWT auth), limited to the past 3 days [5][45].
- Validate `X-Hub-Signature-256` (HMAC-SHA256, constant-time compare) and deduplicate on `X-GitHub-Delivery`, which is preserved across redeliveries [3][46].
- The Events API is not a substitute: latency "anywhere from 30s to 6h", 300 events/30 days [47].
- Conditional requests: a 304 with a valid `Authorization` header does not count against the primary rate limit [48].

Recommendation: a **level-triggered reconciler** is the source of truth for the factory's view. Every N seconds it lists `GET /repos/{o}/{r}/issues?state=all&sort=updated&since=<watermark>` with `If-None-Match`, then refreshes only changed items. Webhooks, when configured, merely enqueue "reconcile issue #n now". This gives one code path, makes missed or out-of-order deliveries harmless, and lets a single developer run the factory on a laptop with no inbound endpoint. With webhooks enabled, also run a periodic job that lists failed deliveries and redelivers them [4].

Subscribe to: `issues`, `issue_comment`, `sub_issues`, `issue_dependencies`, `pull_request`, `pull_request_review`, `pull_request_review_thread`, `check_run`, `check_suite`, `workflow_run`, `merge_group`, `push`, `installation` [8].

## 5. Claiming and concurrency

There is no atomic "claim" on an issue: no `If-Match` on update [6], label and assignee writes are last-writer-wins, and assignees can be silently ignored [28]. Patterns such as "add a label, re-read, back off if someone else also labelled" are races with a smaller window.

Recommendation, in order of importance:

1. **One dispatcher.** A single orchestrator process decides which run gets which issue; workers never pick work themselves. This removes the distributed problem for v1.
2. **Git ref as fence.** Before the label swap, the dispatcher creates the work branch `refs/heads/factory/issue-<n>` with `POST /git/refs`. Ref creation is atomic in git; the API returns an error if the ref already exists (the docs list 409 and 422 without saying which applies; "422 Reference already exists" is the commonly observed response, **unverified here**, so test it) [22]. If creation fails, another instance owns the issue.
3. **Idempotency keys everywhere.** Deterministic branch name per issue, at most one open PR per head branch, status comment located by stored comment id, check runs keyed by `external_id`. A crashed and restarted run converges instead of duplicating.
4. **Leases with expiry** in the status comment (`lease_expires_at`, heartbeat on each step). The reconciler reclaims expired leases and moves the issue back to `state:ready` or `state:needs-human`.
5. **CAS on every code write**: `expectedHeadOid` on commit [20], `sha` on merge (409 on mismatch) [27], `expected_head_sha` on update-branch [27].

If multiple orchestrator instances are ever needed, upgrade the fence to a lease chain: a ref whose commits record holder and expiry, taken over with a fast-forward-only `PATCH /git/refs` (`force: false` "will make sure you're not overwriting work") [22].

## 6. REST vs GraphQL and rate limits

Limits (verified 2026-11-18):

| Limit | Value | Source |
|---|---|---|
| REST primary, App installation | 5,000/h minimum; +50/h per repo and per org user above 20, capped at 12,500; 15,000/h for Enterprise Cloud org installations | [1] |
| GraphQL primary, App installation | 5,000 points/h (same scaling to 12,500); 10,000 on Enterprise Cloud | [2] |
| `GITHUB_TOKEN` in Actions | 1,000 requests/h per repository | [1] |
| Concurrency | 100 concurrent requests, REST + GraphQL combined | [1] |
| Per-minute points | 900/min REST, 2,000/min GraphQL; reads 1 point, writes 5 | [1][2] |
| Content creation | "generally" 80/min and 500/h, some endpoints lower | [1] |
| CPU time | 90 s per 60 s real time (60 s GraphQL) | [1] |
| GraphQL shape | `first`/`last` 1-100, 500,000 nodes per call, 10 s timeout | [2] |

GitHub's guidance: make requests serially, wait at least 1 second between mutating requests, honour `retry-after`, otherwise wait until `x-ratelimit-reset` or at least one minute, then back off exponentially; continuing while limited "may result in the banning of your integration" [48]. Secondary-limit status cannot be queried [1].

Consequences for a many-agent factory:

- Ten agents that each post a progress comment per minute would hit the 500/h content-creation cap. Edit the single status comment instead of appending, and budget append-only comments per run.
- All tokens minted for one installation share that installation's budget (**inference** from the limits being stated per installation [1]); minting more tokens adds nothing.
- Build one in-process **GitHub gateway**: a serial mutation queue with at least 1 s spacing, a bounded read pool, a token bucket fed from `x-ratelimit-*` headers, an ETag cache, and priority classes (merge and claim before cosmetic updates). Agents never call GitHub directly.

REST vs GraphQL: use **REST** for writes and simple reads (typed in octocrab, documented per endpoint, supports ETags) and **GraphQL** for composite reads (issue + labels + sub-issues + blockers + linked PR + check rollup in one call) and for operations that are GraphQL-only or better there: `createCommitOnBranch`, `enablePullRequestAutoMerge`, `enqueuePullRequest`, `markPullRequestReadyForReview`, `convertPullRequestToDraft` [20][34]. Pin `X-GitHub-Api-Version: 2026-03-10`; without the header requests default to `2022-11-28`, which is supported until 2028-03-10 [49].

## 7. Pull request workflow

1. Branch `factory/issue-<n>` from the default branch; open a **draft** PR early with `Closes #<n>` in the body. Closing keywords only work when the PR targets the default branch [50].
2. When local validation passes, mark ready (`markPullRequestReadyForReview`) and move the issue to `state:in-review`.
3. Protect the default branch with a **ruleset** (multiple rulesets can layer; branch protection allows one rule) [51]: require PR, required approvals, dismiss stale approvals, "require approval of the most recent reviewable push", required status checks pinned to their source App, block force pushes [17].
4. CODEOWNERS for `.github/`, CODEOWNERS itself, factory configuration and agent instruction files; GitHub recommends owning `/.github/` explicitly [44][24].
5. Merge with the asynchronous merge API `PUT /pulls/{n}/merge-async` (with `sha`), which merges directly or enqueues depending on configuration and is "the required API for merging stacked pull requests"; the classic merge endpoint supports neither stacks nor merge queues [27]. Poll the returned UUID; results are retained 24 hours [27].
6. **Merge queue** when available: workflows must also trigger on `merge_group` or required checks never report [52][26]. Availability is restricted to organization-owned public repositories, or private repositories on Enterprise Cloud [53]. For other setups the factory needs its own serialiser: strict "up to date" required checks plus one-at-a-time update-branch and auto-merge.
7. **Auto-merge** only merges once required reviews and checks pass and is disabled if someone without write access pushes to the head branch [54]. Enable it from the factory only when policy (`risk:*`) allows.
8. Stacked PRs are GA (2026-10-06), land through the merge queue as one group, and emit a `pull_request.stacked` webhook action; creation is documented only through the `gh stack` CLI extension, not an API [31a]. Treat as a later optimisation.

Plan caveat: on private repositories, protected branches, required reviewers and code owners are listed for Pro and Team, not for Free [55]. The spec must state the minimum plan or default to a public or Team-plan organization.

## 8. GitHub Actions as validation runner

What works in the factory's favour: events caused by `GITHUB_TOKEN` do not start workflows, but pushes and PRs made with an App installation token do, so CI runs normally on factory PRs [25].

Pitfalls, with mitigations:

- **Same-repo PRs are not sandboxed.** Secrets are withheld and the token is read-only only for fork PRs (and Dependabot PRs, which are treated as forks) [26]. Factory branches live in the repository, so a `pull_request` workflow runs agent-written code (tests, `build.rs`, proc macros) with whatever the job is given (**inference** from the fork-only wording). Mitigate: top-level `permissions: contents: read`, no secrets in validation workflows, deployment secrets only behind environments with required reviewers [24].
- **Workflow tampering.** Without the Workflows permission the App cannot push changes under `.github/workflows` [18]; add CODEOWNERS on `.github/` as a second layer [24].
- **Script injection.** Never interpolate `${{ github.event.* }}` text (issue titles, PR bodies, branch names) into `run:`; pass through `env:` [24].
- **`pull_request_target` / `workflow_run`.** Avoid. `actions/checkout` v7 refuses fork PR checkouts in those contexts (June 2026), and from 2026-12-02 `pull_request_target` is blocked by default in public repositories unless an Actions event policy allows it [56][57].
- **Prompt injection.** Issue, PR and comment text is attacker-controlled input to the agent. Aikido's "PromptPwnd" documented real compromises where such text reached an agent holding a privileged token [58]. GitHub's Agentic Workflows respond with zero-secret agents, an egress firewall and "safe outputs": the agent only buffers proposed writes, and a separate job validates and applies them with volume limits [59]. Adopt the same split: the agent sandbox has no GitHub credential; the orchestrator applies a typed, validated set of outputs.
- **Runners.** Self-hosted runners "can be persistently compromised by untrusted code"; use GitHub-hosted or just-in-time ephemeral runners, and pin third-party actions to full commit SHAs [24].

## 9. Rust client options

| Option | Assessment |
|---|---|
| **octocrab** 0.54.9 (docs.rs latest) | Active. Typed handlers already cover sub-issues, blocked-by dependencies, issue field values, comment pinning, checks, `projects_v2`; App JWT auth with cached, auto-refreshed, repository-scoped installation tokens; `graphql()`; raw `_get/_post`; `etag` module; webhook payload types (marked beta, not all events typed) [35][60][61]. Gaps: no built-in rate limiting (its own docs warn "There's no rate limiting so be careful") [60]; pre-1.0, so minor releases may break (registry listings show 0.49.x through 0.54.x; 0.54.1 was published 2026-07-24) [62] |
| octorust (generated from OpenAPI) | Latest 0.10.0; a crates mirror shows last update December 2024. Looks stale; not recommended [62] |
| Raw `reqwest` + serde, GraphQL via `graphql_client` or `cynic` | Maximum control, most code to own. Status of the GraphQL crates not verified in this pass |
| Shelling out to `gh` | Good for prototypes and as an agent-facing tool (`gh issue create --blocked-by`, `gh stack`) [7][31a]; poor for the core: process spawning, text parsing, no central rate governance |

Recommendation: define a narrow `GitHub` port (trait) owned by the factory, with domain-level operations (`claim_issue`, `publish_commit`, `upsert_status_comment`, `set_state`, `open_draft_pr`, `report_check`, `merge`). Implement it with octocrab for auth and typed REST, `octocrab.graphql()` with hand-written queries and serde structs for the few composite reads and GraphQL-only mutations, all behind the gateway from section 6. Ship an in-memory fake of the port for tests; pin octocrab exactly and upgrade deliberately.

## 10. Risks and open questions

- Native issue status may go GA and overlap `state:*` labels; keep the mapping 1:1.
- Whether a GitHub App bot can be an issue assignee is unverified; the design does not rely on it.
- Exact error code for duplicate ref creation, payload limits for `createCommitOnBranch`, and comment body limits need integration tests against a scratch repository.
- Personal account versus organization: merge queue and Projects webhooks are organization-only [53][8]. The spec should recommend an organization.
- Whether comment edits count toward the content-creation limit is undocumented [1].

## Sources

1. https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
2. https://docs.github.com/en/graphql/overview/rate-limits-and-query-limits-for-the-graphql-api
3. https://docs.github.com/en/webhooks/using-webhooks/best-practices-for-using-webhooks
4. https://docs.github.com/en/webhooks/using-webhooks/handling-failed-webhook-deliveries
5. https://docs.github.com/en/webhooks/testing-and-troubleshooting-webhooks/redelivering-webhooks
6. https://docs.github.com/en/rest/issues/issues
7. https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/creating-issue-dependencies and https://docs.github.com/en/rest/issues/issue-dependencies
8. https://docs.github.com/en/webhooks/webhook-events-and-payloads
9. https://docs.github.com/en/rest/issues/sub-issues
10. https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/adding-sub-issues and https://github.blog/changelog/2025-04-08-evolving-github-issues-and-projects/
11. https://docs.github.com/en/issues/tracking-your-work-with-issues/configuring-issues/managing-issue-types-in-an-organization
12. https://github.blog/changelog/2026-07-02-issue-fields-are-now-generally-available
13. https://docs.github.com/en/issues/planning-and-tracking-with-projects/understanding-fields/about-issue-fields
14. https://github.blog/changelog/2026-11-11-issue-types-and-fields-are-available-for-personal-accounts
15. https://github.com/github/roadmap/issues/1336
16. https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app
17. https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets
18. https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/choosing-permissions-for-a-github-app
19. https://docs.github.com/en/rest/checks/runs
20. https://docs.github.com/en/graphql/reference/commits
21. https://gist.github.com/brasic/0c3f587e1f43a57eb1fd158ceb707422 (stale `expectedHeadOid` error example)
22. https://docs.github.com/en/rest/git/refs
23. https://docs.github.com/en/authentication/managing-commit-signature-verification/about-commit-signature-verification
24. https://docs.github.com/en/actions/reference/security/secure-use
25. https://docs.github.com/en/actions/concepts/security/github_token
26. https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows
27. https://docs.github.com/en/rest/pulls/pulls
28. https://docs.github.com/en/rest/issues/assignees
29. https://docs.github.com/en/copilot/concepts/agents/cloud-agent/risks-and-mitigations
30. https://docs.github.com/en/copilot/concepts/agents/coding-agent/about-coding-agent
31. https://github.blog/changelog/2025-08-21-dependencies-on-issues/
31a. https://github.blog/changelog/2026-10-06-stacked-pull-requests-generally-available
32. https://github.blog/changelog/2025-09-11-a-rest-api-for-github-projects-sub-issues-improvements-and-more/
33. https://docs.github.com/en/issues/planning-and-tracking-with-projects/learning-about-projects/about-projects
34. https://docs.github.com/en/graphql/reference/pulls
35. https://docs.rs/octocrab/latest/octocrab/issues/struct.IssueHandler.html
36. https://docs.prow.k8s.io/docs/components/core/tide/
37. https://docs.renovatebot.com/key-concepts/dashboard
38. https://github.com/dead-claudia/github-limits (secondary)
39. https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/about-merge-methods-on-github
40. https://docs.github.com/en/apps/creating-github-apps/about-creating-github-apps/deciding-when-to-build-a-github-app
41. https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-json-web-token-jwt-for-a-github-app
42. https://jvt.me/posts/2023/04/20/github-app-email-address (secondary)
43. https://github.com/orgs/community/discussions/181487 (community)
44. https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners
45. https://docs.github.com/en/rest/apps/webhooks
46. https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliveries
47. https://docs.github.com/en/rest/activity/events
48. https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api
49. https://docs.github.com/en/rest/about-the-rest-api/api-versions
50. https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/linking-a-pull-request-to-an-issue
51. https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets
52. https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue
53. https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/merging-a-pull-request-with-a-merge-queue and https://github.blog/changelog/2023-07-12-pull-request-merge-queue-is-now-generally-available/
54. https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/incorporating-changes-from-a-pull-request/automatically-merging-a-pull-request
55. https://docs.github.com/en/get-started/learning-about-github/githubs-plans
56. https://github.blog/changelog/2026-06-18-safer-pull_request_target-defaults-for-github-actions-checkout
57. https://github.blog/changelog/2026-11-02-pull_request_target-blocked-by-default-december-2
58. https://aikido.dev/blog/promptpwnd-github-actions-ai-agents
59. https://github.blog/ai-and-ml/generative-ai/under-the-hood-security-architecture-of-github-agentic-workflows/ and https://github.github.com/gh-aw/reference/safe-outputs/
60. https://docs.rs/octocrab/latest/octocrab/
61. https://docs.rs/octocrab/latest/octocrab/struct.Octocrab.html
62. https://docs.rs/crate/octocrab/0.54.1 and https://docs.rs/crate/octorust/latest/features
