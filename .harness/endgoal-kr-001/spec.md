---
task_id: endgoal-kr-001
title: EndGoal — Full Single-KR Prototype
version: 4
status: approved
branch: feature/endgoal-kr-prototype
created: 2026-04-12T00:00:00+00:00
updated: 2026-04-12T01:00:00+00:00
---

## Goal

Build the first end-to-end instrumented Key Result prototype for the EndGoal product — a Goal-Managed Agent OKR Dashboard. A single Node (KR) must survive the full lifecycle: prose acceptance → freeze co-authoring → structured acceptance → Run dispatch (local CLI agent) → live streaming output → In-Review gate → human approve → Complete, with full audit trail replayability.

This is the "decisive proof" called for in architecture-foundations.md §17.2: one KR that survives clarification, approval, governed launch, live streaming, verification, and acceptance without state ambiguity.

**Reference codebases (read-only, for pattern reference):**
- `~/dev/multica/features/realtime/provider.tsx` — WebSocket provider pattern (invalidation signal + refetch)
- `~/dev/multica/features/issues/components/issue-detail.tsx` — panel component with timeline, resizable panels
- `~/dev/multica/apps/web/features/` — feature folder structure to follow
- `~/dev/multica/server/internal/handler/issue.go` — REST handler patterns
- `~/dev/endgoal/docs/architecture-foundations.md` — locked architectural decisions (17 sections)
- `~/dev/endgoal/docs/wireframes.html` — 9 wireframe views (UI reference, views v1f/v1e/v2/v3/v6)

---

## Success Criteria

1. **Node creation**: A Node can be created with `intent` (string) and prose `acceptance`. Phase starts as Draft.
2. **Archetype B gate**: Attempting to dispatch a `research_iteration` Run to a Draft/prose Node blocks with a modal — three choices: Freeze now / Proceed as exploration / Cancel.
3. **Freeze co-authoring**: "Freeze now" opens a conversational session. Agent proposes one item at a time (assertion → metric → rubric). Each proposal is an SSE event with shape `{ layer, item_json, reasoning, source_quote }`. User responds; backend generates next proposal from full context. Session state persists in DB. Commit → Node structured acceptance written, phase → Active.
4. **Run dispatch**: From an Active Node (structured acceptance), dispatching a Run sends `RunInput` to daemon via WebSocket. Daemon spawns local CLI (`claude` or `codex`) in an isolated `scratchpads/run-{id}/` directory.
5. **Live streaming**: Layer 2 Run Detail shows real-time stdout from the spawned process. WebSocket invalidation triggers frontend refetch.
6. **Run completion**: Subprocess exit → Run transitions to `completed` / `budget_exhausted` / `failed`. State Layer recomputes Node progress.
7. **State Layer**: `state_at(node_id)` returns `{ state, progress, confidence, next_step, effective_policy }`. Progress formula: `(assertions_pass_rate × 0.4 + metric_achievement × 0.4 + rubric_score_normalized × 0.2) × 100`. Parent context (Option C) assembled correctly.
8. **In-Review gate**: Node In-Review → no new Run dispatch allowed (422). Human sees Approve / Reject buttons.
9. **Human approval**: Approve → Complete. Reject → Active.
10. **Audit trail**: Frozen `run_input_snapshot` + `run_events` table sufficient to replay/re-review any Run.
11. **WebSocket sync**: UI updates automatically (within 2 seconds) on Run/Node state changes via WS invalidation + refetch. No manual reload required.
12. **Type safety**: ts-rs generates all `.d.ts` from Rust structs. No hand-maintained TypeScript–Rust bridges. `pnpm typecheck` and `cargo test --features generate-bindings && git diff --exit-code frontend/bindings/` both pass in CI.

---

## Technical Approach

### Monorepo structure

```
endgoal/
├── Cargo.toml              # Rust workspace root
├── crates/
│   ├── shared/             # Core types: Node, Run, Phase, Acceptance, Policy, NodeState, WS messages
│   ├── backend/            # axum HTTP/WebSocket server + State Layer
│   └── daemon/             # Local subprocess manager (RuntimeAdapter)
├── frontend/               # Next.js App Router
│   ├── app/
│   ├── features/           # nodes · panel · runs · freeze · realtime
│   ├── lib/api.ts          # Typed fetch wrapper (single source, used everywhere)
│   └── bindings/           # ts-rs auto-generated .d.ts (never hand-written)
└── db/
    └── migrations/         # SQL migration files (SQLite-compatible)
```

### Backend (crates/backend)

- **Framework**: `axum` 0.8 + `tokio` async runtime
- **DB**: `sqlx` 0.8 with SQLite (compile-time query checking via `sqlx::query!`)
- **Migrations**: `sqlx-cli` applied at startup via `sqlx::migrate!()`
- **WebSocket hub**: two namespaces — `ws/daemon` (single daemon connection) and `ws/frontend` (N browser connections). Hub state: `Arc<RwLock<Hub>>`. Daemon disconnect marks in-flight Runs as `failed`.
- **State Layer**: pure function `state_at(pool, node_id, rollup_depth) → NodeState`; reads DB only, never writes, result not stored (except `next_step` cached in `nodes.next_step_cache` column; regenerated when `canonical_updated_by_run_id != next_step_cache_for_run_id`; after regeneration write `next_step_cache_for_run_id = canonical_updated_by_run_id`)
- **LLM calls** (freeze session + next_step cache): direct HTTP to Claude API via `reqwest`; streaming SSE; no daemon involved
- **Auth**: MVP uses no auth (single-user, local). Daemon WS upgrade requires `Authorization: Bearer $ENDGOAL_DAEMON_TOKEN`

