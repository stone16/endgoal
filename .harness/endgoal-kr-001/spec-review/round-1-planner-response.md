---
task_id: endgoal-kr-001
round: 1
---

## Accepted Changes

**Critical fixes (both accepted):**

1. **CP04 circular WS criterion** → Replaced "frontend WS client receives signal" with `tokio-tungstenite` test client in `cargo test -p backend -- ws`. Criterion stays fully backend-verifiable. Also added `POST /api/nodes/:id/review` endpoint (surfaced in CP08 failure mode analysis) to CP04 scope.

2. **CP07 SSE + UI in one checkpoint** → Split into 3 checkpoints: CP09 (Archetype B modal + exploration path), CP10 (freeze session backend: state machine + SSE streaming from Claude API), CP11 (freeze session frontend: chat UI + commit flow). SSE event schema now specified in spec (field names: `layer`, `item_json`, `reasoning`, `source_quote`).

**Warning fixes (all accepted):**

3. **CP05 combinator weights** → Formula locked: `progress = (assertions_pass_rate × 0.4 + metric_achievement × 0.4 + rubric_score_normalized × 0.2) × 100`. Fixture expectation tightened to single value ±2: 2/3 assertions (67%) × 0.4 + 60% metric × 0.4 + 70% rubric (7/10) × 0.2 = `26.8 + 24 + 14 = 64.8` → expected range 63-67.

4. **ts-rs annotation coverage** → Added to CP01 acceptance criteria: all types in `shared` used in API responses or WS messages carry `#[derive(TS, Serialize, Deserialize)]`. CI step added: `cargo test --features generate-bindings && git diff --exit-code frontend/bindings/`.

5. **Scratchpad file listing security** → Deferred to post-prototype. Removed from CP08 scope. Added to Out of Scope section.

6. **API client layer unspecified** → Added to CP07 (former CP06) scope: typed fetch wrapper (`lib/api.ts`) established before Layer 0a implementation. All feature directories must use this wrapper; no ad-hoc fetch calls.

**Info fixes (accepted):**

7. **Reference codebase pointers** → Added to Technical Approach: specific file references for Generator (multica WS provider, feature folder structure).

8. **canonical_artifact smoke test ambiguity** → Explicitly stated in CP13 smoke test: `canonical_summary = None` is acceptable for MVP; step that PATCHes `canonical_artifact` is optional stretch goal.

**Checkpoint splits (all accepted):**

- Old CP04 → New CP04 (Run API + enforcement + review endpoint) + New CP05 (WS hub)
- Old CP05 → New CP06 (State Layer, renumbered)
- Old CP06 → New CP07 (scaffold + WS + Layer 0a) + New CP08 (Layer 0b + Layer 1)
- Old CP07 → New CP09 (Archetype B modal) + New CP10 (freeze backend) + New CP11 (freeze frontend)
- Old CP08 → New CP12 (Layer 2 + SSE) + New CP13 (review gate + smoke test)

**Additional fixes from failure mode analysis:**

- CP02 PATCH clarification added: PATCH accepts `intent`, `local_policy` (append-only tightening); rejects `phase` field (400).
- CP05 (old) / CP06 (new) `next_step` caching: `next_step` computed once when canonical_artifact changes and cached in DB column; not recomputed on every `GET /api/nodes/:id/state` request.
- CP04 (new) WS hub daemon disconnect: specified that daemon disconnect must mark in-flight Runs as `failed`.

## Rejected Changes

None. All evaluator suggestions accepted.

## Spec Updated To

Version 2 (13 checkpoints, all concerns addressed)
