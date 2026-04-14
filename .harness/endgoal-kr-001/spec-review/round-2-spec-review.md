---
task_id: endgoal-kr-001
spec_version: 2
round: 2
---

# Round 2 Spec Review — EndGoal Single-KR Prototype

## Verdict

**approve** — with one warning that requires a decision (not a rewrite) before the Generator begins CP13.

Both Round 1 criticals are genuinely resolved. The 13-checkpoint structure is sequenced correctly. No critical blockers remain. One new warning was introduced by the restructuring (smoke test daemon coordination), one existing warning surfaced by the new CP structure (freeze session route not defined), and two info items round out the concerns. The Generator can proceed from CP01 through CP12 without ambiguity. CP13 requires one clarifying decision recorded before execution.

---

## Scope Assessment

### Round 1 Critical Resolutions

**Critical 1 (CP04 circular WS verification) — RESOLVED.**
CP05 acceptance criteria now specify `tokio-tungstenite` test clients in `cargo test -p backend -- ws`. The criterion is fully backend-verifiable. The planner response correctly placed `POST /api/nodes/:id/review` (Active → In-Review) in CP02 scope, not CP04 — this is the right location since it is a phase transition endpoint rather than a Run lifecycle endpoint.

**Critical 2 (CP07 SSE+UI in one checkpoint) — RESOLVED.**
The split into CP09 (Archetype B modal), CP10 (freeze backend + SSE), CP11 (freeze frontend) is correct. Each is independently verifiable. The SSE event schema is now specified in the Technical Approach (`layer`, `item_json`, `reasoning`, `source_quote`). CP10 acceptance criteria confirm the contract before CP11 begins building against it.

### Round 1 Warning Resolutions

All five warnings addressed: progress formula locked (assertions 40% / metrics 40% / rubric 20%), ts-rs annotation coverage added to CP01, scratchpad listing deferred to Out of Scope, `lib/api.ts` typed wrapper mandated in CP07 scope, and `canonical_summary = None` explicitly acceptable in smoke test. All accepted without regression.

### Complexity Score (v2)

- Checkpoints: 13 (up from 8 in v1). The increase is appropriate; all splits reduce blast radius.
- Files to create: ~60-80, unchanged. Greenfield cost is unchanged.
- New abstractions introduced: 5, unchanged. No new abstractions added by restructuring.
- Dependency graph depth: CP01 → CP02 → CP05 → CP07 → CP08 → CP09 → CP11 → CP13 is 8 levels deep. Linear chains at this depth create scheduling risk but do not block execution — each checkpoint is independently verifiable.

### What Already Exists

Nothing. Greenfield. No change from Round 1 assessment.

---

## Checkpoint Review

| CP | Title | Granularity | Acceptance Criteria | Dependencies | TDD Readiness | Effort |
|---|---|---|---|---|---|---|
| 01 | Rust workspace + DB schema + type bindings | OK | TESTABLE | CORRECT (none) | YES | M |
| 02 | Node CRUD API + phase lifecycle | OK | TESTABLE | CORRECT (01) | YES | M |
| 03 | Daemon + RuntimeAdapter + subprocess spawn | OK | TESTABLE | CORRECT (01) | YES | M |
| 04 | Run API + enforcement + review endpoint | OK | TESTABLE | CORRECT (02, 03) | YES | M |
| 05 | WebSocket hub (full implementation) | OK | TESTABLE | CORRECT (03, 04) | PARTIAL | M |
| 06 | State Layer — state_at() | OK | TESTABLE | CORRECT (02) | YES | M |
| 07 | Frontend scaffold + WS provider + Layer 0a | OK | TESTABLE | CORRECT (02, 05, 06) | PARTIAL | M |
| 08 | Layer 0b (Objective Tree) + Layer 1 (Node Panel) | OK (edge) | TESTABLE | CORRECT (07) | PARTIAL | M |
| 09 | Archetype B gate modal + exploration dispatch | OK | TESTABLE | CORRECT (04, 08) | PARTIAL | S |
| 10 | Freeze session backend (state machine + SSE) | OK | TESTABLE | SEE NOTE | YES (backend) | M |
| 11 | Freeze session frontend (chat UI + commit) | OK | TESTABLE | CORRECT (09, 10) | PARTIAL | M |
| 12 | Layer 2 Run Detail + live stdout SSE | OK | TESTABLE | CORRECT (04, 05, 08) | PARTIAL | M |
| 13 | Human review gate + end-to-end smoke test | OK | TESTABLE | INCOMPLETE — see Concern 1 | PARTIAL | L |

