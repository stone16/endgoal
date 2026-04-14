---
task_id: endgoal-kr-001
spec_version: 1
round: 1
---

# Round 1 Spec Review — EndGoal Single-KR Prototype

## Verdict

**revise**

Two checkpoints are too large to complete in a single Generator session (CP07, CP08), and several acceptance criteria are vague enough that two engineers would disagree on PASS/FAIL. Three warnings require resolution before the Generator can execute safely. No blockers that require re-architecting — the approach is sound.

---

## Scope Assessment

### Minimum Viable Scope

The spec is appropriately scoped for a decisive proof-of-concept. The lifecycle arc (prose → freeze → structured → dispatch → stream → review → complete) is the minimal path that validates all three architectural layers simultaneously. Nothing in the checkpoint list is obviously deferrable without defeating the stated goal.

One near-deferral: the scratchpad file listing in CP08 (`GET /api/runs/:id/scratchpad`) is a nice-to-have for the audit trail story but is not required for the smoke test to pass. Consider explicitly marking it as a stretch goal within CP08 rather than a hard acceptance criterion, to protect the checkpoint from scope creep when the filesystem path is non-trivial to expose over HTTP.

### Complexity Score

- Files to create from scratch: ~60-80 (Cargo workspace, 3 crates, 5 frontend feature directories, migrations, bindings). This is high but expected for a greenfield Rust + Next.js monorepo.
- New abstractions introduced: 5 (`RuntimeAdapter` trait, `freeze_sessions` state machine, `WebSocket hub` (two namespaces), `state_at()` pure function, ts-rs generation pipeline). All are justified by architecture decisions; none appear accidental.
- Rust/TS type bridge via ts-rs: one pattern, consistently applied. This is the right call for a Rust/Next.js stack.

### What Already Exists in Codebase

Nothing. The repo contains only `docs/`, `findings.md`, `progress.md`, `task_plan.md`. Every checkpoint builds from scratch.

The reference codebase `~/dev/multica/` exists and provides WS invalidation patterns and feature folder structure. CP06 explicitly borrows from it. The Generator should be directed to read relevant parts of multica before implementing the WS hub and frontend features.

### Prior Art / Approach Soundness

- axum + sqlx + SQLite for Rust MVP: well-established, low-risk.
- ts-rs for type bridge: correct choice; maintained, production-used.
- WS invalidation signal + REST refetch (not WS data push): the right tradeoff for MVP; avoids serialization complexity in the hub.
- SSE for freeze session streaming: correct; SSE is simpler than WS for server-push-only flows.
- `cargo test --features generate-bindings` as CI gate: non-obvious setup but well-documented in ts-rs. Flag as a warning: the Generator needs explicit guidance on how to wire this, as getting it wrong silently (bindings generated but not committed) is a common failure mode.

---

## Checkpoint Review

### Checkpoint 01: Rust workspace scaffold + DB schema + type bindings

- **Granularity**: OK
- **Acceptance Criteria**: TESTABLE — all four criteria are concrete and mechanically verifiable. The `cargo build --workspace` / `cargo test --workspace` / `sqlx migrate run` / presence of `.d.ts` files check is unambiguous.
- **Dependencies**: CORRECT — none required.
- **TDD Readiness**: YES — migrations can be tested with `sqlx::test` macro; ts-rs output can be tested by asserting file existence and a spot-check of a known type field.
- **Notes**: Scope includes "all core Rust types" which is a non-trivial surface. The acceptance criteria wisely narrows to four named types (`Node`, `Run`, `NodeState`, `RunInput`). If the Generator produces partial bindings, this checkpoint may appear to pass while CP06 silently fails because `freeze_sessions`, `AncestorSummary`, etc. are missing. Suggest adding: "bindings contain all types exported from `shared` crate" as an additional criterion, or at minimum note that CP06/CP07 will expose missing types.

### Checkpoint 02: Node CRUD API + phase lifecycle enforcement

