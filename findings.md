# Findings & Decisions

## Requirements
- Traverse `/Users/stometa/Downloads/claudecode` as a repository, not just surface files.
- Focus on the lifecycle starting when a user opens the terminal tool and sends a message.
- Explain startup flow, wake-up logic, skills/commands/hooks dispatch, sub-agent creation, agent communication, evaluation, next-step control, hallucination prevention, and auto-compact behavior.
- Use a Socratic style and proceed one question at a time.

## Research Findings
- The analysis should prioritize executable entrypoints, bootstrapping modules, and the main event loop before reading support utilities.
- `/Users/stometa/Downloads/claudecode` is a wrapper directory, not the actual single repo root for the app logic.
- There are two meaningful trees:
- `sourcecode/`: TypeScript/TSX source tree with `src/cli`, `src/services`, `src/tools`, `src/remote`, `src/compact`, and other app modules.
- `claude-code-2.1.88/`: packaged distribution that likely reflects the published CLI artifact.
- The top-level `package.json` in `/Users/stometa/Downloads/claudecode` is just a tiny wrapper with a `source-map` dependency and is not the real product manifest.
- Early evidence suggests the runtime control plane is spread across:
- CLI handlers under `sourcecode/src/cli/`
- Tool orchestration under `sourcecode/src/services/tools/`
- Compaction under `sourcecode/src/services/compact/`
- MCP/plugin surfaces under `sourcecode/src/services/mcp/` and `sourcecode/src/services/plugins/`
- The presence of `src/main.tsx` suggests the CLI UI itself is rendered via Ink/React.
- The published CLI package is definitively `claude-code-2.1.88/package.json`, where `bin.claude` points to `cli.js`.
- `sourcecode/` does not contain its own `package.json`, which suggests it is a recovered or unpacked source tree rather than a standalone workspace root.
- The packaged artifact contains a small surface at the top level: `cli.js`, `cli.js.map`, `sdk-tools.d.ts`, `README.md`, and license files.
- `sourcecode/src/main.tsx` is the readable startup spine for the interactive CLI.
- Startup has deliberate top-level side effects before normal imports complete:
- `profileCheckpoint('main_tsx_entry')`
- `startMdmRawRead()`
- `startKeychainPrefetch()`
- These run before the rest of module evaluation to hide latency under import time.
- `main.tsx` imports the main startup/control modules directly:
- `init` and `initializeTelemetryAfterTrust` from `entrypoints/init`
- `launchRepl` from `replLauncher`
- `processSessionStartHooks` and `processSetupHooks`
- `getTools`, bundled plugins/skills initialization, MCP/client services, permission setup, conversation restore, and remote/session helpers
- `run()` is the main Commander-based CLI builder, starting around line 884 in `main.tsx`.
- `main()` itself starts around line 585 in `main.tsx` and performs pre-`run()` setup, including client-type/session-source determination, early settings parsing, and then `await run()`.
- `run()` attaches a Commander `preAction` hook that acts as the shared initialization barrier for commands:
- wait for early MDM/keychain prefetch completion
- `await init()`
- attach logging sinks
- wire inline plugin dirs
- run migrations
- kick off remote managed settings and policy limits asynchronously
- This means "startup" is split between:
- top-level module side effects
- `main()` early process/session shaping
- `run().preAction` shared initialization
- the default action handler for interactive/headless session setup
- The default action handler sets `CLAUDE_CODE_SIMPLE=1` for `--bare`, resolves assistant/proactive/brief mode, creates the Ink root for interactive sessions, runs trust/onboarding UI, then only after trust:
- initializes LSP
- surfaces settings validation errors
- starts quota/bootstrap/fast-mode prefetches
- resolves MCP configs and prefetches MCP resources
- starts SessionStart hooks
- initializes plugin bookkeeping
- enters either headless print flow or interactive REPL flow
- Trust is a hard gate before several risky behaviors. Comments explicitly note that LSP/plugin execution and some credential/API-related work must wait until trust is accepted.
- `main()` performs early argv rewriting for at least three special interactive paths before Commander parsing:
- `cc://` / `cc+unix://` URLs
- `claude assistant [sessionId]`
- `claude ssh <host> [dir]`
- These are stripped/re-written so the main interactive command path handles them with the full TUI instead of a thin subcommand path.
- `main()` also sets process-wide session metadata before `run()`:
- interactive vs non-interactive
- entrypoint (`cli`, `sdk-cli`, `mcp`, etc.)
- client type (`cli`, `sdk-python`, `remote`, `claude-vscode`, `github-action`, etc.)
- session source (`remote-control` when bridge-launched)
- `eagerLoadSettings()` runs before `init()`, specifically so `--settings` and `--setting-sources` affect initialization from the very beginning.
- `runMigrations()` runs inside `preAction`, after `init()` and before later session setup.
- `entrypoints/init.ts` is a memoized shared initializer. It:
- enables config loading
- applies only safe env vars before trust
- applies extra CA certs early
- sets up graceful shutdown
- starts async analytics/logging/bootstrap tasks
- initializes remote-settings/policy-limit loading promises
- configures mTLS and proxy/global HTTP agents
- preconnects to Anthropic API
- sets shell behavior on Windows
- registers cleanup for LSP manager and session swarm teams
- ensures scratchpad directory if enabled
- `initializeTelemetryAfterTrust()` is explicitly separated from `init()`. For remote-settings-eligible users it waits for remote settings, reapplies full env vars, then initializes telemetry. This is another example of staged startup.
- `interactiveHelpers.tsx` is where trust completion transitions into "full session" mode:
- `showSetupScreens(...)` always enforces the trust dialog in interactive mode
- after trust it sets session trust accepted, reinitializes GrowthBook, prefetches system context, processes `.mcp.json` approvals, applies full env vars, and schedules telemetry init
- `renderAndRun(...)` renders the main React tree, starts deferred prefetches, waits for UI exit, then performs graceful shutdown
- `setup.ts` is another startup-phase module called before first conversation turn. It:
- sets cwd and session/project roots
- may start the UDS messaging server
- captures teammate mode snapshot
- restores terminal/iTerm backups
- captures hook configuration snapshot and initializes file-changed hook watcher
- may create/switch to a worktree and tmux session
- warms commands/plugin hooks asynchronously
- Comments in `setup.ts` show that command/hook/agent availability is intertwined with startup ordering; e.g. worktree setup must run before `getCommands()`, and plugin hooks are preloaded for `processSessionStartHooks` before render.
- The REPL component (`screens/REPL.tsx`) is itself a large runtime composition surface, not just a view. Imports and hook usage indicate it directly integrates:
- deferred hook message injection
- query execution
- merged tools/commands/clients
- plugin and skill change management
- remote/direct-connect/SSH session hooks
- swarm/team initialization and permission bridges
- session compaction and post-compact cleanup
- scheduled tasks and background sessioning
- This means startup continues "inside React" after `launchRepl(...)`; some mechanisms only become live when the REPL mounts.
- `MDM` in this repo means managed-device / mobile-device-management policy reads, not model or message data. `utils/settings/mdm/rawRead.ts` spawns `plutil` on macOS or `reg query` on Windows to read device-managed settings early and in parallel.
- `startupProfiler.ts` is primarily startup performance instrumentation, not generic runtime monitoring. It samples startup checkpoints, logs timing phases to analytics for a fraction of sessions, and can emit a full local report when `CLAUDE_CODE_PROFILE_STARTUP=1`.
- `runMigrations()` in `main.tsx` runs configuration and model migrations, not database schema migrations. The migration files rewrite settings and config state like:
- auto-updater preference moved into settings env vars
- bypass-permission acceptance moved into settings
- MCP approval fields moved from project config into settings
- model alias migrations (`sonnet`, `opus`, legacy model names)
- bridge config rename (`replBridgeEnabled` -> `remoteControlAtStartup`)
- Config state is file-backed. `config.ts` defines large `GlobalConfig` and `ProjectConfig` TypeScript objects, while `sessionStorage.ts` persists transcripts under `~/.claude/projects/.../*.jsonl`, including per-agent transcript files.
- `UDS_INBOX` refers to Unix Domain Socket based cross-session messaging. Evidence:
- startup passes/exports a `CLAUDE_CODE_MESSAGING_SOCKET` path during setup
- SDK init messages include a hidden `messaging_socket_path`
- `SendMessageTool` supports `uds:/path.sock` addresses for local cross-session messaging and `bridge:session_...` for remote-control peers
- prompt text says these cross-session messages arrive wrapped as `<cross-session-message from="...">`
- In interactive startup, `processSessionStartHooks('startup', ...)` may run asynchronously before REPL render; unresolved hook results are passed as `pendingHookMessages` so the UI renders immediately but still injects hook context before the first API call.
- After startup processing, the main path enters `launchRepl(...)` with `sessionConfig`, `initialMessages`, and optional `pendingHookMessages`.
- `main.tsx` contains a large subcommand surface (`mcp`, `server`, `ssh`, `open`, `auth`, `plugin`, `agents`, `doctor`, `update`, etc.), but in `--print` mode it intentionally skips most subcommand registration and routes directly to the default action for performance.