### Daemon (crates/daemon)

```rust
trait RuntimeAdapter: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(
        &self,
        input: &RunInput,
        scratchpad: &Path,
    ) -> impl Stream<Item = RunEvent> + Send;
}
```

- **Implementations**: `ClaudeCodeAdapter` → `spawn("claude", ["--workspace", scratchpad])`, `CodexAdapter` → `spawn("codex", [...])`, `EchoAdapter` → `spawn("echo", [...])` (test only)
- **Subprocess**: `tokio::process::Command`, stdout/stderr captured as async line stream
- **Scratchpad root**: configurable via `ENDGOAL_SCRATCHPAD_ROOT` env var (default: `./scratchpads/`)
- **WS client**: connects to `ws://localhost:{ENDGOAL_PORT}/ws/daemon`; auth header; receives `RunDispatch`; sends `RunEvent` + `RunTerminal`

### Frontend (Next.js App Router)

- **API client**: `lib/api.ts` — single typed fetch wrapper. All feature code imports from here; no ad-hoc `fetch()` calls. Response types come from `bindings/*.d.ts`.
- **Realtime**: `features/realtime/provider.tsx` — WS connection to `ws/frontend`; "WS as invalidation signal + refetch" (Multica pattern). WS carries only `{ type: string, id: string }`; all data fetched via REST.
- **Feature folders**: `features/nodes/`, `features/panel/`, `features/runs/`, `features/freeze/`, `features/realtime/`

### DB schema (key tables)

```sql
nodes (
  id TEXT PRIMARY KEY,
  intent TEXT NOT NULL,
  parent_id TEXT REFERENCES nodes(id),
  phase TEXT NOT NULL DEFAULT 'draft',
  acceptance_json TEXT NOT NULL DEFAULT '{"type":"prose","text":""}',
  local_policy_json TEXT,
  canonical_artifact_text TEXT,
  canonical_updated_by_run_id TEXT,
  next_step_cache TEXT,               -- cached, regenerated when canonical changes
  next_step_cache_for_run_id TEXT,    -- run_id for which next_step_cache was generated
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
)
-- Required index for CP12 SSE polling performance:
-- CREATE INDEX idx_run_events_run_seq ON run_events(run_id, seq);

node_docs (id, node_id, content, created_at)
review_log (id, node_id, actor, action, details_json, created_at)

runs (
  id TEXT PRIMARY KEY,
  node_id TEXT NOT NULL REFERENCES nodes(id),
  type TEXT NOT NULL,             -- research_iteration | exploration | synthesis | audit | reconcile
  status TEXT NOT NULL,           -- dispatched | running | completed | budget_exhausted | failed
  runtime TEXT NOT NULL,
  input_snapshot_json TEXT,       -- frozen at dispatch
  output_json TEXT,
  scratchpad_path TEXT,
  started_at TEXT,
  ended_at TEXT,
  created_at TEXT NOT NULL
)

run_events (id, run_id, seq INTEGER, event_type TEXT, data_text TEXT, created_at TEXT)

freeze_sessions (
  id TEXT PRIMARY KEY,
  node_id TEXT NOT NULL REFERENCES nodes(id),
  approved_items_json TEXT NOT NULL DEFAULT '[]',
  current_layer TEXT NOT NULL DEFAULT 'assertions',  -- assertions | metrics | rubric | committed
  status TEXT NOT NULL DEFAULT 'active',             -- active | committed | abandoned
  created_at TEXT,
  updated_at TEXT
)
```

### Freeze session SSE event schema

Every `POST /api/nodes/:id/freeze/respond` response is an SSE stream. Each event:
```json
{ "layer": "assertion" | "metric" | "rubric" | "done",
  "item_json": "{ ... layer-specific proposal object ... }",
  "reasoning": "string explaining why this item was proposed",
  "source_quote": "string quoted from Node docs or parent context that motivated it" }
```
`layer: "done"` signals that all items for the current layer are complete; frontend advances to next layer or shows "Commit" button.

### State Layer formula

```
progress = (
  assertions_pass_rate  × 0.40 +   -- fraction of assertions with status=pass
  metric_achievement    × 0.40 +   -- avg(min(current/target, 1.0)) across metrics
  rubric_normalized     × 0.20     -- avg(score / scale) across rubric dimensions
) × 100
```

`confidence` = `avg(rubric_scores) / max_scale` across rubric dimensions (0-1).
`next_step` = cached string; regenerated only when `canonical_artifact_text` changes.

### Parent context (Option C)

```rust
struct AncestorSummary {
    id:                 NodeId,
    intent:             String,
    phase:              Phase,
    acceptance_summary: String,         // first 300 chars of acceptance text/json
    canonical_summary:  Option<String>, // first 500 chars of canonical_artifact_text
    progress:           u8,
}
```