- **Granularity**: OK — seven endpoints + phase invariants + two policy operations is at the edge of one session but tractable because the logic is uniform.
- **Acceptance Criteria**: TESTABLE — phase transition tests (Active→Draft = 400, Complete→Active = 400, Draft→Active with prose = 200) are concrete. `effective_policy` intersection is verifiable with a fixture test.
- **Dependencies**: CORRECT — requires 01.
- **TDD Readiness**: YES — phase transitions and policy intersection are pure business logic, ideal for unit tests written before implementation.
- **Notes**: `PATCH /api/nodes/:id` is listed in scope but the acceptance criteria do not specify what fields are patchable or what validation applies. The Generator will need to make assumptions. Suggest adding: "PATCH /api/nodes/:id rejects a `phase` field that would violate invariants (returns 400), but accepts `intent` and `local_policy` updates."

### Checkpoint 03: Daemon + RuntimeAdapter + subprocess spawn + WS protocol

- **Granularity**: OK — the scope is well-bounded: one binary, one trait, two adapters, one WS client, one backend stub. The "echo runtime" testing strategy is a good concreteness anchor.
- **Acceptance Criteria**: TESTABLE — "stream back one RunEvent containing 'hello' and one RunTerminal" is exact. Scratchpad directory existence is checkable.
- **Dependencies**: CORRECT — requires 01 for shared types.
- **TDD Readiness**: YES — subprocess spawn behavior is unit-testable with `echo` as the runtime; WS protocol can be integration-tested with a local server.
- **Notes**: The backend stub in CP03 ("echoes dispatch messages") is a placeholder that will be replaced in CP04. The Generator must not over-engineer this stub. Flag explicitly that CP03's backend stub is throw-away scaffolding.

### Checkpoint 04: Run API + full WebSocket hub + Run lifecycle

- **Granularity**: TOO_LARGE — this checkpoint combines: (a) Run CRUD API (3 endpoints), (b) complete WS hub with two namespaces and broadcast logic, (c) Run status state machine, and (d) four enforcement rules (Active phase, structured acceptance, In-Review gate, 422 error codes). That is 4 distinct subsystems. The hub alone (receiving RunEvents from daemon, persisting to run_events, broadcasting invalidation signals to N frontend clients) has enough failure surface to fill a checkpoint.

  **Split suggestion**:
  - CP04a: Run API (POST/GET endpoints) + Run status state machine + 422 enforcement rules (prose-acceptance = requires_freeze, In-Review = in_review_gate). Depends on 02, 03.
  - CP04b: Complete WS hub (ws/daemon → run_events table → ws/frontend broadcast). Replaces the CP03 backend stub. Depends on CP04a.

  If the split is rejected (to keep checkpoint count down), at minimum the acceptance criteria must be tightened (see below).

- **Acceptance Criteria**: VAGUE on one point — "Frontend WebSocket client receives `{ type: "run:updated" }` signal after each RunEvent arrives" requires a frontend client to exist, but CP06 (frontend) depends on CP04. This creates a circular evaluation dependency: the acceptance criterion cannot be verified until CP06 is complete. Suggest rewording: "A test WebSocket client (in a `cargo test` integration test using `tokio-tungstenite`) receives the invalidation signal." This keeps the criterion in the backend test suite.
- **Dependencies**: CORRECT — 02 and 03 required.
- **TDD Readiness**: PARTIAL — the 422 enforcement is easily TDD'd. The WS hub broadcast is harder to TDD because it involves concurrent connections; recommend specifying that a lightweight in-process test client is sufficient for CP04 testing.

### Checkpoint 05: State Layer — state_at() implementation

