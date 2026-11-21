# 15 - The size of a session

Measured 2026-11-21. Evidence for issue #19: the values of the session limits that M1 enforces (SPEC section 14, D-22). The decision is in SPEC 23.1; this report is the measurement behind its three numbers.

Method: one script, run once on the operator's Mac, read the transcripts that Claude Code keeps of its own sessions on this repository, from 2026-11-18 to 2026-11-21. It changed nothing and sent nothing anywhere; the one outside request was a fetch of the public pricing page. The script was throwaway code and is not in the repository. This report holds aggregate figures only and nothing of what a session said.

These were interactive sessions: the operator typed prompts and answered questions, and the work was this repository's specification, its probes and its research. No session was a headless factory session, which does not exist yet. A statement marked **(inference)** is a reading of the figures, not something they show.

## 1. Answer

A session that began with the command to implement one ticket, counted with every prompt that followed it, is the nearest thing here to one headless `implement` session. There are 15 of them.

| Per ticket (n = 15) | Median | p90 | p95 | Max |
|---|---:|---:|---:|---:|
| Model calls, subagents included | 83 | 118 | 124 | 137 |
| Model calls, main loop only | 61 | 100 | 105 | 107 |
| Active minutes, gaps over 5 minutes dropped | 27.1 | 49.7 | 52.3 | 56.5 |
| Active minutes, gaps over 15 minutes dropped | 29.4 | 55.8 | 57.8 | 61.0 |
| Dollars at list price, from the transcripts | 14.50 | 21.29 | 24.83 | 29.48 |
| Dollars, raised 6% to Claude Code's own count (section 4) | 15.37 | 22.57 | 26.32 | 31.25 |
| Dollars at the prices of `claude-opus-5-5`, raised 6% (section 4) | 9.17 | 13.25 | 15.69 | 18.72 |

94% of the model calls were made on `claude-fable-5-2`, the model the operator specifies on. The operator runs implementation on `claude-opus-5-5`, so the last row is the one a spend limit is read against: the same tokens at that model's prices.

Against the defaults the spec listed before D-22, each with the 10% headroom of BUD-1:

| Limit | Listed default | Stops at | Tickets of the 15 it would have stopped |
|---|---:|---:|---|
| Turns | 150 | 135 | 1, if a subagent's calls count; none, if only the main loop counts |
| Wall clock | 60 min | 54 min | 1 under the 5-minute rule, 3 under the 15-minute rule |
| `implement` spend | $15 | $13.50 | 2, at the prices of `claude-opus-5-5`; 9, at the prices the sessions ran at |

## 2. What was read

- 23 finished sessions, holding 132 prompts typed by the operator and 1,772 model calls. The session that made this measurement was left out.
- A session is one transcript file. A new file starts when the context is cleared, so a session is also one context.
- A subagent's calls are in files of their own beside the session's. They are counted with the prompt that was current when the subagent started.
- One model call is one message id. A call appears on several lines, one per content block, all with the same usage; the last line is the one counted.
- A prompt is a line the operator typed, pasted or accepted, or a skill's slash command. Local commands, shell input, task notifications, interrupt markers and messages from other sessions are not prompts.
- A tool call is one tool-use block.
- Active time is the sum of the gaps between consecutive events, with a gap over 5 minutes dropped. The 15-minute rule is given beside it: it puts back long model calls and long tool calls, 9 and 5 of which ran over 5 minutes.

## 3. Sizes

### 3.1 One prompt

The work between one prompt and the next. The 105 prompts that led to fewer than 10 model calls are questions and short answers; the table is the other 27.