Full ancestor chain (root → immediate parent) included in `RunInput.parent_context`. If ancestor has no canonical artifact, `canonical_summary = None` (acceptable for MVP).

### Run output_json schema (MVP minimal)

When a Run completes, the daemon (or a post-processing step) writes structured `output_json` to the Run row. For the MVP, the schema is:

```json
{
  "findings": "string summary",
  "concerns": [],
  "confidence": 0.0,
  "needs_human_review": false,
  "assertion_results": { "<assertion_id>": "pass" | "fail" | "pending" },
  "metric_values": { "<metric_id>": <number> },
  "rubric_scores": { "<rubric_id>": <number> }
}
```

For the MVP echo runtime used in smoke tests, `output_json` is populated manually via `PATCH /api/runs/:id/output` after the Run completes. Real runtimes (claude, codex) would parse their output to produce this structure (post-prototype scope). `state_at()` reads `runs.output_json` for the latest completed Run to compute progress/confidence. If `output_json` is null, `state_at()` returns `progress: 0, confidence: 0`.

### Two execution paths

| Concern | Mechanism |
|---|---|
| Run execution (research_iteration etc.) | Backend REST → DB → WS → Daemon → subprocess (claude/codex CLI) |
| Freeze session proposals | Backend REST ← → Claude API (reqwest SSE streaming) |
| State Layer next_step | Backend → Claude API (one-shot, cached in DB) |

---

## Checkpoints

### Checkpoint 01: Rust workspace scaffold + DB schema + type bindings
- Scope: Initialize Cargo workspace with `shared`, `backend`, `daemon` crates. Define all core Rust types in `shared`: `Node`, `Run`, `Phase`, `Acceptance` (enum: Prose/Structured), `StructuredAcceptance` (assertions: Vec, metrics: Vec, rubric: Vec — all three Vec fields are optional/may be empty per foundations §7; `done_when: DoneWhen` deferred to Out of Scope for MVP — default: progress computed from whatever layers are present), `Policy`, `NodeState`, `RunInput`, `RunOutput`, `RunEvent`, `RunTerminal`, `RunDispatch`, `WsFrontendMessage`, `WsDaemonMessage`, `AncestorSummary`, `FreezeSession`, `FreezeProposal`. Every type used in API responses or WS messages carries `#[derive(TS, Serialize, Deserialize)]`. Write SQLite migrations for all tables. Configure `ts-rs` with `TS_RS_EXPORT_DIR=frontend/bindings`. Add `cargo test --features generate-bindings` that runs ts-rs export and writes `.d.ts` files. Initialize Next.js project in `frontend/` with TypeScript, App Router, Tailwind.
- Acceptance criteria: `cargo build --workspace` succeeds with zero errors. `cargo test --workspace` passes. `sqlx migrate run` applies all migrations to fresh SQLite DB with zero errors. `cargo test --features generate-bindings` writes `frontend/bindings/` containing valid `.d.ts` for Node, Run, NodeState, RunInput, WsFrontendMessage, WsDaemonMessage, FreezeProposal (spot-check: grep for these type names in generated files). `git diff --exit-code frontend/bindings/` passes after generation (bindings committed). `cd frontend && pnpm install && pnpm typecheck` passes (zero type errors, even if app/page.tsx is a placeholder).
- Depends on: none
- Type: infrastructure

### Checkpoint 02: Node CRUD API + phase lifecycle enforcement
- Scope: Implement axum REST handlers: `POST /api/nodes`, `GET /api/nodes` (top-level only), `GET /api/nodes/:id`, `GET /api/nodes/:id/children`, `GET /api/nodes/:id/ancestors`, `PATCH /api/nodes/:id` (accepts `intent` and `local_policy` only; rejects `phase` field with 400; `local_policy` updates must be monotonic — tightening only, verified against existing policy), `DELETE /api/nodes/:id` (sets phase=archived). Also: `POST /api/nodes/:id/review` (Active → In-Review, returns 400 if not Active). Implement `effective_policy(node_id)` as a single recursive CTE DB query (not N individual queries). Enforce phase invariants: Draft→Active blocked if acceptance is prose (separate endpoint `POST /api/nodes/:id/freeze/commit` handles that); Active→Draft is 400; Complete→any is 400; In-Review→Active via reject endpoint only.
- Acceptance criteria: `POST /api/nodes` creates Node, returns JSON with all fields matching ts-rs bindings. `PATCH /api/nodes/:id` with `{ "phase": "draft" }` returns 400. `PATCH /api/nodes/:id` with `{ "intent": "new intent" }` returns 200. `GET /api/nodes/:id/ancestors` for a depth-3 Node returns array of 2 ancestors in order [root, parent]. `effective_policy` for a child with `{ max_tokens: 50000 }` whose parent has `{ max_tokens: 100000 }` returns `{ max_tokens: 50000 }` (child tightens). `POST /api/nodes/:id/review` on Active Node returns 200 and transitions to In-Review; on non-Active Node returns 400. `cargo test -p backend -- nodes` passes all tests.
- Depends on: 01
- Type: backend