- **Granularity**: OK — a pure Rust function with a clear signature and numeric acceptance criteria.
- **Acceptance Criteria**: TESTABLE — the fixture-data assertions (2/3 assertions + 60% metric + 7/10 rubric → 55-70 progress) are concrete. However, the range "55-70" reveals that the combinator weights are not specified in the spec. The Generator will need to choose weights, and the range may not survive the first implementation. Suggest specifying the combinator formula explicitly (e.g., "assertions: 40%, metrics: 40%, rubric: 20%") in the spec rather than leaving it to the Generator.
- **Dependencies**: CORRECT — requires 02 for the Node/acceptance DB schema.
- **TDD Readiness**: YES — pure function, fixture data, exact numeric assertions. Ideal for TDD.
- **Notes**: `next_step` is generated via a live Claude API call. The criterion "may be mocked in tests with a fixture response" is correct but needs to specify the mock mechanism (e.g., a trait bound or a cfg flag). Without this, the Generator may make the Claude API call non-injectable, breaking the unit test. Suggest adding: "The Claude API dependency is injected via a trait or function pointer so tests can stub it without network calls."

### Checkpoint 06: Frontend Layer 0a + Layer 0b + Layer 1

- **Granularity**: TOO_LARGE — three distinct UI layers, a WebSocket provider, SWR/React Query integration, and type-safety enforcement is too much for one session. Each layer (0a, 0b, 1) has non-trivial interaction logic (navigation, side panel open/close, structured acceptance rendering with three sub-components).

  **Split suggestion**:
  - CP06a: Next.js project scaffold + WS provider + Layer 0a (Workspace Overview card grid). Depends on 02, 05.
  - CP06b: Layer 0b (Objective Tree) + Layer 1 (Node Panel with acceptance rendering). Depends on CP06a, 04.

  The spec currently lists "Depends on: 02, 04, 05" — note that CP04 must be complete for the WS invalidation to work. If the checkpoint is split, CP06a can depend only on 02 and 05 (no WS yet, polling fallback) and CP06b picks up 04.

- **Acceptance Criteria**: VAGUE on one point — "Creating a Node via backend API and refreshing (or via WS invalidation) shows new Node in Overview without manual page reload" conflates two different mechanisms. "Refreshing" is not WS invalidation; it is manual. The criterion should separate these: "After Node creation, the Workspace Overview updates within 2 seconds without manual reload (WS invalidation + refetch)." Manual refresh is trivially true and does not test WS.
- **Dependencies**: CORRECT as stated (02, 04, 05), but see split suggestion above.
- **TDD Readiness**: NO — frontend UI rendering is hard to TDD in the strict sense. The criterion `pnpm typecheck passes` is the strongest automated gate available here. Suggest adding a Playwright smoke test as the E2E acceptance criterion for the WS invalidation story: "A Playwright test creates a Node via API, observes the Overview DOM update without page reload."

### Checkpoint 07: Archetype B gate + freeze co-authoring session

- **Granularity**: TOO_LARGE — this is the most complex checkpoint in the spec. It combines: (a) Archetype B modal UI (frontend), (b) freeze_sessions DB table + three backend endpoints (start/respond/commit), (c) SSE streaming from Claude API through backend to browser, (d) conversational chat UI component, (e) session resumability (browser close + reopen), and (f) phase transition on commit. Each of these has independent failure modes.

  **Split suggestion**:
  - CP07a: Archetype B gate modal (frontend) + "Proceed as exploration" dispatch path. Backend changes limited to: accept `type: "exploration"` on the dispatch endpoint. Depends on 04, 06.
  - CP07b: Freeze session backend (freeze_sessions table, POST /freeze/start, POST /freeze/respond with SSE, POST /freeze/commit, state machine tests). Depends on 02, 05. No frontend yet.
  - CP07c: Freeze session frontend (chat UI, session resumability, commit → Node Panel update). Depends on CP07a, CP07b.

  This split adds one checkpoint but dramatically reduces blast radius. If the LLM streaming fails (the highest-risk piece), the failure is isolated to CP07b and does not block CP07a.