### Checkpoint Notes

**CP01**: The spot-check criterion names 7 of 17 defined types. The corrected criterion ("every type used in API responses or WS messages carries `#[derive(TS)]`") closes the gap. Compiler enforcement via `git diff --exit-code frontend/bindings/` is the right gate.

**CP02**: `POST /api/nodes/:id/review` correctly placed here (Active → In-Review). CP04 handles approve/reject (In-Review → Complete/Active). The split is clean and the acceptance criteria for each are non-overlapping.

**CP03**: Backend stub ("log received events to stdout") is explicitly marked as throw-away scaffolding, replaced in CP05. The Generator must not over-engineer this stub. This guidance is present in the scope text and is sufficient.

**CP04**: The four 422 enforcement rules are now independent criteria. "Exploration Run dispatches successfully against prose-acceptance Node" is a concrete test that validates the bypass path. Criterion count is 7 — at the edge but each is independently verifiable with a single HTTP call.

**CP05**: TDD readiness is PARTIAL because the concurrent WS hub test (connect daemon, connect frontend client, dispatch run, assert signal received within 2s) requires an in-process test server. `tokio-tungstenite` in `cargo test` can do this, but the Generator must use `tokio::test` with a test-local server. The spec does not specify this setup, but it is standard `axum` testing practice (`axum::Server::bind(SocketAddr::from(([127,0,0,1], 0)))`). Not a blocker — flagged as info.

**CP06**: The dependency on CP02 only (not CP03/CP04/CP05) is correct — `state_at()` is a pure DB reader. The LLM injection via function pointer or trait is specified. Progress formula fixture: `(0.667×0.4 + 0.6×0.4 + 0.7×0.2)×100 = 64.7`, range 63-67 is correct and tight enough to catch formula errors.

**CP07**: Three dependencies (02, 05, 06) are all correct. `lib/api.ts` is established here. WS provider pattern referenced from multica file. The acceptance criterion "Creating a Node via POST causes card to appear within 2 seconds" is testable via manual verification documented in output-summary.md — this is adequate for a greenfield MVP.

**CP08**: Seven acceptance criteria — at the "5+ = too large" boundary. However, the criteria are uniform UI behavior checks (panel rendering, badge colors, Escape key), not distinct subsystems. The scope is coherent: one tree view + one side panel. Keeping merged is the right call.

**CP09**: Smallest checkpoint in the spec (S effort). Acceptance criteria are concrete behavioral assertions (modal renders, Cancel has no network call, exploration dispatches with type:exploration). The "Freeze now leads to loading/placeholder state until CP11" is explicit — Generator will not be confused.

**CP10**: See Concern 2 below for the freeze session route question. Backend state machine and SSE criteria are TESTABLE. "Integration test with real or stub Claude API" is vague on which — should be stub in unit tests, real as optional integration. This is workable but the Generator may reach for real API and fail in CI.

**CP11**: Session resumability criterion ("simulate browser close by navigating away and returning to freeze session URL") assumes a stable URL for the freeze session. The spec does not define this route. See Concern 2.

**CP12**: Polling at 200ms for new `run_events` rows is specified. SSE close-on-completion is specified. The `sleep 2 && echo done` test is a concrete integration anchor for live streaming. Backend SSE test (3 events, assert all returned then stream closes) is mechanically verifiable.

**CP13**: Depends listed as 04, 05, 06, 11, 12. CP03 is a transitive dependency via CP04 (CP04 depends on 03). Transitive dependencies are typically not re-stated. However, the smoke test step 6 ("POST dispatch Run with runtime: echo, assert dispatched→running→completed") requires the daemon process to be running. The smoke test is defined as `cargo test -p backend -- e2e` but cannot launch the daemon binary automatically within a backend-only test. This is the primary unresolved concern. See Concern 1.