### Checkpoint 03: Daemon + RuntimeAdapter + subprocess spawn
- Scope: Implement daemon binary (`crates/daemon`). Define `RuntimeAdapter` trait. Implement `ClaudeCodeAdapter` (spawns `claude --workspace <path>`), `CodexAdapter` (spawns `codex --workspace <path>`), `EchoAdapter` (spawns `echo <message>` — for testing). Implement scratchpad creation: `mkdir -p $ENDGOAL_SCRATCHPAD_ROOT/run-{id}/` before spawn. Implement subprocess execution: `tokio::process::Command`, capture stdout line-by-line as `RunEvent { run_id, seq, event_type: "stdout", data_text }`, capture stderr as `event_type: "stderr"`. On process exit, emit `RunTerminal { run_id, status }` where status maps exit code 0 → "completed", non-zero → "failed". Implement daemon WS client stub: connects to `ws://localhost:{PORT}/ws/daemon` with `Authorization: Bearer $ENDGOAL_DAEMON_TOKEN` header, receives JSON `RunDispatch`, dispatches to appropriate adapter, streams events back. Backend stub: accept WS connection at `/ws/daemon`, log received events to stdout (no DB writes yet — replaced in CP05).
- Acceptance criteria: `cargo run -p daemon` connects to a running backend stub, logs "Daemon connected". Sending `RunDispatch { run_id: "test-1", input: { runtime: "echo", ... } }` to daemon over WS causes: (a) `scratchpads/run-test-1/` directory created; (b) one `RunEvent { data_text: "hello" }` received by backend stub; (c) one `RunTerminal { status: "completed" }` received. Sending `RunDispatch` with non-existent binary causes `RunTerminal { status: "failed" }` with error in data_text (not a panic). `ENDGOAL_SCRATCHPAD_ROOT` env var changes scratchpad location. `cargo test -p daemon` passes subprocess unit tests using `EchoAdapter`.
- Depends on: 01
- Type: backend

### Checkpoint 04: Run API + enforcement rules + review endpoint
- Scope: Implement Run API: `POST /api/nodes/:id/runs` (dispatch), `GET /api/nodes/:id/runs`, `GET /api/runs/:id`, `PATCH /api/runs/:id/output` (write `output_json` — used by smoke test to populate structured results for echo runs; in production, this would be written by a post-processing step). `POST /api/nodes/:id/runs` enforces: (1) Node phase must be Active (else 422 `{ error: "wrong_phase" }`), (2) acceptance must be Structured (else 422 `{ error: "requires_freeze" }`), (3) Node must not be In-Review (else 422 `{ error: "in_review_gate" }`). On successful dispatch: write Run row to DB with `status: "dispatched"` and frozen `input_snapshot_json` (deep copy of intent + acceptance + effective_policy + ancestor summaries), then send `RunDispatch` message to daemon via WS (fire-and-forget, using the backend stub WS endpoint from CP03 upgraded to real hub in CP05). Run `type: "exploration"` bypasses the structured-acceptance requirement. Implement `POST /api/nodes/:id/approve` (In-Review → Complete, 400 if wrong phase) and `POST /api/nodes/:id/reject` (In-Review → Active, 400 if wrong phase, optionally accepts `{ tighter_policy: {...} }` body).
- Acceptance criteria: `POST /api/nodes/:id/runs` on Active+Structured Node returns `{ id, status: "dispatched" }` and writes run row. On prose-acceptance Node returns 422 `requires_freeze`. On In-Review Node returns 422 `in_review_gate`. On Draft Node returns 422 `wrong_phase`. `GET /api/runs/:id` returns run with `input_snapshot_json` non-null. `POST /api/nodes/:id/approve` on In-Review Node transitions to Complete; on Active Node returns 400. `POST /api/nodes/:id/reject` on In-Review Node transitions to Active. Exploration Run dispatches successfully against prose-acceptance Node. `cargo test -p backend -- runs` passes all enforcement tests. Note: actual subprocess spawn is not verified here (WS to daemon stub from CP03); full integration tested in CP05.
- Depends on: 02, 03
- Type: backend

### Checkpoint 05: WebSocket hub (full implementation)
- Scope: Implement complete WS hub replacing CP03's backend stub. Hub state: `Arc<RwLock<Hub>>` containing `daemon: Option<DaemonConn>` and `frontend_clients: HashMap<ClientId, Sender>`. `ws/daemon` endpoint: upgrades WS, authenticates via `Authorization: Bearer`, stores as `Hub.daemon`. On first `RunEvent` received for a Run: flip `runs.status` from `dispatched` to `running`, stamp `runs.started_at = now()`, then write to `run_events` table, broadcast `{ type: "run:updated", id: run_id }` to all frontend clients. On subsequent `RunEvent`: write to `run_events`, broadcast. On `RunTerminal` received: update `runs.status` in DB, broadcast `{ type: "run:updated", id: run_id }` and `{ type: "node:updated", id: node_id }`. On daemon disconnect: mark all runs with `status: "running"` belonging to this daemon session as `status: "failed"`. `ws/frontend` endpoint: upgrades WS, adds client to `Hub.frontend_clients`, removes on disconnect. No data flows from frontend WS to backend (receive-only signal channel). Update `POST /api/nodes/:id/runs` dispatch: after writing Run row, look up `Hub.daemon` and send `RunDispatch` (return 503 if no daemon connected).
- Acceptance criteria: Integration test using `tokio-tungstenite` test clients in `cargo test -p backend -- ws`: (a) connect daemon client, (b) connect frontend client, (c) POST dispatch a Run, (d) send `RunEvent` from daemon client, (e) assert frontend client receives `{ type: "run:updated" }` within 2 seconds. Daemon disconnect test: daemon connects, Run is dispatched (status: running), daemon disconnects, Run status becomes "failed" in DB within 1 second. POST /api/nodes/:id/runs returns 503 when no daemon connected. `cargo test -p backend -- ws` passes all hub tests.
- Depends on: 03, 04
- Type: backend