- **Acceptance Criteria**: VAGUE — "User types response → agent generates Assertion 2 (or modifies 1 based on feedback)" is vague on what "modifies 1" means and how it is verified. Suggest: "User types response → backend returns next SSE event containing a JSON proposal object with `{ layer: 'assertion', item: {...}, reasoning: string, source_quote: string }`." The shape of the SSE event must be specified here; the Generator will otherwise make free choices that break CP08.
- **Dependencies**: MISSING — CP07 depends on CP06 (for the modal and chat UI), but CP06 is in turn listed as depending on CP04. The chain is CP01 → CP02 → CP04 → CP06 → CP07. This is correct but implicit. The spec should note that CP07's frontend work requires CP06 to be complete.
- **TDD Readiness**: PARTIAL — the backend state machine (pending → proposing_assertions → ... → committed) is TDD-ready. The SSE streaming and frontend are not.

### Checkpoint 08: Layer 2 Run Detail + human review gate + full end-to-end lifecycle

- **Granularity**: TOO_LARGE — four distinct scopes: (a) Layer 2 overlay UI, (b) SSE stream endpoint for run events, (c) scratchpad file listing endpoint + UI, (d) approve/reject endpoints + UI, (e) end-to-end smoke test covering all 8 checkpoints. The smoke test alone merits its own checkpoint.

  **Split suggestion**:
  - CP08a: Layer 2 overlay UI + `GET /api/runs/:id/stream` SSE endpoint. Depends on 04, 06.
  - CP08b: Approve/Reject endpoints + Node Panel In-Review UI (buttons, disabled Trigger Run). Depends on 02, 06.
  - CP08c: End-to-end smoke test: creation → freeze → dispatch (echo) → In-Review → Approve → Complete, with audit trail verification. Depends on all prior checkpoints.

  The scratchpad file listing (`GET /api/runs/:id/scratchpad`) should be explicitly deferred to a stretch goal within CP08a. It requires exposing filesystem paths over HTTP, which has security surface that deserves separate review.

- **Acceptance Criteria**: TESTABLE — the smoke test criteria are concrete: "final Node phase == Complete, final Run status == completed, input_snapshot_json non-empty, run_events ≥1 row." This is the strongest acceptance criterion in the spec. Preserve it exactly.
- **Dependencies**: 04, 05, 06, 07 — CORRECT but dense. This checkpoint cannot start until all prior fullstack work is complete.
- **TDD Readiness**: YES for backend endpoints (approve/reject, SSE stream). NO for Layer 2 UI — same constraint as CP06.

---

## Concerns

**1. severity: critical**
**Details**: CP04's acceptance criterion "Frontend WebSocket client receives invalidation signal" requires a frontend client (CP06) that depends on CP04. This is a circular verification dependency. As written, the Generator cannot verify this criterion without completing CP06 first, which means either the criterion will be skipped or CP06 will be partially built ahead of schedule as a test fixture.
**Suggested fix**: Replace the frontend WS criterion in CP04 with: "A `tokio-tungstenite` test client in `cargo test -p backend -- ws` connects to `ws/frontend`, dispatches a Run via the API, and asserts it receives `{ type: 'run:updated' }` within 2 seconds." This keeps CP04 fully backend-verifiable.

**2. severity: critical**
**Details**: CP07 scopes SSE streaming from Claude API through the backend in the same checkpoint as the frontend chat UI. If Claude API streaming behaves differently than expected (streaming chunking, SSE framing, token-level vs message-level), the Generator will be debugging infrastructure while also building UI. This maximizes time-to-failure-detection.
**Suggested fix**: Split CP07 as described above. CP07b (backend-only SSE from Claude) completes and is verified independently before CP07c (frontend chat UI) begins. The SSE response schema must be specified in the spec (field names, JSON structure) so both halves can be developed against a contract.

**3. severity: warning**
**Details**: The combinator weights for the `state_at()` progress formula are unspecified. The acceptance criterion uses "55-70" as the expected range for a specific fixture, implying the weights exist but are left to the Generator. If the Generator chooses weights that produce 71 or 54, the test fails and the Generator will iterate on the weights rather than the logic.
**Suggested fix**: Specify the formula in the spec, e.g.: `progress = (assertions_pass_rate * 0.4 + metric_achievement * 0.4 + rubric_score_normalized * 0.2) * 100`. Lock the fixture expectation to a single value or a ±2 tolerance.