---

## Concerns

### Concern 1 — smoke test daemon coordination
**Severity: warning**

**Details**: CP13's smoke test is defined as `cargo test -p backend -- e2e` (or `scripts/e2e-smoke.sh`). Step 6 dispatches a Run with `runtime: "echo"` and asserts the Run transitions dispatched → running → completed. This transition requires the daemon process to be running, connected via WebSocket, and processing the `RunDispatch` message via `EchoAdapter`. A test in `cargo test -p backend` cannot launch the daemon binary without explicitly spawning it via `std::process::Command::new("cargo").args(["run", "-p", "daemon"])`. This spawning approach is fragile (requires build artifacts to be present, timing-sensitive, and requires cleanup). The alternative — `scripts/e2e-smoke.sh` — can start the backend and daemon as background processes and coordinate them, but this form is not specified as the primary path.

The spec offers both options ("or shell script") without choosing. The Generator will make a free choice. If it chooses `cargo test -p backend -- e2e` without daemon spawning, the smoke test will hang on step 7 (polling `/api/runs/:id` for status change that never comes) or the dispatch itself will return 503 (no daemon connected). Either way, the smoke test fails without a clear error message pointing to the missing daemon.

**Suggested fix**: Specify the form: recommend `scripts/e2e-smoke.sh` for the smoke test (it is the correct tool for coordinating multiple processes). Specify that the script starts backend and daemon as background processes, waits for both to be healthy (poll `/health` endpoint or use `cargo run -p daemon &` with a sleep), then runs curl-based assertions, then terminates both processes on exit. Add a `/health` endpoint to the backend (trivially `GET /api/health → 200`) if not already present, so the script can poll for readiness. Note: if the Generator prefers the Rust test approach, specify that it must spawn the daemon binary via `tokio::process::Command` and await daemon WS connection before proceeding.

---

### Concern 2 — freeze session URL / route not specified
**Severity: warning**

**Details**: CP11 opens the freeze session as a component (`features/freeze/freeze-session.tsx`). The session resumability criterion says "navigating away and returning to the freeze session URL — active session state restored from DB." For session resumability to work, the freeze session must be accessible at a stable URL, not just mounted as an overlay. The spec defines the freeze session as opened by "Freeze now" in the Archetype B modal (CP09), which is a modal overlay — not a page navigation. An overlay rendered over the Node Panel does not have its own URL.

Two possible implementations exist: (a) freeze session as a full-page route at `/nodes/[id]/freeze` with session state loaded from DB on mount, or (b) freeze session as an overlay with browser history manipulation (pushState) to create a stable URL. Neither is specified. The Generator must choose, and the choice affects both the routing structure and the resumability implementation.

The acceptance criterion "simulate browser close by navigating away and returning to the freeze session URL" is untestable if there is no defined URL.

**Suggested fix**: Add to CP11 scope: "Freeze session is rendered as a full-page route at `app/nodes/[id]/freeze/page.tsx`. Navigating to this URL resumes an active session from DB or redirects to Node Panel if no active session exists. The Archetype B modal 'Freeze now' button navigates to this route (`router.push('/nodes/${id}/freeze')`) rather than mounting an overlay." This one-line decision makes resumability trivially testable and avoids history manipulation complexity.

---

### Concern 3 — CP10 Claude API stub ambiguity in acceptance criteria
**Severity: info**

**Details**: CP10 acceptance criterion says: "test client receives at least one event matching `{ layer: 'assertion', ... }` (integration test with real or stub Claude API)." The phrase "real or stub" leaves the testing strategy open. If the Generator uses the real API in `cargo test -p backend -- freeze`, the tests are flaky (network-dependent, rate-limited, slow), and CI will be unreliable. If the Generator stubs it, it needs to decide the stub mechanism (the spec says "inject LLM dependency via function pointer or trait so tests can stub it without network calls" — this is correctly specified in the scope text). The acceptance criteria should match the scope text's explicit guidance.