### Checkpoint 06: State Layer — state_at() implementation
- Scope: Implement `state_at(pool: &SqlitePool, node_id: &str, rollup_depth: u8) -> Result<NodeState>`. Reads: `nodes` row (acceptance_json, canonical_artifact_text), `runs.output_json` for latest completed Run (NOT `run_events` — those are for streaming/audit only), `nodes` for children (for rollup). Computes `progress` using locked formula: `(assertions_pass_rate × 0.40 + metric_achievement × 0.40 + rubric_normalized × 0.20) × 100`. Computes `confidence` = `avg(rubric_scores / scale)`. `next_step`: read from `nodes.next_step_cache` column; if null or `canonical_artifact_text` changed since last cache, generate via one Claude API call (configurable model via `ENDGOAL_FREEZE_MODEL` env var, default `claude-sonnet-4-6`) and write back to `nodes.next_step_cache`. Implement `parent_context` assembly: single CTE query walking ancestor chain, build `Vec<AncestorSummary>` (intent, phase, first 300 chars of acceptance, first 500 chars of canonical, progress). Rollup: recursively call `state_at` for children up to `rollup_depth`, surface any child with `state: Blocked` in `rollup_blockers`. Expose as `GET /api/nodes/:id/state?rollup_depth=N` endpoint. Inject LLM dependency via function pointer or trait so tests can stub it without network calls.
- Acceptance criteria: Unit test fixture: Node with Structured acceptance (assertions: 2 pass / 1 fail, metrics: one at 60% of target, rubric: score 7 out of 10 scale). Expected progress: `(0.667×0.4 + 0.6×0.4 + 0.7×0.2)×100 = (0.267 + 0.24 + 0.14)×100 = 64.7` → assert in range [63, 67]. All-passing fixture: progress == 100. `GET /api/nodes/:id/state` returns `NodeState` JSON with correct shape per ts-rs bindings. `parent_context` for depth-3 Node: assert array length == 2 with correct ancestor data. Rollup test: Node with one child that has `state: Blocked` → `rollup_blockers` contains child id. `next_step` is non-empty string (stubbed in tests). `cargo test -p backend -- state` passes all State Layer tests. Claude API call is injectable (tested with stub returning "mock next_step").
- Depends on: 02
- Type: backend