**4. severity: warning**
**Details**: ts-rs binding generation is triggered by `cargo test --features generate-bindings`. This is a non-standard Cargo feature gate. The Generator must know: (a) how to declare the feature in `Cargo.toml`, (b) which types to annotate with `#[derive(TS)]`, (c) where to configure the output directory (`TS_RS_EXPORT_DIR` env var or `export_to` attribute). None of this is specified. Missing even one `#[derive(TS)]` annotation will silently produce incomplete bindings that pass CP01 but fail in CP06/CP07 when the frontend tries to import a missing type.
**Suggested fix**: Add to CP01 acceptance criteria: "All types in `shared` that are used in API responses or WS messages carry `#[derive(TS, Serialize, Deserialize)]`. A CI step runs `cargo test --features generate-bindings && git diff --exit-code frontend/bindings/` and fails if bindings are stale." Also add a note that `ts-rs` requires the `TS_RS_EXPORT_DIR` env var or `export_to` attribute to be configured.

**5. severity: warning**
**Details**: CP08 includes a scratchpad file listing endpoint (`GET /api/runs/:id/scratchpad`). Exposing arbitrary filesystem paths over an unauthenticated HTTP endpoint (no auth is specified for MVP) creates a local filesystem read primitive. Even for a local-only prototype this is a bad pattern to establish, because the code will persist into a production iteration.
**Suggested fix**: Either (a) explicitly scope the endpoint to read only from `ENDGOAL_SCRATCHPAD_ROOT/{run_id}/` with path traversal protection, or (b) defer the scratchpad file listing to a post-prototype iteration and mark it as out-of-scope for MVP. Option (b) is simpler and does not compromise the smoke test.

**6. severity: warning**
**Details**: CP06 acceptance criteria say "No `any` types for API responses" and "`pnpm typecheck` passes." These are correct quality gates. However, the spec does not specify how the frontend calls the backend API (fetch wrapper, SWR, React Query, etc.). Without a specified data-fetching layer, the Generator will make a free choice, and CP07/CP08 frontend work will need to match that choice. Mismatched choices (SWR in CP06, fetch in CP07) create inconsistency.
**Suggested fix**: Add to CP06 scope: "Establish a typed API client layer (recommend: typed fetch wrapper using bindings/*.d.ts shapes) used consistently across all feature directories."

**7. severity: info**
**Details**: The spec references `multica` and `ccsdk-main` as pattern sources but gives no guidance on which specific files to read. The Generator will likely spend time exploring those repos without a map.
**Suggested fix**: Add a "Reference codebase pointers" section to the spec: e.g., "multica: `features/realtime/ws-provider.tsx` for WS invalidation pattern; `features/nodes/` for feature folder structure." This is a time-saver, not a blocker.

**8. severity: info**
**Details**: Open Question 4 notes that `canonical_artifact` is updated manually via `PATCH /api/nodes/:id/canonical`. This is correct for MVP, but the In-Review → Approve → Complete flow in CP08's smoke test does not mention `canonical_artifact`. The smoke test will pass without ever populating it, which means the `AncestorSummary.canonical_summary` field will always be `None` in any test that runs the full lifecycle.
**Suggested fix**: Either (a) add a step in the smoke test that PATCHes the canonical_artifact before triggering In-Review, or (b) explicitly state that `canonical_summary = None` is acceptable in the MVP smoke test. Either is fine; the current silence will leave the Generator unsure.

---

## Effort Estimate

