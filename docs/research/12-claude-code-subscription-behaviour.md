# 12 - Claude Code on a subscription: limits, usage reporting, plan-limit behaviour and network hosts

Research date: 2026-11-18. Answers probe P-22 (issue #9). Scope: `claude -p`, non-bare, authenticated with `CLAUDE_CODE_OAUTH_TOKEN` from `claude setup-token` on a Claude Max plan, behind a default-deny egress proxy.

Sources are primary only: the official Claude Code documentation fetched as markdown from code.claude.com on 2026-11-18, the published type declarations of the official TypeScript Agent SDK (`sdk.d.ts`, version 0.3.311, which matches the CLI version the spec pins), the official Python Agent SDK source on GitHub, and `claude --help` of the locally installed CLI (2.1.311, Homebrew). No `claude` session was started, so nothing here is observed behaviour. Numbers in brackets refer to the Sources section. Quoted sentences are verbatim. Where the documentation does not state something, the text says "not documented".

## 1. Answer in short

1. **Caps.** `--max-turns` and `--max-budget-usd` are documented as print-mode flags with no condition on the authentication method. The budget cap is checked against a client-side estimate computed from token counts at list price, so on a subscription it caps API-equivalent dollars, not money. No sentence says in so many words "these work with a subscription token"; that is an inference from the absence of any restriction, backed by statements that the cost ledger is computed locally on a plan.
2. **Usage reporting.** The `result` line carries `total_cost_usd`, `usage`, `modelUsage`, `num_turns` and timings. Each `assistant` line carries the API's `usage` object, but its `output_tokens` is a placeholder. All dollar figures are estimates and, on a plan, "not what your plan bills". Nothing in the stream reports plan quota consumed by the session, except the `utilization` fraction on `rate_limit_event`.
3. **Plan limit.** The stream has a dedicated `rate_limit_event` message, documented as "Rate limit information for claude.ai subscription users", with `status: "rejected"`, a `resetsAt` Unix timestamp and a `rateLimitType` naming the window. The failed turn also produces an `assistant` message with `error: "rate_limit"` and a `result` with `subtype: "success"`, `is_error: true` and `api_error_status`. A `-p` run does not wait for the reset unless `CLAUDE_CODE_RETRY_WATCHDOG` is set. The exit code is documented only as "non-zero". An API rate limit differs: it is retried with backoff and announced by `system/api_retry` events, which carry no reset time.
4. **Hosts.** The only host documented for model requests is `api.anthropic.com`. The documentation's host table is not split by authentication method, and does not say whether a session using a pre-issued `CLAUDE_CODE_OAUTH_TOKEN` ever contacts `platform.claude.com` or `claude.ai`.
5. **Switches.** `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` turns off auto-updates, telemetry, error reporting, feedback, release notes and availability checks in one variable. It does not cover the WebFetch domain check or official-marketplace auto-install, which have their own switches. Telemetry and feature-flag fetches go to `api.anthropic.com` itself, so a hostname allowlist cannot separate them from model requests; only the variables can.

One finding qualifies the spec: usage credits, if turned on for the account, "let you keep working past your plan's usage limit", and in `-p` there is no consent prompt before a request is billed to them (section 4.6). "No dollar bill can run away" (13.5) holds only while usage credits are off.

## 2. Do `--max-turns` and `--max-budget-usd` apply under subscription authentication?

### 2.1 What is documented

- `--max-turns`: "Limit the number of agentic turns (print mode only). No limit by default. With `--input-format stream-json`, a message still queued when the limit ends a turn stays queued and starts a new turn with its own limit" [1].
- `--max-budget-usd`: "Stop the run once estimated spend on API calls reaches this amount (print mode only). Claude Code checks the cap against its client-side cost estimate, which can differ from your bill. Spend from subagents counts toward the cap. Spend can pass the cap, so leave headroom." [1]
- Neither row, nor the SDK option rows for `maxTurns` and `maxBudgetUsd` [6], names an authentication method. The local `--help` text for the budget flag is "Maximum dollar amount to spend on API calls (only works with --print)" [15]. The same help text does mark a flag that depends on authentication: `--betas` is "(API key users only)" [15]. The caps carry no such mark.
- `--max-turns` does not appear in `claude --help` of 2.1.311 at all [15]; it is documented in the CLI reference [1]. This agrees with the note in AG-1 that `--help` omits supported flags.
- The cost ledger the budget cap reads is computed locally and exists on a plan. The usage-report type says of its `session` block, "read from the same ledger as `total_cost_usd` and `modelUsage` on `SDKResultMessage`": "Claude Code computes `session.total_cost_usd` locally from token counts, so it is an estimate and not what your plan bills." [6] The costs page says: "Claude Code computes the dollar figure locally from token counts at list price, unless a `modelPricing` table is in effect." and "Claude Pro, Max, and Ultra subscribers have usage included in their subscription, so the session cost figure isn't relevant for billing purposes." [4]

### 2.2 How the caps behave

- Turn counting: "Count each turn with tool calls, plus one for the final text-only response. Without that extra turn, the run ends with a `ResultMessage` of subtype `error_max_turns` and no final response." [8]
- Budget overshoot: "Claude Code compares spend with the `max_budget_usd` / `maxBudgetUsd` cap after model responses arrive, because each response's cost comes from the token usage the API returns with it. The response that reaches the cap still completes and counts toward `total_cost_usd`. Spend can therefore pass the cap by up to the cost of that one response, plus anything that subagents still running at that moment spend before they stop." [8]
- Result shape: "When either limit is hit, the SDK returns a `ResultMessage` with a corresponding error subtype (`error_max_turns` or `error_max_budget_usd`)." [8] `terminal_reason` has the values `max_turns` and `budget_exhausted` [6]. "The underlying Claude Code process also exits with a nonzero code." [8]
- With `--input-format stream-json` (which AG-1 uses) the two caps have different scope: "the max-turns count starts over for that turn. The budget total keeps accumulating across messages, and once spend reaches `maxBudgetUsd`, later messages in the same conversation end with the `error_max_budget_usd` result." [8]
- Resume: "`maxBudgetUsd` (TypeScript) or `max_budget_usd` (Python) counts only the call's own spend: totals restored from a resumed session don't count against it, and a `/clear` starts the budget over." [7]
- `CLAUDE_CODE_MAX_TURNS` is an environment equivalent: "Cap the number of agentic turns when no explicit limit is passed. Equivalent to passing `--max-turns`, which takes precedence when both are set." [3]

### 2.3 Not documented

- No sentence states that either flag is honoured under `CLAUDE_CODE_OAUTH_TOKEN`. The conclusion rests on: no documented restriction, the `--help` convention of marking API-key-only flags, and the statements that the estimate is computed locally on a plan.
- Whether `total_cost_usd` is ever zero or absent under a subscription token is not documented. The only documented zero case is a crash result [7].

Confidence: high for `--max-turns` (a loop counter with no billing input); medium-high for `--max-budget-usd`.

## 3. Usage and cost fields in stream-JSON

### 3.1 The `result` message

Type definition: `SDKResultMessage` [6], declared as `SDKResultSuccess | SDKResultError` in `sdk.d.ts` [10]. "The last line of the stream is a `result` message with the final response text, cost, and session metadata." [2]

| Field | What it holds | Scope |
|---|---|---|
| `total_cost_usd` | "cumulative estimated cost in USD, covering the same calls as `modelUsage` and reset at the same points ... It is an estimate, not a billing statement." [6] | Main loop, subagents and internal calls such as compaction; cumulative |
| `usage` | The API `BetaUsage` shape: `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `cache_creation.ephemeral_5m_input_tokens`, `cache_creation.ephemeral_1h_input_tokens`, `server_tool_use`, `service_tier`, `speed`, `inference_geo`, `iterations`, `output_tokens_details.thinking_tokens`, `fallback_credit` [6] | "main agent loop only. Excludes subagent and auxiliary model calls, and is per-turn in streaming-input sessions. Prefer `modelUsage` for token/cost accounting." [6] |
| `modelUsage` | Map of model name to `inputTokens`, `outputTokens`, `thinkingTokens?`, `cacheReadInputTokens`, `cacheCreationInputTokens`, `webSearchRequests`, `costUSD`, `contextWindow`, `maxOutputTokens`, `canonicalModel?`, `provider?`, `costBasis?` [6] | "per-model totals for every model call made through the query pipeline during this `query()` call, including the main loop, subagents, and internal calls such as compaction and Workflow agents. Helper calls outside that pipeline, such as the permission classifier and token-counting requests, are excluded." [6] |
| `num_turns`, `duration_ms`, `duration_api_ms` | Turn count and timings | The call |
| `is_error`, `api_error_status`, `stop_reason`, `terminal_reason`, `errors` | How the run ended (section 4) | The call |
| `permission_denials` | Denied tool calls | The call |

Points that matter for the factory's accounting:

- **Estimates.** "The `total_cost_usd` and `costUSD` fields are client-side estimates, not authoritative billing data. The SDK computes them locally from a price table bundled at build time, unless a `modelPricing` table is in effect." [7] `costBasis` is "`list` for list price, `managed` for a `modelPricing` table, or `unknown` when neither matched the model ID" [6]. `modelPricing` is read from managed settings only [14], so `--settings` cannot change the rates.
- **Read the latest result; do not sum.** With streaming input "the totals are cumulative across turns, so read the latest result rather than summing across results" [6].
- **Resumed sessions carry earlier spend.** "Claude Code saves the session's totals to its transcript when the process exits normally and restores them when a later call resumes or forks the session. Each result already includes the session's earlier spend. Read the latest result for the session total; summing results double-counts the restored spend. Before v2.1.277, a session that you resumed through the SDK or `claude -p` started its totals at zero" [7]. A stage that resumes a session must subtract the previous total.
- **Error results still carry cost, with two exceptions.** "Both success and error result messages include `usage` and `total_cost_usd`" [7]. After a crash "every cost field may be zeroed", and on `error_max_budget_usd` "`usage` leaves out the response that crossed the budget, while `total_cost_usd` and `modelUsage` include it." [7]
- **SIGTERM gives no result at all** [2], so a session stopped that way has no final totals (already recorded as fact-check claim 51).

### 3.2 Per-message events

- **`assistant`** (`SDKAssistantMessage`): `message` is the API message, with `message.id` and `message.usage`. "When Claude uses multiple tools in one turn, all messages in that turn share the same ID, so deduplicate by ID to avoid double-counting." [7]
- **Output tokens on `assistant` messages are not real.** "Claude Code builds each assistant message from the usage the API reported when the response began, so the message's `output_tokens` is only the count the API had reported at `message_start`, before the response was generated." and "The deduplicated per-step values are accurate for input and cache tokens. Per-step `output_tokens` is a placeholder" [7]. The real count is added to the result message only.
- **No per-message dollar figure exists.** Cost appears only as `total_cost_usd` and `modelUsage[*].costUSD` on the result [6][7].
- **`stream_event`** (only with `--include-partial-messages`): the raw API stream events. `message_delta` carries "Message-level updates (stop reason, usage)" [9], which is where the API reports the final output count of one response. "Stream events are emitted for the main session only; token-level deltas from subagents aren't forwarded." [9] After a dropped connection or a non-streaming fallback `message_delta` "can be missing" [9].
- **Subagent messages** appear as `assistant` and `user` lines with `parent_tool_use_id` set [2]; their tokens are in `modelUsage` and `total_cost_usd` but not in the result's `usage` [7].
- **`system/init`** carries `apiKeySource`. For a claude.ai login it is `none`: "No API key. The session authenticates another way, such as a claude.ai login, a bearer token, or a cloud provider" [6]. The value `oauth` is in the type but "Claude Code doesn't report" it [6]. So the init event does not positively identify subscription authentication. `claude auth status` does: its JSON `authMethod` is "one of `none`, `claude.ai`, `oauth_token`, `api_key`, `api_key_helper`, or `third_party`" [1].

### 3.3 What the figures mean on a subscription

- Dollars are list-price equivalents computed on the client; "not what your plan bills" [6]. Section 14 of the spec already treats them this way.
- Token counts are the API's own, so they are real, subject to the placeholder caveat above.
- The stream has no field for "plan allowance consumed by this session". The nearest signals are:
  - `rate_limit_event.rate_limit_info.utilization`, "Fraction of the rate limit consumed (0.0 to 1.0)" [11], for the window named by `rateLimitType`; it is account-wide, not per session.
  - The experimental `usage_report` that an `assistant` message carries when the prompt is `/usage`. Its `rate_limits` block has the plan's rows (`kind`, `percent`, `resets_at` as ISO 8601). "It is `null` when Claude Code couldn't get the plan's usage, for example when the session's OAuth token lacks the `user:profile` scope." [6] The `setup-token` token "can only make model requests" [5]; whether it has that scope is not documented.
- Prompt caching differs by plan state: "On a Claude subscription within your plan's included usage, you get the 1-hour TTL on your own turns ... and Claude Code drops those turns to the 5-minute TTL once you're drawing on usage credits." [7] This changes the split between `ephemeral_1h_input_tokens` and `ephemeral_5m_input_tokens`, and so the estimate.

Confidence: high on the field list and semantics (type definitions and reference text agree); medium on anything specific to a `setup-token` credential, which the documentation barely distinguishes from a `/login` credential.

## 4. What the CLI emits when a plan usage limit is reached

The documentation describes the pieces separately. It has no single description of the sequence in a `-p --output-format stream-json` run.

### 4.1 `rate_limit_event`

The published type declarations, version 0.3.311 [10]:

```typescript
/** Rate limit event emitted when rate limit info changes. */
export declare type SDKRateLimitEvent = {
    type: 'rate_limit_event';
    /** Rate limit information for claude.ai subscription users. */
    rate_limit_info: SDKRateLimitInfo;
    uuid: UUID;
    session_id: string;
};

export declare type SDKRateLimitInfo = {
    status: 'allowed' | 'allowed_warning' | 'rejected';
    resetsAt?: number;
    rateLimitType?: 'five_hour' | 'seven_day' | 'seven_day_opus' | 'seven_day_sonnet' | 'seven_day_overage_included' | 'overage';
    utilization?: number;
    overageStatus?: 'allowed' | 'allowed_warning' | 'rejected';
    overageResetsAt?: number;
    overageDisabledReason?: 'overage_not_provisioned' | 'org_level_disabled' | 'org_level_disabled_until' | 'out_of_credits' | /* ...nine more... */ 'unknown';
    isUsingOverage?: boolean;
    overageInUse?: boolean;
    overageEnabled?: boolean;
    surpassedThreshold?: number;
    limitScope?: 'service' | 'channel' | 'group_pool';
    errorCode?: 'credits_required';
    canUserPurchaseCredits?: boolean;
    hasChargeableSavedPaymentMethod?: boolean;
};
```

- The wire names are camelCase inside `rate_limit_info`. The Python SDK parser reads exactly `status`, `resetsAt`, `rateLimitType`, `utilization`, `overageStatus`, `overageResetsAt`, `overageDisabledReason` from the CLI's JSON line and keeps the whole object as `raw` [12].
- Meaning of `status`: "`allowed` means within the limit; `allowed_warning` means approaching the limit; `rejected` means the limit has been hit." [12]
- Reset time: `resetsAt` is the "Unix timestamp when the rate limit window resets" [11]. It is optional in the type.
- When it is emitted: "The CLI emits this whenever the rate limit status transitions (e.g. from `allowed` to `allowed_warning`). Use this to warn users before they hit a hard limit, or to gracefully back off when `status == "rejected"`." [12] The TypeScript reference page says "Emitted when the session encounters a rate limit." [6]
- `errorCode: "credits_required"`: "the rejection is from a claude.ai subscription whose included usage is exhausted, and the session cannot continue until the user buys usage credits." [6]
- The reference page's own listing of this type [6] is a subset of the declaration above: it omits `rateLimitType` and the `overage*` fields. The declaration file and the Python SDK agree with each other, so they are taken as authoritative.

### 4.2 The failed turn

- The `assistant` message for the failed request carries `error: "rate_limit"`: "A 429 against your quota is `'rate_limit'`" [6].
- Its text is one of: `You've hit your session limit · resets 3:45pm`, `You've hit your weekly limit · resets Mon 12:00am`, `You've hit your Opus limit · resets 3:45pm`, `You've hit your Sonnet limit · resets 3:45pm` [13]. "Claude Code blocks further requests until the reset time shown in the message. The session and weekly limits are shared across all models" [13]. The reset time in the text is a local clock time without a date or zone, so it is unsuitable for scheduling; `resetsAt` is the machine-readable form.
- The `result` message is not an `error_*` subtype. "A failed final request has `subtype` `"success"` and its cause in `terminal_reason`, such as `"api_error"` for an API error." [11] On that result `is_error` is true, and: "the turn ended on an error that Claude Code reported in place of a model response, such as an API error. Read `terminal_reason` for the cause, `result` for the message, and `api_error_status` for the HTTP status when the API returned one." [11] `api_error_status` is, in the Python SDK source, the "HTTP status code (e.g. 429, 500, 529) of the failing API call" when `is_error` is true and `subtype` is `success` [12]. `result` "holds the API error string if one is available but can be empty" [11].
- A subagent that hits the limit ends with "Agent terminated early due to an API error: <error detail>" returned to the parent [13].

### 4.3 Exit code

- "Claude Code exits with code 0 on success and a non-zero code when the run fails ... When a failure happens inside the run, such as missing authentication, Claude Code prints the failure as the result on stdout." [2]
- The Python SDK raises `ResultError` "when the Claude Code process exits because the run ended with an error result message, such as a turn-limit error or an API error" [11].
- With streaming input the process does not exit on an error result: "A streaming input session stays alive, and you can keep sending messages" [8].
- The numeric exit code for a plan-limit failure is not documented. Only 143 for SIGTERM is given a number [2].

### 4.4 No automatic wait in `-p`, unless the retry watchdog is set

- The interactive wait-and-continue feature is not available: "Claude Code doesn't offer the wait at all in these cases: **Background sessions and `-p` runs**: the menu row isn't available." [16]
- Plan-limit 429s are not in the default retry set. Claude Code retries "Temporary 429 throttles", and on a subscription "this includes 429 throttles that don't carry your plan's quota headers" [13]. A real limit response carries those headers (4.5).
- `CLAUDE_CODE_RETRY_WATCHDOG=1` changes this: "Retries `429` and `529` capacity errors indefinitely instead of failing after `CLAUDE_CODE_MAX_RETRIES` attempts ... The watchdog backs off up to 5 minutes between attempts, or until the limit resets when the response carries a rate-limit reset time, so a session that hits a usage limit waits out the remaining window." [3] It "fails at once when a standard-speed request gets a `429` that reports a spend limit or exhausted usage credits" [3].
- `CLAUDE_CODE_RETRY_WATCHDOG_MAX_USAGE_LIMIT_WAIT_MS` bounds that wait: "When the reset is further away than this cap, the request fails at once, unless usage credits have room or the API couldn't check them." (v2.1.301 or later) [13]

### 4.5 How this differs from an API rate limit and from a server throttle

| Case | Message text | Retried by default | Stream signal | Reset time |
|---|---|---|---|---|
| Plan usage limit (subscription) | `You've hit your session limit · resets ...` and the three variants [13] | No (4.4) | `rate_limit_event` with `status: "rejected"`; `assistant.error: "rate_limit"`; `result.is_error` | Yes, `resetsAt` |
| Server throttle, any authentication | `API Error: Server is temporarily limiting requests (not your usage limit)` [13] | Yes: "As of v2.1.199 this is retried automatically with backoff before being shown, whichever way you authenticate." [13] | `system/api_retry` per attempt | No |
| API-key rate limit | `API Error: Request rejected (429) · this may be a temporary capacity issue...`; "You have hit the rate limit configured for your API key, Amazon Bedrock project, or Google Cloud project." [13] | Yes, as a temporary 429, "up to 10 times with exponential backoff" [13] | `system/api_retry` with `error: "rate_limit"`, `error_status`, `attempt`, `max_retries`, `retry_delay_ms` [2] | No field for it |
| Overload | Repeated 529 errors; "A 529 is not your usage limit and doesn't count against your quota." [13] | Yes | `system/api_retry` with `error: "overloaded"` [6] | No |

- How the CLI tells a plan limit from a throttle: "Claude Code tells these apart from your plan limit by the absence of the unified quota headers a real limit response carries." [13]
- `system/api_retry` is emitted "When an API request fails with a retryable error ... before retrying" [2]. Its fields include no reset time.
- Plan windows do not exist for API keys: for "API keys, cloud providers, and usage-based billing: usage there is metered per request, so there is no reset to wait for." [16] The usage-report type says the same: "False when plan rate limits do not apply (API key, Bedrock, Vertex, or missing profile scope)" [10].

### 4.6 Usage credits change the outcome

- "Usage credits let you keep working past your plan's usage limit." [4] They are switched on or off by the subscriber at claude.ai under Settings > Usage [4].
- The one explicit statement about `-p` concerns models that bill to credits: "In non-interactive mode with the `-p` flag, and in an Agent SDK application that doesn't show the prompt, Claude Code never asks for consent. When a Fable request there would bill to usage credits, Claude Code bills it without asking." [17]
- For an exhausted plan window, the documentation implies the same without stating it for `-p`: a waiting request "fails at once, unless usage credits have room" [13], and prompt caching changes "once you're drawing on usage credits" [7]. The switch to credits is announced only by a notification ("Overage-transition notifications ... Toast only; these never arrive as API errors" [10]), so it is not an error in the stream.
- With credits on, the limit that eventually stops the run is the monthly spend limit: "Your plan's included usage can't cover this request, and the usage credits that would otherwise pay for it have reached a spend limit." [13]
- The event exposes the state: `isUsingOverage`, `overageStatus`, `overageEnabled` and `rateLimitType: "overage"` [10]. The type declarations carry no description of `isUsingOverage` or `overageInUse` beyond their names.
- Not documented: a sentence saying that a `-p` run on a non-Fable model moves to usage credits automatically when the plan window runs out. The statements above point that way.

### 4.7 Not documented

- The order of `rate_limit_event`, the `assistant` error message and the `result` in a `-p` stream, and whether a `rejected` event is guaranteed before the result.
- Whether `resetsAt` is in seconds or milliseconds. The type says only "Unix timestamp" [11]. The status line's separate `rate_limits.*.resets_at` fields are stated to be "Unix epoch seconds" [18], which suggests seconds but is a different interface.
- The value of `api_error_status` (presumably 429) and of `terminal_reason` (presumably `api_error`) for a plan limit specifically.
- The numeric exit code.
- Whether a `rate_limit_event` with `status: "allowed"` is emitted at the start of every session, which would give `utilization` without waiting for a warning.
- Whether a turn that hits the limit after some tool calls reports the partial work's cost in the result. The general rule (error results carry cost [7]) suggests yes.

Confidence: high that the event exists with a reset timestamp and that `-p` does not wait by default; medium on the result shape for this specific failure; low on sequence, units and exit code, which need a live test.

## 5. Hosts an egress allowlist must permit

### 5.1 What is documented

The network page gives one table, introduced by "Claude Code requires access to the following URLs. Allowlist these in your proxy configuration and firewall rules, especially in containerized or restricted network environments." [19] It is not split by authentication method. The rows, grouped by whether the factory's configuration needs them:

| Host | Documented purpose [19] | Needed by a factory worker? |
|---|---|---|
| `api.anthropic.com` | "Claude API requests, including the WebFetch domain safety check, feature flag fetches, and telemetry event logging" | **Yes.** The only host documented for model requests |
| `claude.ai` | "claude.ai account authentication" | Browser sign-in. Needed where the operator runs `claude setup-token`. For a worker holding a pre-issued token: not documented |
| `claude.com` | "claude.ai account sign-in opens a `claude.com` page in the browser, which redirects to `claude.ai`; pre-approved WebFetch documentation lookups also reach this host from the CLI" | Same as above; WebFetch is not in the worker's tool list |
| `platform.claude.com` | "Anthropic Console account authentication. OAuth token exchange, refresh, and revocation also go to this host for claude.ai accounts, so both Console and claude.ai sign-ins require it" | Sign-in on the operator's machine. For a worker holding a pre-issued token: not documented (5.2) |
| `sso.claude.com` | Browser sign-in for SSO organisations | No |
| `mcp-proxy.anthropic.com` | claude.ai MCP connectors | No. The `setup-token` token "can't ... fetch claude.ai connectors" [5]; also off with `ENABLE_CLAUDEAI_MCP_SERVERS=false` |
| `downloads.claude.ai` | "Plugin executable downloads; native installer, native auto-updater, and update version checks" | No, with updates disabled |
| `storage.googleapis.com` | Plugin metadata in `/plugin`; native updater before 2.1.116 | No |
| `registry.npmjs.org` | Plugin installs, `npx`-launched MCP servers, npm installs of the CLI | No, unless a configured MCP server is launched through `npx` |
| `bridge.claudeusercontent.com` | Claude in Chrome bridge | No |
| `*.frame.claudeusercontent.com` | Artifact reads, "only when the Artifact tool is available for your account"; off with `CLAUDE_CODE_DISABLE_ARTIFACT=1` | No |
| `github.com` | Cloning plugin marketplaces, "including the official Anthropic marketplace" | No; must stay off the list (SEC-11). Set `CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL=1` |
| `raw.githubusercontent.com` | Changelog feed; "non-interactive and cloud sessions never fetch it" | No |
| `*-review.googlesource.com` | Gerrit lookup in Desktop sessions only | No |
| `http-intake.logs.us5.datadoghq.com` | "Operational telemetry events ... Optional: disable with `DISABLE_TELEMETRY` or `DO_NOT_TRACK`" | No |
| `browser-intake-us5-datadoghq.com` | "Operational error reports ... Optional: disable with `DISABLE_ERROR_REPORTING` or `DISABLE_TELEMETRY`" | No |
| `formulae.brew.sh` | "Update version checks on Homebrew installs. Other install methods don't contact this host" | No; depends on how the worker image installs the CLI |
| `code.claude.com` | Documentation lookups by the built-in guide agent | No; "Blocking this host only affects documentation lookups" |

Supporting statements that model traffic needs only `api.anthropic.com`:

- "The SDK needs outbound HTTPS to `api.anthropic.com`, or to your provider's regional endpoint when running on Amazon Bedrock or Google Cloud's Agent Platform." [20]
- The dev container page separates the two groups: "See Network access requirements for the inference and authentication domains, and Telemetry services for the optional telemetry and error reporting connections and how to disable them." [21]
- Anthropic's reference dev container firewall script allows, of Anthropic's hosts, only `api.anthropic.com` (with `registry.npmjs.org`, GitHub's address ranges, `sentry.io`, `statsig.com` and three VS Code hosts) [22]. The documentation calls it "a working example rather than a maintained base image" [21], and it is written for interactive use where a login has already been stored, so it is weak evidence.
- Proxy support: "Claude Code respects standard proxy environment variables" (`HTTPS_PROXY`, `HTTP_PROXY`, `NO_PROXY`), and "Claude Code does not support SOCKS proxies." [19]

### 5.2 Not documented

- **Whether a session authenticated by `CLAUDE_CODE_OAUTH_TOKEN` contacts `platform.claude.com` or `claude.ai`.** The documentation says token "exchange, refresh, and revocation" use `platform.claude.com` [19], and separately that with the environment token "Claude Code uses the token you set for the whole session. To replace an expired token, generate a new one and restart" [3], which reads as "no refresh". No sentence joins the two. The token is a "one-year OAuth token" [5].
- **A complete list for non-bare `-p`.** `--bare` is documented in `--help` as skipping "background prefetches" [15]; the documentation does not enumerate what a non-bare run prefetches or from where.
- **A guarantee** that with the variables in section 6 set, the CLI contacts no host other than `api.anthropic.com`.

The documented minimum for SEC-11 is therefore `api.anthropic.com` alone. Whether that is sufficient for a clean start is a question for the contract test, which should run with the proxy in deny-and-log mode and assert on the denied-host log.

A hostname allowlist does not separate model requests from telemetry events and feature-flag fetches, because all three use `api.anthropic.com` [19]. Since the allowlist's purpose is to limit where the subscription token can be sent, this is acceptable for SEC-11, but the non-essential requests still have to be switched off by variable.

Confidence: high that `api.anthropic.com` is necessary; medium that it is sufficient.

## 6. Switching off telemetry, error reporting, auto-update and other non-essential traffic

### 6.1 The variables

| Variable or setting | Effect (quoted) | Source |
|---|---|---|
| `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` | "Set to any non-empty value, such as `1`, to disable nonessential network traffic: auto-updates, telemetry, error reporting, the `/feedback` command, Claude-drafted feedback, release notes, the PR and MR status badge checks, and availability checks such as the fast mode check." "**Setting it to `0` or `false` still disables this traffic**". "Also disables feature-flag fetching". "Official plugin marketplace auto-install isn't covered" | [3] |
| `DISABLE_TELEMETRY` | "Set to any non-empty value, such as `1`, to opt out of telemetry ... Also disables feature-flag fetching." | [3] |
| `DO_NOT_TRACK` | "Set to `1` to opt out of telemetry, with the same effect as `DISABLE_TELEMETRY`" | [3] |
| `DISABLE_ERROR_REPORTING` | "Set to any non-empty value, such as `1`, to opt out of error reporting." | [3] |
| `DISABLE_AUTOUPDATER` | "Set to `1` to disable automatic background updates. Manual `claude update` still works. Use `DISABLE_UPDATES` to block both" | [3] |
| `DISABLE_UPDATES` | "Set to `1` to block all updates including manual `claude update` and `claude install`. Stricter than `DISABLE_AUTOUPDATER`." | [3] |
| `DISABLE_FEEDBACK_COMMAND` | "Set to `1` to disable the `/feedback` command and Claude-drafted feedback. Also disables `/bug` and `/share`" | [3] |
| `CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY` | Disables session quality surveys; they "are also disabled when `DISABLE_TELEMETRY`, `DO_NOT_TRACK`, or `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` is set" | [3] |
| `CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL` | "Set to `1` to disable automatic registration of the official plugin marketplace." Read "usually during a machine's first interactive launch" | [3] |
| `ENABLE_CLAUDEAI_MCP_SERVERS=false`, or setting `disableClaudeAiConnectors` | "Set to `false` to stop Claude Code from fetching claude.ai MCP servers." | [3] |
| Setting `skipWebFetchPreflight: true` | Skips the WebFetch check, which "sends each requested hostname to `api.anthropic.com` before fetching". The check "is not affected by `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`" | [14][23] |
| `CLAUDE_CODE_DISABLE_ARTIFACT=1`, or setting `enableArtifact: false` | Turns off the Artifact tool | [3] |
| `DISABLE_GROWTHBOOK` (2.1.124+), `CLAUDE_CODE_DISABLE_FEATURE_FLAGS` (2.1.319+) | Turn off feature-flag fetching only | [3] |
| `CLAUDE_CODE_ENABLE_TELEMETRY` | OpenTelemetry export is opt-in: "Set to `1` to enable OpenTelemetry data collection". Leave unset | [3] |

What is on by default for this configuration:

- Metrics: "Default on. `DISABLE_TELEMETRY=1` to disable." for the Claude API [23]. "Metrics never include your code, prompts, or file paths." [23]
- Error reports: on when "you sign in with a Claude Pro, Max, or Ultra subscription", "you're running Claude Code v2.1.198 or later" and "you're connecting directly to the Claude API" [23]. A Max-plan factory meets all three, so error reporting is on unless disabled.
- Auto-update: "Native installations automatically update in the background." "Homebrew, WinGet, apt, dnf, and apk installations do not auto-update by default" [24].
- Prompt suggestions cost a model request in interactive mode, but "In print mode, Claude Code doesn't generate suggestions by default." [16]

Where to set them: "All environment variables can be checked into `settings.json`" [23], through the `env` key. The spec's worker environment is cleared to an allowlist (AG-11), so either the host adds these variables to that allowlist or it writes them into the `env` block of the host-written settings file (AG-4).

### 6.2 Side effects

- Turning off feature-flag fetching removes flag-gated features. The list includes "Use MCP tasks" and "Have Claude Code exclude MCP tools whose input schema the API would reject" [3]; the rest are interactive or claude.ai features.
- A fresh configuration directory has no saved flags in any case: "**A clean environment on every run**: if each run starts in a CI container, or any other environment without the flags an earlier session saved, every run is a first session" [3]. AG-4 creates a fresh directory per execution, so the factory's sessions already run without fetched flags at start. Disabling the fetch makes that deterministic for the whole session.
- With `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` set, the retry watchdog's early-reset check is off: `CLAUDE_CODE_RETRY_WATCHDOG_USAGE_LIMIT_RECHECK_MS` checks are made "not while `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` is set" [3].

### 6.3 Not documented

- Whether the `/usage` plan request and `rate_limit_event` are affected by `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`. The event is described as rate-limit information "emitted when rate limit info changes" [10], and the default retry logic reads "quota headers" on API responses [13], which suggests the event is derived from model responses and needs no extra request. That is an inference.
- Whether any request remains, with all variables set, other than model requests to `api.anthropic.com`.

Confidence: high on what each variable is documented to do; medium that the set is complete for a non-bare `-p` run.

## 7. What this means for the spec

**Section 14 and AG-14.**
- AG-14 is sound: both caps exist in print mode, and the budget cap is the same local estimate the factory reads from `total_cost_usd`. Keep the 10% headroom of BUD-1; the documented overshoot is one response plus running subagents.
- BUD-4's "per model call ... output ... and dollars" cannot be met from `assistant` events: per-call output tokens are placeholders and there is no per-call dollar figure. The options are to take per-stage totals from `result.modelUsage` and apply the BUD-3 price table to them, or to add `--include-partial-messages` and read `message_delta.usage`, which covers the main loop only.
- `UsageBatch` (ORCH-4) should not sum `total_cost_usd` across results. In a streaming-input session each result is a running total, and a resumed session's first result already includes earlier spend.
- 13.5's "No dollar bill can run away" needs a condition: usage credits must be off for the account, and `sf init` or `sf doctor` should say so. A `rate_limit_event` with `isUsingOverage: true` or `rateLimitType: "overage"` should be treated as a stop condition.

**8.3 pause.**
- The reset time the pause needs is available as `rate_limit_event.rate_limit_info.resetsAt`, with `rateLimitType` saying which window (five-hour or seven-day). The driver should map `status: "rejected"` to `LimitReached(kind)` from that event, and not parse the `You've hit your ... limit · resets 3:45pm` text.
- `allowed_warning` with `utilization` allows the scheduler to stop dispatching before running sessions are interrupted.
- `system/api_retry` events belong to the Transient class of 8.4 and must not trigger the plan-limit pause.
- The factory should not set `CLAUDE_CODE_RETRY_WATCHDOG`, since the CLI would then sit inside a request until the reset and the supervisor's stall rule would fire. If it is ever set, bound it with `CLAUDE_CODE_RETRY_WATCHDOG_MAX_USAGE_LIMIT_WAIT_MS`.
- A weekly limit can reset days out, so the pause must survive daemon restarts.
- In streaming-input mode the process does not exit on the error result, so the supervisor must act on the event and run the AG-12 stop sequence itself.

**SEC-11.**
- The model-endpoint entry for `subscription` mode is `api.anthropic.com` only, as the documented minimum. `claude.ai`, `claude.com` and `platform.claude.com` are needed on the operator's machine for `claude setup-token`, not demonstrably on workers.
- The worker environment should set `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, `DISABLE_TELEMETRY=1`, `DISABLE_ERROR_REPORTING=1`, `DISABLE_UPDATES=1`, `CLAUDE_CODE_DISABLE_OFFICIAL_MARKETPLACE_AUTOINSTALL=1`, `ENABLE_CLAUDEAI_MCP_SERVERS=false` and `CLAUDE_CODE_DISABLE_ARTIFACT=1`.
- The contract test (19.4) should run with the proxy denying everything except `api.anthropic.com` and assert that the session completes and that the denied-request log is empty.

## 8. What only a live test can settle

1. The exact stream on a plan limit: order of `rate_limit_event`, `assistant` error and `result`; values of `api_error_status` and `terminal_reason`; whether `resetsAt` is always present on `rejected`, and its unit.
2. The process exit code after a plan-limit result, in single-shot and in `--input-format stream-json` mode.
3. Whether a `rate_limit_event` is emitted at session start or only on a status change.
4. Whether a session on a `setup-token` credential starts and runs with only `api.anthropic.com` reachable, and which hosts it attempts with and without the variables of section 6.
5. Whether `/usage` under a `setup-token` credential returns plan rows or `rate_limits: null`.
6. Whether `total_cost_usd` and `modelUsage` are populated as documented under this credential, and that `--max-budget-usd` stops a run at the estimate.
7. What the stream shows when usage credits are on and the plan window is exhausted.

Items 1, 2 and 7 need an account at its limit, which is hard to arrange on purpose; a small low-priority probe that runs when the factory first meets a limit, and records the raw lines, is the practical route.

## 9. Adjacent observation, outside this ticket

The legal page states: "Advertised usage limits for Pro, Max, and Ultra plans assume ordinary, individual usage of Claude Code and the Agent SDK." [25] It also restricts third-party products from offering claude.ai login, while not preventing "an end user from signing in to the unmodified Claude Code binary with their own Claude subscription" [25]. This bears on D-2 and P-23 and was not investigated further here.

## Sources

All fetched 2026-11-18. Claude Code documentation pages were fetched as markdown (`.md` appended to the page URL).

1. https://code.claude.com/docs/en/cli-reference.md
2. https://code.claude.com/docs/en/headless.md
3. https://code.claude.com/docs/en/env-vars.md
4. https://code.claude.com/docs/en/costs.md
5. https://code.claude.com/docs/en/authentication.md
6. https://code.claude.com/docs/en/agent-sdk/typescript.md (`SDKResultMessage`, `SDKAssistantMessage`, `SDKRateLimitEvent`, `SDKUsageReport`, `ModelUsage`, `Usage`, `ApiKeySource`, `Options`)
7. https://code.claude.com/docs/en/agent-sdk/cost-tracking.md
8. https://code.claude.com/docs/en/agent-sdk/agent-loop.md
9. https://code.claude.com/docs/en/agent-sdk/streaming-output.md
10. https://unpkg.com/@anthropic-ai/claude-agent-sdk@0.3.311/sdk.d.ts (type declarations shipped in the official npm package; `SDKRateLimitEvent`, `SDKRateLimitInfo`, `SDKResultSuccess`, `SDKResultError`, `TerminalReason`; 0.3.327, the latest, has the same `SDKRateLimitInfo`)
11. https://code.claude.com/docs/en/agent-sdk/python.md (`ResultMessage`, `ResultError`, `RateLimitEvent`, `RateLimitInfo`)
12. https://github.com/anthropics/claude-agent-sdk-python/blob/cd08fa086187e7f938775621e39da7ec70995fae/src/claude_agent_sdk/types.py ; https://github.com/anthropics/claude-agent-sdk-python/blob/cd08fa086187e7f938775621e39da7ec70995fae/src/claude_agent_sdk/_internal/message_parser.py
13. https://code.claude.com/docs/en/errors.md ("Automatic retries", "Tune retry behavior", "Usage limits", "Agent terminated early due to an API error")
14. https://code.claude.com/docs/en/settings-reference.md (`modelPricing`, `skipWebFetchPreflight`, `autoContinueAtUsageLimit`)
15. `claude --help` and `claude setup-token --help`, Claude Code 2.1.311, local Homebrew install, read 2026-11-18
16. https://code.claude.com/docs/en/interactive-mode.md ("Wait for a usage limit to reset", "Prompt suggestions")
17. https://code.claude.com/docs/en/model-config.md ("Fable and usage credits")
18. https://code.claude.com/docs/en/statusline.md
19. https://code.claude.com/docs/en/network-config.md ("Network access requirements", "Proxy configuration")
20. https://code.claude.com/docs/en/agent-sdk/hosting.md
21. https://code.claude.com/docs/en/devcontainer.md
22. https://github.com/anthropics/claude-code/blob/154c06539c4309e172b8ce6f2b07388987617d68/.devcontainer/init-firewall.sh
23. https://code.claude.com/docs/en/data-usage.md ("Telemetry services", "Default behaviors by API provider", "WebFetch domain safety check")
24. https://code.claude.com/docs/en/setup.md ("Auto-updates", "Disable auto-updates")
25. https://code.claude.com/docs/en/legal-and-compliance.md

## Method notes

- Pages were downloaded with `curl` and read or searched locally, so quotations are from the page text and not from a summarising fetcher.
- The documentation describes the current release line (it refers to versions up to 2.1.324). The spec pins flag names to 2.1.311 and the local CLI is 2.1.311. Version requirements stated in the documentation for the features used here are all at or below 2.1.305, except `CLAUDE_CODE_DISABLE_FEATURE_FLAGS` (2.1.319), for which the older name `DISABLE_GROWTHBOOK` is given.
- The TypeScript reference page and the shipped type declarations disagree on the fields of `rate_limit_info`. The declarations and the Python SDK source agree with each other and were preferred.