### Checkpoint 07: Frontend scaffold + WS provider + Layer 0a (Workspace Overview)
- Scope: Establish all frontend infrastructure consumed by later checkpoints. Set up `lib/api.ts` typed fetch wrapper (functions: `getNodes()`, `getNode(id)`, `getNodeState(id)`, `getNodeAncestors(id)`, `getNodeChildren(id)`, `getRuns(nodeId)`, `getRun(id)` — return types from `bindings/*.d.ts`). Set up `features/realtime/provider.tsx` (WS connection to `ws/frontend`, subscribe/unsubscribe pattern matching Multica's `use-realtime-sync.ts`). Implement Layer 0a (Workspace Overview) at `app/page.tsx`: fetch top-level Nodes via `getNodes()`, render card grid. Each card shows: `intent` text, phase badge (color-coded), own progress bar, rollup progress bar (from `NodeState`), flag icons (blocker/review from `rollup_blockers`), last-updated timestamp, `next_step` truncated to 2 lines (from `NodeState.next_step`), "Open tree →" link to `/nodes/[id]`. WS invalidation: on `node:updated` signal, refetch affected node state.
- Acceptance criteria: Workspace Overview renders at `http://localhost:3000` with at least one Node fetched from a running backend. Card shows correct phase badge color (draft=gray, active=indigo, in-review=amber, complete=green). Progress bar reflects `NodeState.progress` value. Creating a Node via `POST /api/nodes` causes the card to appear within 2 seconds without manual page reload (WS invalidation path). `pnpm typecheck` passes with zero errors. No `any` types in `features/nodes/` or `lib/api.ts`. Playwright test (or manual verification documented in output-summary.md): Overview loads, card visible, WS invalidation triggers update.
- Depends on: 02, 05, 06
- Type: frontend

### Checkpoint 08: Layer 0b (Objective Tree) + Layer 1 (Node Panel)
- Scope: Implement Layer 0b (Objective Tree) at `app/nodes/[id]/page.tsx`. Breadcrumb: "← Workspace overview / {root intent} / ... / {current intent}". Top-down recursive tree rendering: root Node at top, children below with 22px left indent per level, vertical connector lines. Each tree row (`node-card`): phase left-border (3px, phase color), intent text, progress bar, phase badge, flags. Clicking any row opens Layer 1 panel (side panel, not page navigation). Implement Layer 1 (Node Panel) at `features/panel/node-panel.tsx`. Panel shows: phase badge + intent (h1), acceptance section (prose: text area read-only; structured: assertion rows with pass/fail/pending badge + text + check_fn note; metric rows with baseline/current/target + progress bar; rubric rows with dimension name + score/scale), runs list (type, status badge, created_at, findings snippet; click → Layer 2), next_step from NodeState, action buttons (Edit Intent, Trigger Run, Add Note, Archive). Layer 1 is singleton: opening a new panel closes previous one.
- Acceptance criteria: Navigating to `/nodes/{root_id}` renders the full subtree (up to depth 3 in test data). Clicking a child node row opens Node Panel as side panel without URL change. Node Panel shows structured acceptance with correct section groups (Assertions / Metrics / Rubric). Assertion with `status: "pass"` shows green badge. Metric at 60% shows progress bar at 60%. Runs list shows most recent run at top. Closing panel by clicking elsewhere or pressing Escape. `pnpm typecheck` passes. Multica's feature folder conventions followed: components are in `features/panel/components/`, hooks in `features/panel/hooks/`.
- Depends on: 07
- Type: frontend

### Checkpoint 09: Archetype B gate modal + exploration dispatch path
- Scope: Wire "Trigger Run" button in Node Panel (Layer 1). If Node acceptance is Structured and phase is Active: dispatch directly (`POST /api/nodes/:id/runs`). If Node acceptance is Prose: intercept and show Archetype B modal. Modal content: Node intent displayed at top, three choices as buttons: "Freeze now" (primary/accent), "Proceed as exploration" (secondary), "Cancel". "Cancel": close modal, no side effects. "Proceed as exploration": call `POST /api/nodes/:id/runs` with `{ type: "exploration" }`, close modal, show toast "Run dispatched as exploration". "Freeze now": close modal, open freeze session view (CP11, show loading state until CP11 implemented). Backend: accept `type: "exploration"` in `POST /api/nodes/:id/runs` without requiring Structured acceptance (already implemented in CP04 — this CP wires the frontend).
- Acceptance criteria: "Trigger Run" on Structured-acceptance Active Node dispatches directly (no modal). "Trigger Run" on Prose-acceptance Node shows Archetype B modal. "Cancel" closes modal without network call. "Proceed as exploration" calls POST with type:exploration and shows success toast. Modal renders correctly on mobile viewport (no overflow). `pnpm typecheck` passes. Note: "Freeze now" leads to a loading/placeholder state until CP11 is complete.
- Depends on: 04, 08
- Type: frontend

### Checkpoint 10: Freeze session backend (state machine + SSE streaming)
- Scope: Implement freeze session backend entirely, no frontend yet. DB table `freeze_sessions` already in schema from CP01. Backend endpoints: `GET /api/nodes/:id/freeze/active` (returns active session `{ session_id, approved_items_json, current_layer }` or `null` if no active session — used by CP11 to detect resume vs. start), `POST /api/nodes/:id/freeze/start` (creates freeze_session row, returns `{ session_id }`; if an active session exists, marks it `abandoned` first), `POST /api/nodes/:id/freeze/respond` (accepts `{ session_id, user_response: string, action: "approve" | "edit" | "reject", approved_item_json?: string }` — `action` records the user's decision on the previous proposal; if `action: "approve"` or `action: "edit"`, `approved_item_json` is appended to `freeze_sessions.approved_items_json` in DB before generating the next proposal; this ensures every approval is persisted server-side for resume; returns SSE stream with next proposal), `POST /api/nodes/:id/freeze/commit` (converts `approved_items_json` in freeze_session → `StructuredAcceptance`, writes to `nodes.acceptance_json`, transitions Node to Active if was Draft, marks session committed). State machine transitions: `active` → `committed` (on commit) or `abandoned` (on new freeze/start for same node). Proposal generation: prompt includes Node intent + docs + parent context + already-approved items. Generate assertions first (one per SSE event), then metrics, then rubric. Final SSE event per layer: `{ layer: "...", item_json: null, reasoning: "layer complete", source_quote: "" }` signals layer done. When all three layers done: `{ layer: "done", ... }`. Handle client disconnect: cancel `reqwest` stream (use `CancellationToken`).
- Acceptance criteria: `GET /api/nodes/:id/freeze/active` returns `null` when no active session, returns `{ session_id, approved_items_json, current_layer }` when active. `POST /api/nodes/:id/freeze/start` creates freeze_session with `status: "active"`. `POST /api/nodes/:id/freeze/respond` returns SSE stream; test client receives at least one event matching `{ layer: "assertion", item_json: "{...}", reasoning: "...", source_quote: "..." }` (integration test using stub Claude API injected via the trait/function pointer — no network call in `cargo test`). State machine: calling `/start` twice on same Node marks first session as `abandoned`, creates new one. `/commit` with `approved_items_json` containing at least 1 approved item in any layer (layers are optional) writes Structured acceptance to Node and transitions Node to Active; `GET /api/nodes/:id` confirms `acceptance_json` is no longer prose. `/commit` on already-committed session returns 409. `cargo test -p backend -- freeze` passes all state machine tests. SSE streaming tested with stub Claude API.
- Depends on: 02, 06
- Type: backend

### Checkpoint 11: Freeze session frontend (chat UI + commit flow)
- Scope: Implement freeze session as a full-page route at `app/nodes/[id]/freeze/page.tsx`. The Archetype B modal "Freeze now" button navigates to this route (`router.push('/nodes/${id}/freeze')`) rather than mounting an overlay — this gives the session a stable URL that survives browser reload. On mount: call `GET /api/nodes/:id/freeze/active`; if active session found, resume from `approved_items_json`; if no active session, call `POST /api/nodes/:id/freeze/start`. If no active session and Node acceptance is already Structured, redirect to Node Panel. Implement `features/freeze/freeze-session.tsx` as the main component rendered by this route. Layout: header shows Node intent + layer progress indicator (Assertions → Metrics → Rubric → Done). Message list: agent messages on left (proposal card showing `reasoning` + `source_quote` + `item_json` rendered as editable fields), user messages on right (textarea + Send button). **Session lifecycle**: on mount at `/nodes/[id]/freeze`: call `GET /api/nodes/:id/freeze/active`; if active session returned, restore `approved_items_json` from response and resume from current layer; if no active session, call `POST /freeze/start` which returns `{ session_id }`, then immediately call `POST /freeze/respond { session_id, action: "start", user_response: "" }` to receive the first proposal SSE stream. **Per-turn flow**: user reads agent proposal → user actions map to `/freeze/respond` calls: "Approve as-is" → `{ action: "approve", approved_item_json: <item from proposal>, user_response: "" }`, "Edit then approve" → `{ action: "edit", approved_item_json: <edited item>, user_response: "<edit notes>" }`, "Reject" → `{ action: "reject", user_response: "<rejection reason>" }`, "Skip this layer" → `{ action: "reject", user_response: "skip_layer" }`. Each call returns SSE stream with next proposal. Approved items accumulate server-side (in `freeze_sessions.approved_items_json`). When `layer: "done"` event received: show "Move to next layer" button (or "Skip" to skip remaining layers). When all layers done or final `layer: "done"`: show "Review all & Commit" summary + "Commit acceptance" button. On commit: `POST /freeze/commit`, navigate to Node Panel which now shows Structured acceptance. **Session resumability**: navigating to `/nodes/[id]/freeze` after browser close restores full state from `GET /freeze/active`; no local-only state.
- Acceptance criteria: Freeze session opens with agent's first proposal visible. User types "make it more specific" → next proposal reflects the feedback. Approved items counter increments. Layer progress indicator advances after all assertions done. "Commit acceptance" button appears when at least one item has been approved in any layer (layers are optional per foundations §7 — "Each layer is optional. A simple Node may have only two assertions and no metrics or rubric."). User may skip layers via a "Skip this layer" button. After commit: Node Panel shows Structured acceptance (assertion rows, metric rows, rubric rows). Reopening frozen Node after page reload shows Structured acceptance (persisted). Session resumability: simulate browser close by navigating away and returning to the freeze session URL — active session state restored from DB. `pnpm typecheck` passes.
- Depends on: 09, 10
- Type: fullstack

### Checkpoint 12: Layer 2 Run Detail + live stdout SSE endpoint
- Scope: Implement `GET /api/runs/:id/stream` SSE endpoint: if Run is `running`, pipe live `run_events` rows as SSE as they arrive (poll `run_events` table every 200ms for new rows by `seq` since last sent); if Run is `completed/failed`, send all `run_events` rows from DB then close stream. Implement Layer 2 Run Detail overlay (`features/runs/run-detail-overlay.tsx`). Opened by clicking a Run row in Node Panel (Layer 1). Overlay (not page navigation) shows: `run_input_snapshot` JSON rendered as collapsible tree, stdout stream via `EventSource` on `/api/runs/:id/stream` (scrolling live view, autoscroll to bottom), audit trail timeline (run_events with `event_type: "stdout"/"stderr"/"system"` grouped, timestamps), Run metadata (type, status, runtime, started_at, ended_at). Stream closes gracefully on Run completion. "Return to panel" closes overlay.
- Acceptance criteria: Clicking Run row in Node Panel opens overlay. run_input_snapshot is visible (check for `intent` key in rendered JSON). Stdout stream shows lines as they arrive for a running `echo`-runtime Run (tested with a Run that runs for ~2 seconds via `sleep 2 && echo done`). After Run completes, stream closes and status badge updates from "running" to "completed" (WS invalidation). Overlay closes on Escape key. `pnpm typecheck` passes. `cargo test -p backend -- stream` passes: test sends 3 run_events to DB for a completed Run and asserts SSE endpoint returns all 3 then closes.
- Depends on: 04, 05, 08
- Type: fullstack

### Checkpoint 13: Human review gate + end-to-end smoke test
- Scope: Implement In-Review UI in Node Panel (Layer 1): when Node phase is `in_review`, show "Approve synthesis" (success/green button) and "Reject — back to Active" (danger/red button); hide "Trigger Run" button entirely (do not show disabled). On Approve: `POST /api/nodes/:id/approve`, Node transitions to Complete, panel updates. On Reject: optional text field "Reason / tighter constraint", `POST /api/nodes/:id/reject` with optional policy body, Node transitions to Active. Add `GET /api/health → 200 OK` to backend for process readiness polling. Implement end-to-end smoke test as `scripts/e2e-smoke.sh`: (1) POST create Node with prose acceptance, (2) POST /freeze/start, (3) POST /freeze/respond (stub Claude returns one assertion proposal), (4) POST /freeze/commit with one approved assertion, (5) assert Node phase=Active, acceptance=Structured, (6) POST dispatch Run with `runtime: "echo"` and message "smoke_test_output", (7) assert Run status transitions dispatched→running→completed (poll /api/runs/:id with max 10s timeout), (8) assert run_events table has ≥1 row with data_text containing "smoke_test_output", (8b) PATCH /api/runs/:id/output with mock `output_json` containing `{ assertion_results: { "a1": "pass" }, metric_values: { "m1": 80 }, rubric_scores: { "r1": 8 }, confidence: 0.8, findings: "smoke test pass", concerns: [], needs_human_review: false }` (this gives state_at() real data to compute progress), (9) POST /api/nodes/:id/review → In-Review, (10) POST /api/nodes/:id/approve → Complete, (11) assert Node phase=Complete, input_snapshot_json non-null, (12) assert GET /api/nodes/:id/state returns progress > 0 (expected: ~72 based on 100% assertion × 0.4 + 80% metric × 0.4 + 80% rubric × 0.2 = 88, but acceptance has only 1 of each so scaling may differ). Note: `canonical_summary = None` is acceptable in smoke test (canonical_artifact not populated for MVP).
- Acceptance criteria: Node Panel In-Review view shows Approve and Reject buttons; Trigger Run button absent. Approve transitions Node to Complete; Approve button disappears. Reject transitions to Active; Trigger Run reappears. `scripts/e2e-smoke.sh` passes with exit code 0: script starts backend and daemon as background processes (both with matching `ENDGOAL_DAEMON_TOKEN`), polls `GET /api/health` until 200 (max 10s), then runs curl-based assertions for all 12 smoke steps, terminates both processes on exit (trap EXIT). All 12 smoke assertions succeed. `cargo test --workspace` passes. `pnpm typecheck` passes. `pnpm build` succeeds with zero errors.
- Depends on: 04, 05, 06, 11, 12
- Type: fullstack

---

## Out of Scope

- Multi-user concurrency (CRDT / pessimistic locking) — single-user assumption throughout
- Cloud API RuntimeAdapter (future implementation behind same trait)
- Fork-Return-Synthesize parallel sub-Runs — single Run per Node dispatch for MVP
- Judge calibration subsystem (CalibrationRun, GraderTrustScore, Cronbach's alpha)
- Cross-goal knowledge reuse (KnowledgeArtifact, CrossGoalLink)
- Budget semantics during contract-change reconcile (architecture §15.3)
- Full contract-change reconciliation for in-flight Runs (MVP: daemon disconnect marks Runs failed only)
- Agent-driven escalation (`needs_human_review` propagation through Run → Node → UI): MVP uses manual `POST /api/nodes/:id/review` to enter In-Review. This is an explicit MVP simplification of architecture §12 (agent-driven escalation). Post-prototype: add `needs_human_review` field to `output_json`, auto-transition Node to In-Review when Run completes with `needs_human_review: true`
- Production deployment (Postgres, Docker, Vercel)
- `measurement_fn` programmatic execution (manual or LLM-scored only for MVP)
- Archived phase management UI (phase exists in schema; delete/archive button sets phase=archived; no archive list view)
- `done_when` combinator in StructuredAcceptance (foundations §7 specifies `all_assertions_pass + all_metrics_meet_target + rubric_min_score`; MVP uses the simpler weighted formula in Technical Approach; full `done_when` deferred)
- `rollup_depth` session preference UI (defaults to full rollup, not user-configurable in MVP)
- Scratchpad file browser in Layer 2 (security surface deferred to post-prototype)
- `PATCH /api/nodes/:id/canonical` endpoint (canonical_artifact not updated via UI in MVP smoke test)

---

## Open Questions

1. **Freeze session model**: Which model generates freeze proposals? `ENDGOAL_FREEZE_MODEL` env var, default `claude-sonnet-4-6`.
2. **Daemon auth**: `ENDGOAL_DAEMON_TOKEN` env var; sent as `Authorization: Bearer` in WS upgrade; backend rejects upgrade without matching token.
3. **Run budget defaults**: `tokens_max=100000`, `iterations_max=null` (no limit for MVP), `wallclock_max_s=300`.
4. **canonical_artifact MVP**: Updated only via manual `PATCH /api/nodes/:id/canonical` (not exposed in UI for MVP). `canonical_summary = None` in all MVP smoke tests — explicitly acceptable.
5. **ts-rs generation trigger**: `cargo test --workspace --features generate-bindings` writes bindings; CI step runs `git diff --exit-code frontend/bindings/` to fail if stale. Pre-commit hook optional.
6. **next_step cache invalidation**: `next_step_cache` regenerated when `canonical_artifact_text` column changes (compare `canonical_updated_by_run_id` against cached version). If Node has no canonical artifact, `next_step = "No runs completed yet — dispatch a research_iteration Run to begin."` (hardcoded, no API call).
7. **WS hot reload (dev experience)**: Next.js hot reload causes WS provider remount, creating stale backend connections. Acceptable for dev; backend hub removes closed connections lazily on next broadcast. Document in CLAUDE.md as known dev quirk, not a bug.