## Technical Decisions
| Decision | Rationale |
|----------|-----------|
| Analyze from package metadata into runtime imports | Establishes concrete startup path |
| Treat claims as provisional until backed by file/function references | Needed because repo structure can be misleading without tracing imports |
| Use `sourcecode/src/main.tsx` as the primary readable control-flow source, with `claude-code-2.1.88/cli.js` only as distribution confirmation | The packaged file is too minified for efficient architectural reasoning |
| Distinguish four startup layers: module side effects, `main()`, `run().preAction`, and action handler | This matches the actual control boundaries in the code and avoids flattening distinct initialization phases |
| Treat `entrypoints/init.ts` as infrastructure bootstrap, not full session startup | It initializes shared process services but intentionally defers trust-sensitive session work |
| Separate "pre-REPL startup" from "REPL-mounted runtime startup" | Many dynamic systems are activated only once the REPL component mounts and its hooks run |

## Issues Encountered
| Issue | Resolution |
|-------|------------|
| Session catchup surfaced context from another repo analysis | Treated as unrelated because the requested repo path is different and no local planning files existed yet |
| Assumed `sourcecode/` had its own package manifest | Corrected after direct file check; use the packaged `package.json` for actual startup entry tracing |

## Resources
- Target repo: `/Users/stometa/Downloads/claudecode`
- Source tree: `/Users/stometa/Downloads/claudecode/sourcecode`
- Packaged tree: `/Users/stometa/Downloads/claudecode/claude-code-2.1.88`
- Working notes: `/Users/stometa/dev/endgoal/task_plan.md`, `/Users/stometa/dev/endgoal/findings.md`, `/Users/stometa/dev/endgoal/progress.md`

## Visual/Browser Findings
- None yet.