| One prompt, at least 10 model calls (n = 27) | Median | p75 | p90 | p95 | Max |
|---|---:|---:|---:|---:|---:|
| Model calls, subagents included | 27 | 54 | 108 | 161 | 318 |
| Model calls, main loop only | 16 | 28 | 46 | 68 | 88 |
| Tool calls | 37 | 67 | 134 | 203 | 771 |
| Active minutes (5-minute rule) | 11.2 | 23.0 | 39.2 | 46.3 | 81.4 |
| Output tokens | 51,023 | 111,627 | 196,855 | 234,869 | 1,012,504 |
| Input-side tokens: uncached, cache read and cache write | 3,127,011 | 7,215,703 | 17,386,049 | 26,006,791 | 37,050,638 |
| Dollars at list price | 4.68 | 12.07 | 19.68 | 21.17 | 82.42 |

The maximum in every row but the main loop's is one prompt that started a multi-agent workflow: 318 model calls, 97% of them by subagents, in 81 active minutes. No prompt's main loop made more than 88 calls.

### 3.2 The prompt that starts a ticket

The first prompt of each of the 15 sessions of section 1, alone. Five of the 15 stopped within 9 model calls, at a question; the table is the other 10.

| First prompt of a ticket, at least 10 model calls (n = 10) | Median | p75 | p90 | Max |
|---|---:|---:|---:|---:|
| Model calls, subagents included | 36 | 65 | 103 | 118 |
| Model calls, main loop only | 33 | 46 | 79 | 88 |
| Active minutes (5-minute rule) | 19.0 | 35.1 | 43.6 | 47.7 |
| Output tokens | 75,138 | 116,688 | 187,071 | 216,425 |
| Dollars at list price | 6.17 | 13.38 | 16.03 | 21.68 |

### 3.3 The whole ticket

The table of section 1, with the rows it leaves out. A ticket took a median of 5 prompts, and 16 at most.

| Per ticket (n = 15) | Median | p75 | p90 | Max |
|---|---:|---:|---:|---:|
| Tool calls | 108 | 122 | 137 | 157 |
| Output tokens | 131,112 | 170,409 | 206,179 | 229,568 |
| Input-side tokens | 11,781,958 | 16,477,056 | 20,757,212 | 30,230,052 |
| Subagent share of the model calls of the first prompt | 0% | 17% | 27% | 30% |

Subagents made 19% of the dollars of the 15 first prompts taken together.

### 3.4 How long a call takes

| | Median | p95 | p99 | Max |
|---|---:|---:|---:|---:|
| One model call, from the request to the end of its answer (n = 1,789) | 8 s | 71 s | 202 s | 552 s |
| One tool call, from the call to its result (n = 2,494) | 1.4 s | 24 s | 99 s | 2,489 s |

These two rows are over every session, the one that made the measurement included, which is why the first counts more calls than section 2.

Two prompts hold gaps of 21 to 61 minutes after a request that got no answer, about 9 hours in all. They are not in the table above, which times only the calls that were answered. The machine was asleep or the request hung **(inference)**. Both gap rules drop them.

## 4. Dollars

Prices in dollars per million tokens, from `https://platform.claude.com/docs/en/about-claude/pricing.md` as fetched on 2026-11-21:

| Model | Input | Cache write, 5 minutes | Cache write, 1 hour | Cache read | Output |
|---|---:|---:|---:|---:|---:|
| `claude-fable-5-2` | 8.00 | 10.00 | 16.00 | 0.20 | 40.00 |
| `claude-opus-5-5` | 4.00 | 5.00 | 8.00 | 0.20 | 20.00 |
| `claude-haiku-5-5`, prompt up to 100,000 tokens | 0.10 | 0.125 | 0.20 | 0.01 | 0.50 |
| `claude-haiku-5-5`, longer prompt | 0.50 | 0.625 | 1.00 | 0.05 | 2.50 |