| Checkpoint | Estimate | Notes |
|---|---|---|
| CP01 | M | Greenfield Rust workspace + all type definitions + ts-rs wiring is non-trivial even if mechanical |
| CP02 | M | 7 endpoints + phase invariants + policy computation; boilerplate-heavy but low conceptual complexity |
| CP03 | M | Daemon WS client + subprocess spawn is moderately complex; `echo` runtime makes testing tractable |
| CP04 | L | Too many subsystems in one checkpoint; if not split, expect 2+ Generator sessions |
| CP05 | M | Pure function with clear signature; LLM call injection is the main complexity |
| CP06 | L | Three UI layers + WS provider + type safety; if not split, expect 2+ Generator sessions |
| CP07 | L | Highest complexity checkpoint; SSE + LLM + UI + session resumability; should be split into 3 |
| CP08 | L | End-to-end smoke test alone is substantial; combined with Layer 2 UI and review gate, expect 2+ sessions |

---

## Failure Modes

**CP01**: ts-rs generates bindings only for the types explicitly annotated `#[derive(TS)]`. If the Generator forgets to annotate a type used in a WS message (e.g., `RunEvent`), `cargo build --workspace` succeeds, `frontend/bindings/` is populated, but CP04/CP06 break because the WS message type has no .d.ts. The missing annotation is invisible until the frontend tries to import it.

**CP02**: `effective_policy` intersection is computed in Rust on every query (no cache). With a deep ancestor chain (5+ levels) and 50+ concurrent API calls during a freeze session, this becomes an N+1 DB query pattern. For MVP single-user this is acceptable, but the function should be written with a single multi-row ancestor query (CTE or iterative) rather than N recursive calls.

**CP03**: `ClaudeCodeAdapter` spawns `claude --workspace <path>`. If the `claude` binary is not on PATH in the daemon's process environment (common in systemd services or non-login shells), the daemon silently fails to spawn the subprocess and sends a `RunTerminal { status: failed }` with no useful error message. The acceptance criteria test only the `echo` runtime, so this failure mode is never exercised in CP03.

**CP04**: The WS hub holds a reference to the daemon connection and N frontend connections in shared state (likely `Arc<Mutex<Hub>>`). If the daemon disconnects mid-Run (crash, restart), the hub must handle the disconnect event and mark in-flight Runs as `failed`. If this is not implemented, Runs remain in `running` status permanently in the DB, and the frontend shows a spinner forever.

**CP05**: `next_step` is generated via a live Claude API call on every `GET /api/nodes/:id/state` request. If the Workspace Overview fetches state for 10 nodes on load, this triggers 10 sequential (or parallel) Claude API calls per page load. For MVP single-user this may be acceptable, but if the API key has rate limits, the Overview will render slowly or partially. Recommend caching `next_step` in the DB with a TTL rather than computing it on every request.

**CP06**: The WS provider connects to `ws/frontend` on mount. In development with Next.js hot reload, the provider unmounts and remounts on every code change, creating a new WS connection each time. Old connections may not be cleaned up server-side. After 20-30 hot reloads, the backend may have 20-30 stale open connections. This is a dev-experience issue, not a production failure, but it will surface during active development of CP07/CP08.

**CP07**: The freeze session's `POST /api/nodes/:id/freeze/respond` triggers a streaming Claude API call and returns SSE. If the user closes the browser mid-proposal, the SSE connection drops. The backend must either: (a) buffer the full Claude response before the SSE connection drops (defeating streaming), or (b) detect client disconnect and cancel the Claude request (requires `reqwest` stream abort). If neither is handled, the backend holds an open Claude API connection for the duration of the model's response window after the user has already navigated away. At $0.003/1k output tokens and a 2000-token freeze proposal, this is $0.006 per abandoned request — trivial for MVP, but the pattern should be documented.

**CP08**: The end-to-end smoke test dispatches a Run using the `echo` runtime. The test then manually calls `POST /api/nodes/:id/review` to set the Node to In-Review. This step is not covered by any earlier checkpoint's acceptance criteria — CP04 enforces the In-Review gate on dispatch but does not expose an endpoint for transitioning a Node *to* In-Review. The smoke test will fail because there is no `/api/nodes/:id/review` endpoint defined in the spec. The spec should explicitly add: "`POST /api/nodes/:id/review` → sets Node phase to In-Review (only from Active, else 400)" to CP04 or CP08's scope.
