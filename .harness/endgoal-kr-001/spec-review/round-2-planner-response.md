---
task_id: endgoal-kr-001
round: 2
---

## Accepted Changes

**Warning 1 — smoke test daemon coordination (accepted):**
- Added `GET /api/health → 200` endpoint to backend (CP13 scope)
- Locked smoke test form to `scripts/e2e-smoke.sh`; removed "or cargo test" alternative
- Specified: script starts backend + daemon as background processes with matching `ENDGOAL_DAEMON_TOKEN`, polls `/health` until 200 (max 10s), runs curl assertions, traps EXIT to terminate both processes

**Warning 2 — freeze session URL undefined (accepted):**
- CP11 scope updated: freeze session is a full-page route at `app/nodes/[id]/freeze/page.tsx`
- Archetype B modal "Freeze now" now calls `router.push('/nodes/${id}/freeze')` (not overlay)
- On mount: calls `GET /api/nodes/:id/freeze/active` to detect resume vs. start
- Redirect to Node Panel if no active session and acceptance already Structured

**Info 3 — CP10 Claude API stub ambiguity (accepted):**
- CP10 acceptance criterion updated: "stub Claude API injected via trait/function pointer — no network call in cargo test" (removed "real or stub")

**Info 4 — next_step cache invalidation column missing (accepted):**
- Added `next_step_cache_for_run_id TEXT` column to DB schema (nodes table)
- Updated backend Technical Approach: cache regenerated when `canonical_updated_by_run_id != next_step_cache_for_run_id`; writes `next_step_cache_for_run_id = canonical_updated_by_run_id` after regeneration

**Latent issue from CP11 failure mode — GET /active endpoint missing (accepted):**
- Added `GET /api/nodes/:id/freeze/active` to CP10 scope and acceptance criteria
- CP10 acceptance criteria now verifies: returns `null` when no session, returns session data when active

**Performance fix from CP12 failure mode (accepted):**
- Added `CREATE INDEX idx_run_events_run_seq ON run_events(run_id, seq)` to DB schema section (CP01 migrations)

## Rejected Changes

None.

## Spec Updated To

Version 3 — status: **approved**

Spec is ready for Session 2 execution. Generator may begin from CP01.
