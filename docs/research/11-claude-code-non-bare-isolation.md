# 11 - Claude Code isolation without bare mode (probe P-21)

Research date: 2026-11-18. Ticket: issue #8. Feeds AG-3a (SPEC 10.3) and probe P-21 (SPEC 23.2).

Method: primary sources only. The official Claude Code documentation at code.claude.com/docs, fetched today as
Markdown (the changelog page's newest entry is 2.1.327, dated 2026-11-17), and `claude --help` of the installed
CLI, version 2.1.311. No `claude` session was started, so nothing here is observed behaviour. Every claim
carries a verbatim quote and a source number; where the documentation says nothing, the report says "silent".
Bare mode itself is covered in reports 05 and 07 and is not repeated.

The invocation in question is `claude -p` without `--bare`, with `--setting-sources user`,
`--strict-mcp-config --mcp-config <host file>`, `--settings <host file>` and a per-execution `CLAUDE_CONFIG_DIR`.

## 1. Findings

1. **The documentation answers P-21 with "yes, mostly": `--setting-sources user` is the switch.** The setting
   source named `project` covers the project's settings and hooks, its `CLAUDE.md` and rules, and its skills,
   commands and subagents. The source named `local` covers `CLAUDE.local.md` and `.claude/settings.local.json`.
   With only `user` loaded, none of them is read from the working directory or its parents [S6][S3][S5].
2. **The clearest statement of that sits on the Agent SDK page, not on a CLI page.** The CLI reference describes
   the flag only as a "list of setting sources to load" [S2]. The CLI-side confirmations are partial: one
   sentence for `CLAUDE.md` [S5], one for rules [S4], one for settings files and `.mcp.json` [S3], one for
   skills, commands and subagents in `--add-dir` directories [S3], and a changelog fix for project skills [S15].
   A contract test on the pinned CLI is still needed (section 5).
3. **`AGENTS.md` is a gap the ticket did not list, and it matters most here.** Since v2.1.277 the CLI reads
   `AGENTS.md` from the working directory when no `CLAUDE.md` is present [S4]. Onboarded repositories must have
   an `AGENTS.md` and need not have a `CLAUDE.md`. The documentation is silent on whether `--setting-sources`,
   `CLAUDE_CODE_DISABLE_CLAUDE_MDS` or safe mode stop it. The one documented off switch is a `pluginConfigs`
   entry that the host can put in its `--settings` file (section 2.3).
4. **Plugins from the repository are blocked by the trust rule, not by the flag.** A plugin folder under the
   project's `.claude/skills/` is "Not used" in a `claude -p` run whose folder was never trusted [S3]. Trust is
   recorded in `~/.claude.json`, which lives in the host-created config directory. `enabledPlugins` in project
   settings is not read once the `project` source is off.
5. **Hooks have a documented belt-and-braces switch: `--settings '{"disableAllHooks": true}'`.** It outranks
   project and local settings [S7]. It also turns off the factory's own user-level hooks, so it fits only if
   the factory defines none.
6. **There is one flag that switches everything off and keeps subscription login: `--safe-mode`** (or
   `CLAUDE_CODE_SAFE_MODE=1`), present in the installed 2.1.311. It is documented as a troubleshooting aid, it
   also turns off MCP servers and ignores `--agents`, and the documentation is silent on whether servers
   passed with `--mcp-config` survive it (section 3).
7. **Bare mode is still announced as the future default for `-p`. No opt-out is documented** (section 4).
8. **The pinned CLI version is three releases short of a relevant fix.** v2.1.314 fixed "a settings file
   excluded with `--setting-sources` still supplying some settings, such as auto mode rules and saved permission
   consents" [S15]. On 2.1.311 an excluded project or local settings file can still leak those.

## 2. Item by item

Baseline, without any of the controls: "Without it, `claude -p` discovers and loads them as an interactive
session would, including anything configured in the working directory or `~/.claude`." and "Without `--bare`, a
`-p` session runs the hooks in a project's `.claude/settings.json` and connects the servers in its `.mcp.json`,
even in a folder you've never trusted." [S1]

The rule all rows below lean on, from the Agent SDK page [S6]:

| Source | What it loads (quoted) |
|---|---|
| `project` | "Project `settings.json` and hooks; project CLAUDE.md and `.claude/rules/*.md`; project skills, commands, and subagents" |
| `user` | "User `settings.json`; user CLAUDE.md and `~/.claude/rules/*.md`; user skills, commands, and subagents" |
| `local` | "CLAUDE.local.md, `.claude/settings.local.json`" |

The same page ties the SDK option to the CLI: "When you omit `settingSources`, `query()` reads the same
filesystem settings as the Claude Code CLI: user, project, and local settings, CLAUDE.md files, and `.claude/`
skills, agents, and commands." [S6] The changelog treats the two as one control: "`--setting-sources` (and SDK
`settingSources`)" [S15].

Confidence levels used below: **high** = a CLI page says it directly; **medium** = stated on the SDK page or for
a neighbouring case only; **silent** = not documented.

### 2.1 `CLAUDE.md`

| Question | Answer | Quote | Source |
|---|---|---|---|
| Loaded from the working directory with `--setting-sources user`? | No. Confidence: high | "pass `--setting-sources user` to turn off project configuration such as `.claude/settings.json`, `CLAUDE.md`, and `.mcp.json`" | [S5] |
| Scope of what is skipped | Working directory, every parent, and subdirectories on demand | Table rows "Project (parent dirs) / `CLAUDE.md` files in directories above `cwd` / `settingSources` includes `\"project\"`, loaded at session start" and "Project (child dirs) / ... / loaded on demand" | [S6] |
| `.claude/rules/` | Skipped, on demand rules too since v2.1.211 | "Project rules are skipped if you exclude `project` from `--setting-sources`. Before v2.1.211, rules that load on demand ... loaded even when `project` was excluded." | [S4] |
| Environment variable | `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`. Also drops the user-level file and auto memory | "Set to `1` to prevent loading any CLAUDE.md memory files into context, including user, project, and auto memory files" | [S9] |
| Setting key | `claudeMdExcludes`, a glob list on absolute paths, valid in user settings and `--settings` | "The `claudeMdExcludes` setting lets you skip specific files by path or glob pattern." "You can configure `claudeMdExcludes` at any settings layer" | [S4] |
| Flag | `--safe-mode` (section 3) | "CLAUDE.md, skills, plugins, hooks, MCP servers, custom commands and agents ... do not load" | [S2] |

The user-level `CLAUDE.md` in the per-execution config directory still loads, because `user` stays on.

### 2.2 `CLAUDE.local.md`

| Question | Answer | Quote | Source |
|---|---|---|---|
| Loaded with `--setting-sources user`? | No. Confidence: medium | Table row "Local / `<cwd>/CLAUDE.local.md` and `CLAUDE.local.md` in every parent directory / `settingSources` includes `\"local\"`" | [S6] |
| CLI-side wording | Only for `--add-dir` directories | "`CLAUDE.local.md` is skipped if you exclude `local` from `--setting-sources`." | [S4] |
| Other switches | Same as 2.1: `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`, `claudeMdExcludes`, `--safe-mode` | see 2.1 | [S9][S4][S2] |

### 2.3 `AGENTS.md` (not in the ticket, but loaded from the working directory)

| Question | Answer | Quote | Source |
|---|---|---|---|
| Loaded by default? | Yes, when the tree has no `CLAUDE.md` | "At session start: every `AGENTS.md` and `.claude/AGENTS.md` in your working directory and the directories above it" | [S4] |
| Stopped by `--setting-sources user`? | Silent | The page lists what applies inside each `AGENTS.md` ("`claudeMdExcludes` patterns apply") but never mentions setting sources, bare mode, safe mode or `CLAUDE_CODE_DISABLE_CLAUDE_MDS` | [S4] |
| Documented off switch | `pluginConfigs` in user settings or `--settings`, value `claude-md` | "Add it to `pluginConfigs` under `cc-plugin-agents-md@builtin` ... Claude Code reads the entry from `~/.claude/settings.json`, a `--settings` file, or managed settings, and ignores it in project and local settings files." Value `claude-md`: "Your `CLAUDE.md` files only" | [S4] |
| Second switch | `claudeMdExcludes` with a pattern such as `**/AGENTS.md` | "`claudeMdExcludes` patterns apply" | [S4] |

Settings fragment for the host file: `{"pluginConfigs": {"cc-plugin-agents-md@builtin": {"options":
{"instructionFiles": "claude-md"}}}}`. The plugin ID was `agents-md@builtin` before v2.1.285 [S4].

### 2.4 Skills

| Question | Answer | Quote | Source |
|---|---|---|---|
| Project skills loaded with `--setting-sources user`? | No. Confidence: medium | "`<cwd>` and every parent directory up to the repository root for skills, commands, and subagents" under source `project` | [S6] |
| CLI-side evidence | `--add-dir` case, and a bug fix | "Skills, commands, and subagents from any flag-added directory load through the `project` setting source, so they don't load when you exclude that source with `--setting-sources` on the CLI"; "Fixed `--setting-sources user` not blocking dynamically discovered project skills" (2.1.69) | [S3][S15] |
| Flag | `--disable-slash-commands` | "Disable all skills and commands for this session" | [S2] |
| Permission rule | Deny the `Skill` tool, for example `--disallowedTools Skill` | "**Disable all skills** by denying the Skill tool" | [S10] |
| Bundled skills (not from the repository) | `disableBundledSkills` or `CLAUDE_CODE_DISABLE_BUNDLED_SKILLS=1` | "Skills from plugins, `.claude/skills/`, and `.claude/commands/` are unaffected." | [S9] |
| Skills synced from claude.ai | Not synced with the subscription token | "It doesn't sync in these sessions: A session ... where `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`, or an `apiKeyHelper` script supplies the credential" | [S10] |

A project skill's frontmatter hooks and `allowed-tools` are "Used" in an untrusted `-p` run [S3], so a project
skill that does load brings hooks with it. That is the reason to switch skills off twice.

### 2.5 Commands (`.claude/commands/`)

| Question | Answer | Quote | Source |
|---|---|---|---|
| Loaded with `--setting-sources user`? | No. Confidence: medium. Same rule as skills | "project skills, commands, and subagents" under source `project` | [S6] |
| Flag | `--disable-slash-commands` | "Disable all skills and commands for this session" | [S2] |
| Why it matters in `-p` | A prompt can invoke them | "User-invoked skills and custom commands work. Include `/skill-name` in the prompt string and Claude Code expands it before running." | [S1] |

### 2.6 Subagents (`.claude/agents/`)

| Question | Answer | Quote | Source |
|---|---|---|---|
| Loaded with `--setting-sources user`? | No. Confidence: medium. Same rule as skills | "project skills, commands, and subagents" under source `project`; and the `--add-dir` sentence quoted in 2.4 | [S6][S3] |
| Flag for file discovery | None besides `--safe-mode`; `--disable-slash-commands` is not documented to cover agents | "custom commands and agents ... do not load" (safe mode) | [S2] |
| Deny by name | `Agent(<name>)` in `permissions.deny` or `--disallowedTools` | "You can prevent Claude from using specific subagents by adding them to the `deny` array" | [S11] |
| Built-in subagent types | `CLAUDE_AGENT_SDK_DISABLE_BUILTIN_AGENTS=1` | "Set to `1` to disable all built-in subagent types such as Explore and Plan. Only applies in non-interactive mode (the `-p` flag)." | [S9] |
| Frontmatter hooks and inline MCP servers of a project subagent | Not used in `-p` without recorded trust | Table row "Frontmatter hooks in a project subagent ..." under column "`claude -p` or the SDK, folder never trusted": "Not used"; "`--strict-mcp-config` doesn't filter servers you pass inline via `--agents`" | [S3][S11] |

### 2.7 Plugins

| Route into the session | With `--setting-sources user` | Quote | Source |
|---|---|---|---|
| `enabledPlugins` and `extraKnownMarketplaces` in `.claude/settings.json` | Not read. Confidence: high | "Pass `--setting-sources user` ... so Claude Code reads neither the project's settings files nor its `.mcp.json`" | [S3] |
| Same, even if read | Needs a prior install on the machine | "a plugin enabled only in your project's `.claude/settings.json` doesn't load at session start until you install it on your machine" | [S12] |
| Plugin folder under the project's `.claude/skills/` (`@skills-dir`) | Not used in `-p`, by the trust rule. Whether the flag alone also stops it: silent | "Because that content comes from the repository rather than from you, it loads only after the same trust check ... Trusting a parent folder or running with `-p` isn't enough." | [S12][S3] |
| `.claude/plugins/` in the project | Never scanned | "Claude Code doesn't scan a project's `.claude/plugins/` directory." | [S12] |
| Plugins synced from claude.ai | Not with the subscription token | "Claude Code neither downloads nor loads synced plugins ... A session where `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`, or an `apiKeyHelper` script supplies the credential" | [S12] |
| Plugin MCP servers | Skipped by `--strict-mcp-config` | "Skip the MCP servers configured in places such as `.mcp.json`, `~/.claude.json`, or a plugin" | [S2] |

Switches: per plugin, `"<name>@<origin>": false` under `enabledPlugins` in the `--settings` file, which outranks
user, project and local [S12]. For all plugins, only `--bare` and `--safe-mode` are documented; there is no
"no plugins" flag or setting for a non-bare session. The trust record is
`projects["<path>"].hasTrustDialogAccepted` in `~/.claude.json` [S3]; the host must never write it.

### 2.8 Hooks

| Hook source | With `--setting-sources user` | Quote | Source |
|---|---|---|---|
| `.claude/settings.json` and `.claude/settings.local.json` | Not read. Confidence: high. Without the flag they run, trusted or not | "`-p` or SDK session: Claude Code never shows the dialog and treats the folder as trusted, so hooks committed in a repository's `.claude/settings.json` run in a folder you've never trusted" | [S7][S3] |
| Project skill frontmatter | Registered only if the project skill loads and is invoked | "Claude Code registers them when you or Claude invoke the skill, including in a `-p` run in a folder you haven't trusted." | [S7] |
| Project subagent frontmatter | Not used in `-p` | "A `-p` session doesn't count as accepting it." | [S7] |
| Plugin `hooks/hooks.json` | Only if the plugin loads (2.7) | "Plugin `hooks/hooks.json`: When plugin is enabled" | [S7] |

Switch: `"disableAllHooks": true` in the `--settings` file. "To turn hooks off for one run whatever the project's
settings say, pass `--settings '{\"disableAllHooks\": true}'`, which takes precedence over project and local
settings." [S7] Its reach: "Claude Code disables user, project, local, and plugin hooks; managed hooks, Agent SDK
hooks, and hooks from plugins force-enabled in managed `enabledPlugins` keep running" [S8]. Setting it only in
user settings is not enough without `--setting-sources user`, "because the repository's project settings take
precedence over yours and can set it back to `false`" [S3].

## 3. Switches that cover everything at once

| Switch | What the documentation says | Fit for `subscription` mode |
|---|---|---|
| `--safe-mode`, `CLAUDE_CODE_SAFE_MODE=1` (v2.1.169+) | "CLAUDE.md, skills, plugins, hooks, MCP servers, custom commands and agents, output styles, workflows ... and auto memory do not load. Authentication, model selection, built-in tools, and permissions work normally, which differs from `--bare`." [S2] | Keeps OAuth login. But "Claude Code ignores `--agents` entirely" [S14], MCP servers are off, and the docs are silent on `--mcp-config`, on `-p`, and on `AGENTS.md`. Presented as a troubleshooting flag. Needs a live test before the driver relies on it |
| `--restricted`, `CLAUDE_CODE_RESTRICTED=1` (v2.1.248+) | "loads only managed settings and `--settings`"; removes the tools that run commands "unless you name them individually in `--tools`" [S2] | Drops user settings too. Silent on `CLAUDE.md`, skills, commands, subagents and on OAuth |
| `strictPluginOnlyCustomization` | "Block skills, agents, hooks, and MCP servers from user and project sources." Scope: managed only [S8] | Needs a managed settings file on the host. Does not cover `CLAUDE.md` |
| `--bare` | Skips all of it, but "does not read `CLAUDE_CODE_OAUTH_TOKEN`" [S13] | Excluded by AG-3a |

Inputs that no setting-source list controls [S6]: managed settings; `~/.claude.json`, moved with
`CLAUDE_CONFIG_DIR`; auto memory, switched off with `autoMemoryEnabled: false` or
`CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`; and claude.ai connectors, which are "Not loaded when `CLAUDE_CODE_OAUTH_TOKEN`
holds a token from `claude setup-token`".

## 4. Bare mode as the default for `-p`

| Question | Answer | Evidence | Source |
|---|---|---|---|
| Still announced? | Yes | "`--bare` is the recommended mode for scripted and SDK calls, and will become the default for `-p` in a future release." | [S1] |
| Has it happened? | No, as of the changelog's newest entry, 2.1.327 (2026-11-17) | The same page still says "Without it, `claude -p` discovers and loads them as an interactive session would"; no changelog entry changes the default | [S1][S15] |
| Opt-out documented? | No. Silent | No `--no-bare` or similar flag in the CLI reference or in `claude --help` 2.1.311. `CLAUDE_CODE_SIMPLE` is documented only as "Set to `1` ... Equivalent to passing `--bare`"; no value is documented as forcing non-bare | [S2][S9][S16] |

Consequence: if the default flips, `subscription` mode has no documented way to stay non-bare, and bare mode does
not read the subscription token. The version check of AG-5 and the contract test of 19.4 are the only guard.

## 5. What this means for AG-3a

The documentation supports rebuilding the isolation from flags instead of deleting files from the reviewer's
export. A driver configuration that every source above backs:

- `--setting-sources user` and `--strict-mcp-config --mcp-config <host file>`, as today.
- `--disable-slash-commands`, unless the factory wants its own user-level skills.
- In the host `--settings` file: `disableAllHooks: true` (if the factory defines no hooks of its own), the
  `pluginConfigs` entry of 2.3 for `AGENTS.md`, `autoMemoryEnabled: false`, and optionally `claudeMdExcludes`.
- Never write a trust record for the workspace into the per-execution `~/.claude.json`.
- Raise the minimum CLI version for `subscription` mode to 2.1.314, or accept the leak named in finding 8.

Three facts argue for keeping the file removal as a second layer until the contract test passes. First, the
strongest statement is on an SDK page. Second, `AGENTS.md` is undocumented under `--setting-sources`. Third, in a
linked git worktree with no `.claude/skills` at its root, "Claude Code loads the main checkout's project skills
in the worktree session", and "The same read-through covers `.claude/agents` and `.claude/commands`" [S17]. So
deleting `.claude/` from an export that is a linked worktree can cause loading from the main checkout. The docs
are silent on whether `--setting-sources user` stops that read-through. A plain directory export avoids it.

The contract test can assert the lock-down from the `system/init` message, which lists `tools`, `mcp_servers`,
`slash_commands`, `skills`, `agents` and `plugins` [S18], with a planted fixture of each item in the working
directory.

## 6. Left unanswered by the documentation (live test needed)

1. Whether `AGENTS.md` in the working directory is read under `--setting-sources user`.
2. Whether the pinned CLI in fact omits project `CLAUDE.md`, skills, commands and subagents under
   `--setting-sources user`; the CLI pages state this directly only for `CLAUDE.md`, rules and settings files.
3. Whether a `@skills-dir` plugin folder is skipped by `--setting-sources user` alone, or only by missing trust.
4. Whether `--safe-mode` works with `-p` and still honours `--mcp-config`, `--settings`,
   `--append-system-prompt-file` and `--json-schema`.
5. Whether the worktree read-through of `.claude/skills`, `.claude/agents` and `.claude/commands` respects
   `--setting-sources user`.
6. What exactly 2.1.311 still reads from an excluded settings file ("auto mode rules and saved permission
   consents" is the changelog's only description).
7. Whether any value of `CLAUDE_CODE_SIMPLE`, or any flag, will keep `-p` non-bare once bare is the default.

## Sources

- [S1] https://code.claude.com/docs/en/headless (section "Start faster with bare mode"; note under "Create a commit")
- [S2] https://code.claude.com/docs/en/cli-reference (rows `--setting-sources`, `--strict-mcp-config`, `--disable-slash-commands`, `--safe-mode`, `--restricted`, `--bare`)
- [S3] https://code.claude.com/docs/en/permissions (sections "What runs before you trust a folder", "Additional directories grant file access, not configuration")
- [S4] https://code.claude.com/docs/en/memory (sections "How CLAUDE.md files load", "Organize rules with .claude/rules/", "Exclude specific CLAUDE.md files", "AGENTS.md", "Choose which instruction files load")
- [S5] https://code.claude.com/docs/en/debug-your-config (section "Test against a clean configuration")
- [S6] https://code.claude.com/docs/en/agent-sdk/claude-code-features (sections "Control filesystem settings with settingSources", "What settingSources does not control", "CLAUDE.md load locations")
- [S7] https://code.claude.com/docs/en/hooks (sections "Hook locations", "Hooks in skills and agents", "Disable or remove hooks", "Workspace trust")
- [S8] https://code.claude.com/docs/en/settings-reference (entries `disableAllHooks`, `strictPluginOnlyCustomization`, `enabledPlugins`, `claudeMdExcludes`)
- [S9] https://code.claude.com/docs/en/env-vars (`CLAUDE_CODE_DISABLE_CLAUDE_MDS`, `CLAUDE_CODE_SAFE_MODE`, `CLAUDE_CODE_SIMPLE`, `CLAUDE_CODE_RESTRICTED`, `CLAUDE_CODE_DISABLE_BUNDLED_SKILLS`, `CLAUDE_AGENT_SDK_DISABLE_BUILTIN_AGENTS`, `CLAUDE_CODE_DISABLE_AUTO_MEMORY`)
- [S10] https://code.claude.com/docs/en/skills (sections "Choose where skills load", "Load skills from a directory outside the project", "Skills synced from claude.ai", "Restrict Claude's skill access")
- [S11] https://code.claude.com/docs/en/sub-agents (sections "Choose the subagent scope", "Disable specific subagents", "Trust required for inline MCP servers", "Hooks in subagent frontmatter")
- [S12] https://code.claude.com/docs/en/plugins/loading (sections "Plugins shared through a repository", "Find where a plugin is enabled", "Sign-in requirements for terminal sync")
- [S13] https://code.claude.com/docs/en/authentication (section "Generate a long-lived token")
- [S14] https://code.claude.com/docs/en/errors (section "Invalid --agents configuration")
- [S15] https://code.claude.com/docs/en/changelog (entries 2.1.314, 2.1.281, 2.1.169, 2.1.69; newest entry 2.1.327)
- [S16] `claude --help`, Claude Code 2.1.311, run locally on 2026-11-18 (`--safe-mode`, `--restricted`, `--disable-slash-commands`, `--setting-sources`, `--strict-mcp-config` present; no flag that negates `--bare`)
- [S17] https://code.claude.com/docs/en/worktrees (section "What worktrees share with the main checkout")
- [S18] https://code.claude.com/docs/en/agent-sdk/typescript (type `SDKSystemMessage`)
