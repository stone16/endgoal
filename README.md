# EndGoal

**Don't manage tasks. Manage end goals.**

EndGoal is a goal-native workspace where humans set objectives and AI agents pursue them. You define what "done" looks like. Agents figure out how to get there. You review, steer, and approve.

This is not a task tracker with AI bolted on. The goal is the primary object. Everything else, every task, every agent run, every artifact, derives from it and traces back to it.

## How it works

```
You set an Objective
    |
    |  EndGoal breaks it into measurable Key Results
    |  (with your approval at every step)
    |
    v
Each KR becomes a node in a goal tree
    |
    |  AI agents execute toward each node
    |  in parallel, from multiple angles
    |
    v
You monitor progress from a calm dashboard
    |
    |  The system notifies you only when
    |  your attention is needed
    |
    v
You approve, reject, or redirect
    |
    |  Agents adapt and continue
    |
    v
Goal achieved. Artifacts preserved. Fully auditable.
```

## Core ideas

**Goal-binding invariant.** Every unit of work must trace to an Objective. No orphan tasks. Context is derived from the goal tree, not assembled by hand. Boundaries flow down the tree and cannot be loosened by descendants.

**Node and Run.** A Node is a goal (one sentence). A Run is one execution attempt against that goal. Nodes are what users see. Runs are what agents do. One Node can have many Runs, exploring the goal from different angles until consensus.

**Human as client, agent as contractor.** Agents handle all work derived from acceptance criteria. Humans handle all changes to the contract itself: editing goals, setting boundaries, approving results. The client can enter the job site at any time, but the client does not lay every brick.

**Calm by default.** The dashboard is intentionally sparse when agents are working well. Notifications surface only when attention is needed. Empty space means "all is well," not "nothing is happening."

## Architecture

Four progressive disclosure levels, from portfolio scanning to forensic audit:

| Level | View | Unit | Core question |
|-------|------|------|---------------|
| 0 | Workspace overview | All Objectives | "What should I look at?" |
| 1 | Objective tree | One goal + descendants | "What's the structure?" |
| 2 | Node panel | One node | "What should I change?" |
| 3 | Run detail | One execution | "What happened?" |

Contract edits (changing goals, acceptance criteria, policies) happen at exactly one level (Level 2). Every other level is read-only or navigation-only.

For the full architectural specification, see [`docs/architecture-foundations.md`](docs/architecture-foundations.md).

For interactive wireframes showing each level, open [`docs/wireframes.html`](docs/wireframes.html) in a browser.

## Local Development

Requirements:

- Rust stable 1.85 or newer for edition 2024 crates
- pnpm for the Next.js frontend
- SQLite via `sqlx`

Create local environment defaults:

```bash
cp .env.example .env
```

Run database migrations:

```bash
DATABASE_URL=sqlite://endgoal.db?mode=rwc sqlx migrate run --source db/migrations
```

Starting the backend also applies pending migrations:

```bash
DATABASE_URL=sqlite://endgoal.db?mode=rwc cargo run -p endgoal-backend
```

Regenerate TypeScript bindings:

```bash
cargo test -p endgoal-shared --features generate-bindings export_all_bindings
```

The workspace Cargo config sets `TS_RS_EXPORT_DIR` to `frontend/bindings`, so the bindings export test writes to the frontend without requiring a per-command env var.

## Status

EndGoal is in the **architectural design** phase. The 15 foundational decisions are locked. Interactive wireframes exist. No production code yet.

### What's done

- [x] Core invariant: goal-binding (all work traces to an Objective)
- [x] Data model: Node (intent) + Run (execution), 1:N relationship
- [x] Three-layer architecture: Intent / Execution / State
- [x] Acceptance format: hybrid (assertions + metrics + LLM-judge rubric)
- [x] Node lifecycle: 5 phases (Draft / Active / In-Review / Complete / Archived)
- [x] Multi-Run merge: Fork-Return-Synthesize with isolated scratchpads
- [x] Boundary model: derived policy with monotonic intersection
- [x] Run dispatch contract: 5 typed Runs with immutable input snapshots
- [x] Progressive disclosure: 4 levels with action-first Node panel
- [x] Interactive wireframes for all views

### What's next

- [ ] Tech spec: detailed implementation design
- [ ] Choose stack (likely TypeScript + Next.js or Go + HTMX)
- [ ] Node CRUD API
- [ ] State Layer: recomputation engine
- [ ] Agent runtime: Run dispatcher + structured return parser
- [ ] Dashboard frontend
- [ ] Freeze co-authoring session (agent proposes structured acceptance step by step)
- [ ] Judge calibration subsystem

## Research lineage

EndGoal's architecture was validated through 5 iterations of auto-research loops (producer/judge cycles with Codex and Claude) plus Socratic design sessions with CEO-mode pressure-testing. The research repo is at [stone16/auto-research](https://github.com/stone16/auto-research).

Key industrial references that informed the design:
- [Hebbia: Hybrid Deterministic and Rubric-Based Agent Evaluation](https://www.hebbia.com/blog/evaluating-ai-agents-a-hybrid-deterministic-and-rubric-based-framework)
- [Confident AI: LLM Evaluation Metrics](https://www.confident-ai.com/blog/llm-evaluation-metrics-everything-you-need-for-llm-evaluation)
- [Ragas: LLM Judge Alignment](https://docs.ragas.io/en/stable/howtos/applications/align-llm-as-judge/)

## License

MIT