- **Share of the calls.** `claude-fable-5-2` 94.0%, `claude-opus-5-5` 5.9%, `claude-haiku-5-5` 0.2%.
- **Where the dollars go.** Of the `claude-fable-5-2` dollars, 40% is output, 44% is cache writes (rather more than half of it at the 1-hour price), 16% is cache reads, and uncached input is nothing to speak of.
- **Against Claude Code's own count.** A session's transcript ends with the cost Claude Code counted for it. Over the 23 sessions the dollars computed here are 94.6% of that count, between 90% and 104% for a single session. The prices are the same: one session's `claude-fable-5-2` line comes out to the cent. The difference is model calls that the transcript does not list as messages, which belong to the harness and not to a prompt **(inference)**. The cap of `--max-budget-usd` is checked against Claude Code's count (probe P-22), so a limit is read against the dollars here raised by 6%.
- **At the prices of `claude-opus-5-5`.** The sessions measured are dearer than the sessions to be limited, since the operator specifies on `claude-fable-5-2` and runs implementation on `claude-opus-5-5`. Each ticket was priced again with its `claude-fable-5-2` tokens at the prices of `claude-opus-5-5`: output, uncached input and cache writes halve, and cache reads cost the same. The 13 tickets that ran on `claude-fable-5-2` then cost 51% to 61% of what they did, and the two that did not keep their dollars; the 15 together cost 59%. Raised 6%, in ascending order: 0.13, 0.16, 0.47, 2.16, 2.69, 7.19, 8.88, 9.17, 9.39, 9.90, 10.62, 10.89, 11.53, 14.39 and 18.72. This assumes that the other model would use the same tokens for the same ticket **(inference)**.

## 5. What this does not show

- **A headless session.** A factory session gets one prompt and no answers from a human. Here the operator answered questions, approved steps and added requests, and some follow-up prompts asked for new work, so the per-ticket figures lean high for one ticket. On the other side, active time counts short waits for the operator, and a headless session has none.
- **Code.** The repository held a specification and empty crates. A ticket that builds and tests Rust code makes longer tool calls, and may make more of them.
- **The tail.** With 15, 27 or 10 observations a 90th or 95th percentile rests on one to three of them.
- **What a turn is.** A turn is taken here to be one model call. Whether the CLI's own `--max-turns` counts a subagent's calls was not observed; no session of probe P-22 used a subagent. It is now probe P-26.
- **Caching.** 44% of the dollars are cache writes. A headless session without idle gaps, or under other cache lifetimes, would spend them differently.
- **The plan.** Nothing here says what share of the plan's window a ticket uses. The transcripts do not hold it, and the stream gives it only as the account's utilisation (probe P-22).
- **Other operators, other weeks.** Four days, one operator, one model for 94% of the calls.

## 6. What the values of D-22 rest on

| Value | Stops at (BUD-1) | From this report |
|---|---:|---|
| 200 turns | 180 | Every ticket measured is below it by either count: 137 calls with subagents, 107 without |
| 90 minutes | 81 min | The longest ticket took 56.5 or 61.0 active minutes, by the gap rule; SPEC 8.2 lets one tool call alone run 30 minutes |
| $20 for an `implement` session | $18 | At the prices of `claude-opus-5-5`: above the 95th percentile ($15.69) and below the one largest ticket ($18.72); the 14 other tickets cost $14.39 or less |

Two facts about the caps come from elsewhere. With stream-JSON input the CLI starts its `--max-turns` count over with each message, while its budget total runs on over the session: that is the documentation's statement, in [12-claude-code-subscription-behaviour](https://github.com/sapientpants/software-factory/blob/616f64afcb57c3654ebea670f48534fdb4a26c3e/docs/research/12-claude-code-subscription-behaviour.md) (section 2), and it was not observed. Both caps end a session under the subscription token, and the budget cap overshoots by one response: that was observed (probe P-22).

The values keep their *(tunable)* mark. The first measurement of what they limit is the five issues of M1b.

In the spec, the values are in the table of section 14, the rules they belong to are BUD-6 to BUD-9, the decision is D-22 in 23.1, and Appendix B lists this report. What the stream of a headless session shows of turns, subagents and usage credits is probe P-26.