**Suggested fix**: Amend the CP10 acceptance criterion to: "test client receives at least one event (integration test using stub Claude API injected via the trait/function pointer specified in scope — no network call in `cargo test`)."

---

### Concern 4 — `next_step` cache invalidation comparison mechanism underspecified
**Severity: info**

**Details**: Open Question 6 specifies that `next_step_cache` is regenerated "when `canonical_artifact_text` column changes (compare `canonical_updated_by_run_id` against cached version)." The phrase "against cached version" is ambiguous: what column holds the "cached version" of `canonical_updated_by_run_id`? The DB schema shows `canonical_updated_by_run_id TEXT` on the `nodes` table, but there is no `next_step_canonical_run_id` column to track which run the cache was generated for. The Generator will need to add a column to track this, or use a different comparison strategy (e.g., store a hash of `canonical_artifact_text` alongside `next_step_cache`).

CP06's scope text says: "if null or `canonical_artifact_text` changed since last cache" — but "since last cache" is undefined without a tracking column. The spec and Open Question 6 are slightly inconsistent.

**Suggested fix**: Add to the DB schema (CP01): `next_step_cache_for_run_id TEXT` on the `nodes` table. Update CP06 scope: "Regenerate `next_step_cache` if `canonical_updated_by_run_id != next_step_cache_for_run_id` (or if cache is null). After regeneration, set `next_step_cache_for_run_id = canonical_updated_by_run_id`." This makes the invalidation logic deterministic and adds one column to the schema.

---

## Effort Estimate

| CP | Estimate | Notes |
|---|---|---|
| CP01 | M | Greenfield Rust workspace + 17 types + ts-rs wiring; mechanical but non-trivial |
| CP02 | M | 8 endpoints + phase invariants + CTE policy query; uniform boilerplate |
| CP03 | M | Daemon WS client + subprocess spawn; echo runtime contains blast radius |
| CP04 | M | Run CRUD + 4 enforcement rules + approve/reject; all backend, well-bounded |
| CP05 | M | Concurrent WS hub with two namespaces; daemon disconnect handling is subtle |
| CP06 | M | Pure function with clear signature; LLM injection is specified |
| CP07 | M | Next.js scaffold + WS provider + Layer 0a; reference codebase pointers reduce risk |
| CP08 | M | Tree + side panel UI; 7 acceptance criteria but uniform behavioral assertions |
| CP09 | S | Thin frontend: modal + two dispatch paths; smallest checkpoint |
| CP10 | M | Freeze state machine + SSE from Claude API; CancellationToken adds complexity |
| CP11 | M | Freeze chat UI + SSE consumption + resumability; route ambiguity (Concern 2) may add time |
| CP12 | M | SSE endpoint (polling) + Run Detail overlay; well-specified |
| CP13 | L | Smoke test coordination across two processes (Concern 1) + In-Review UI; plan the daemon coordination before implementing |

---

## Failure Modes

**CP01**: `#[derive(TS)]` is missing from a type that appears only inside a WS message body (e.g., `RunEvent` nested inside `WsDaemonMessage`). The binding for the outer type is generated, but the inner type has no `.d.ts`. `cargo build` succeeds; `pnpm typecheck` fails in CP07 with a cryptic import error. Root cause is invisible from the frontend.

**CP02**: `effective_policy` CTE query works for a depth-2 ancestor chain in the acceptance test but silently returns incorrect results for depth-3+ chains due to a CTE base-case error. The CP02 acceptance test only covers depth-3 (2 ancestors), so a bug in the CTE's recursive case is not exercised until CP13's smoke test, where a depth-3 Node's policy is used in `RunInput`.

**CP03**: `ClaudeCodeAdapter` and `CodexAdapter` are implemented but neither is tested because the acceptance criteria only exercise `EchoAdapter`. At CP13 smoke test time, `runtime: "echo"` works, but a real `research_iteration` Run using `ClaudeCodeAdapter` fails silently (binary not on PATH, wrong flags) with `RunTerminal { status: "failed" }`. The prototype passes the smoke test but fails real-world usage.

**CP04**: The `input_snapshot_json` is frozen at dispatch time and must capture the current `effective_policy` and all ancestor summaries. If the Generator serializes a reference to the node's policy rather than a deep copy, and the policy is mutated by a subsequent `PATCH`, the snapshot no longer reflects dispatch-time state. Audit trail is corrupted. The acceptance criterion checks that `input_snapshot_json` is non-null but does not verify immutability after a policy change.

**CP05**: Hub state uses `Arc<RwLock<Hub>>`. If `RunEvent` messages arrive faster than the `run_events` DB write completes (e.g., a subprocess emitting 1000 stdout lines rapidly), the broadcast to frontend clients may race with the DB write. Frontend receives `run:updated` signal, refetches `/api/runs/:id/stream`, but the latest events are not yet in DB. The frontend shows stale data for 200ms (one poll cycle). Acceptable for MVP; document as known behavior.

**CP06**: `next_step` is generated via Claude API on cache miss. If the first request to `GET /api/nodes/:id/state` for a node with no `canonical_artifact` triggers an API call (it should return the hardcoded string per Open Question 6), the hardcoded-string branch is not implemented and the API call returns a generic response. The CP06 acceptance criteria do not explicitly test the "no canonical artifact → hardcoded string" path.

**CP07**: `lib/api.ts` typed fetch wrapper is established here. If the Generator uses `fetch` directly in one feature directory (CP08, CP09, CP11, CP12) without going through `lib/api.ts`, `pnpm typecheck` still passes (fetch responses typed as `any` or cast). The acceptance criterion "No `any` types in `features/nodes/` or `lib/api.ts`" covers nodes and the wrapper itself but does not cover `features/freeze/`, `features/runs/`, or `features/panel/`. Silent type drift in later checkpoints.

**CP08**: Layer 1 Node Panel is a singleton ("opening a new panel closes previous one"). If the Generator implements this via a global context rather than URL state, and the user deep-links to `/nodes/[id]` with a panel already open in another tab, the panel state is desynchronized. Not critical for MVP, but the singleton logic must use component state rather than module-level singletons to avoid cross-tab contamination.

**CP09**: "Freeze now" closes the modal and opens freeze session. If the Archetype B modal and the freeze session use the same overlay/portal mechanism, closing the modal before the freeze session is mounted creates a brief unmounted state where the user sees nothing. The "loading/placeholder state until CP11 implemented" covers this for CP09, but CP11 must mount the freeze session before the modal finishes its close animation.

**CP10**: `POST /api/nodes/:id/freeze/respond` returns an SSE stream. If the Generator implements this as an `axum` handler that holds the response stream open while polling `reqwest`, it must correctly set `Content-Type: text/event-stream` and flush after each event. If flushing is not explicit (axum's default buffering may hold events), the browser receives all proposals at once when the stream closes rather than incrementally. The freeze session appears to "hang" then dump all proposals simultaneously.

**CP11**: Session resumability requires `POST /freeze/start` to be idempotent for resumed sessions (or detect existing active session). The state machine spec says "calling `/start` twice marks first session as abandoned." This means resumability cannot use `/start` — it must detect and load the existing active session differently. CP11 scope says "call `POST /freeze/start` if no active session, else resume from `approved_items_json`." The "else" branch requires a `GET` endpoint to check for active session — this endpoint is not listed in CP10's scope. Generator must add it or use a different mechanism.

**CP12**: The SSE endpoint polls `run_events` table every 200ms. For a Run that produces 5000 stdout lines (a long claude CLI session), the endpoint issues one DB query every 200ms × however long the run takes. A 5-minute Run (300s) generates 1500 queries, each scanning the `run_events` table by `run_id` and `seq`. Without an index on `(run_id, seq)`, this degrades linearly. The DB schema in CP01 does not specify this index.

**CP13**: Smoke test dispatches Run with `runtime: "echo"`. If the backend was started with `ENDGOAL_DAEMON_TOKEN=foo` and the daemon was started with a different token (or no token), the daemon WS connection is rejected with 401, no daemon connects, dispatch returns 503, and the smoke test fails on step 6. The smoke test script must set identical tokens in both process environments. Without an explicit token-coordination step in the smoke test documentation, this is a common first-run failure.
