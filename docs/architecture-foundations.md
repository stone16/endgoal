# Goal-Managed Agent v6: Architectural Foundations

## Purpose and scope

This document records the foundational architectural decisions locked during the `autoresearch/goal-managed-agent-orchestration-v6` research iteration for the OKR Dashboard system (working title: Goal-Managed Agent v6).

**What this document is:**

- A research artifact capturing architectural decisions, hypotheses, and open questions that emerged from Socratic pressure-testing.
- A filtered handoff document that distinguishes which claims are backed by the real `goal-managed-agent-orchestration-v6` Codex+Claude run, which are backed only by discussion and design reasoning, and which are backed by external 2026 references.
- A handoff input for the actual tech spec, which will be written in a new, separate repository.
- An answer to the question "what has already been decided and why?" for anyone walking into that new repository cold.

**What this document is NOT:**

- The tech spec itself.
- An implementation plan.
- A commitment to build the product inside the current `auto-research` repo. The current repo is the research sandbox; the product will be greenfield.

## Discussion posture

This filtered document weighs four evidence sources:

1. **Stometa's stated requirements** for a goal-native, human-in-the-loop OKR system where agents serve objectives rather than arbitrary tasks.
2. **CEO-mode inversion reflex**: every proposed decision was pressure-tested by asking "what would make this fail?"
3. **Industrial precedent** where applicable, verified via 2026 web searches of production LLM eval frameworks and OKR best practices.
4. **Real v6 run evidence** from `runs/goal-managed-agent-orchestration-v6/`, which confirms some product-direction claims and exposes some judge and runtime failure modes, but does not by itself validate the full future product ontology.

The discussion mode was Socratic: concrete proposals by the assistant, one-question-at-a-time refinement by Stometa, with explicit acknowledgment and correction when the assistant misunderstood.

### Evidence labels used in this revision

- **Run-backed**: supported by the real v6 Codex+Claude run artifacts or score trajectory.
- **Discussion-backed**: selected design choice that was pressure-tested in discussion, but not exercised end-to-end in the current prototype.
- **External-backed**: supported mainly by cited external or product-source evidence.

When a section mixes these, the stronger and weaker claims are called out explicitly instead of being flattened into one "validated" bucket.

---

## 1. The core architectural invariant

> **Any unit of work, whether produced by a human or an agent, must be traceable to a goal (Objective) node. The context required for that work must be derivable from the tree path "work → parent KR → ancestor Objective chain," not assembled by hand each time. Boundaries defined at any level of the tree flow strictly downward and cannot be relaxed by descendants.**

Three chosen design assertions:

1. **Goals are the primary key.** Tasks are derived. There are no orphan tasks. Exploratory work that does not yet fit a goal must be parented to an explicit "Exploratory Objective" rather than floating free.
2. **Context is derivable, not assembled.** When a user opens any work item, the context they see is the concatenation of the intents of its ancestors in the Node tree, not the contents of whatever chat window they used.
3. **Boundaries flow down.** A constraint declared at any Node (budget, tool permission, prompt restriction, stop condition, review requirement) applies to every descendant. Descendants may only tighten; they may not loosen.

This is a data-model commitment, not a style preference. It is discussion-backed and partially external-backed; the v6 run supports the narrower product framing and the importance of explicit boundaries, but it does **not** validate this exact Node model.

### Chosen failure-mode stance

The invariant has a cost: exploratory work that has not yet formed a clear goal is awkward under strict goal-binding. The chosen resolution is **strict invariant with no orphan lane**. Exploratory work is parented to a permanent "Exploratory Objective" so no work is ever orphaned. The alternative (orphan lane with TTL) was rejected because it would make system-wide tracing incomplete, breaking the enterprise audit story that the invariant exists to support.

### Product narrative

The operational metaphor: **agent is the contractor, human is the client. The client may enter the job site at any time and take over, but the client does not lay every brick by hand.**

This is the third posture between Hermes / Claude Code (all agent, user gives up control) and Notion (all human, agent reduced to a narrow helper). The system's contract is: agents handle all work derived from acceptance; humans handle all changes to the contract itself.

---

## 2. Atomic units: Node and Run

The system has two first-class objects and nothing else at the atomic level.

### Node

A **Node is a goal, expressed as a single sentence**. It is user-visible. It is the first-class citizen of the Intent Layer. Nodes are recursive: a Node is simultaneously (a) an Objective to its children and (b) a Key Result to its parent. The OKR tree is a single-primitive recursive structure, not a two-object hierarchy.

**Key property**: a Node can be infinitely expanded. Every Node can spawn children; every child can spawn grandchildren. The depth of expansion is user-driven, depending on the force of intent at any given moment.

### Run

A **Run is one execution attempt against a Node's goal**. It is mostly hidden from users. A single Node can have:

- 0 Runs (pure intent, no execution yet)
- 1 Run (one pass sufficed)
- N Runs in parallel (multi-angle exploration toward consensus)

Runs are implementation detail. Users see Nodes; agents see Runs.

### Not 1:1

**Node and Run are not 1:1**. A Node is intent; a Run is execution. Their cardinality is 1:N. Treating them as equivalent (the "Node = Run recursion" proposal) was considered and rejected because it would expose sub-execution structure to users and contaminate the user-visible OKR tree.

---

## 3. Three-layer model

The system decomposes into exactly three layers:

```
┌─────────────────────────────────────────────────────────────────┐
│  Intent Layer                              user-visible         │
│  Node tree (OKR recursion)                                      │
│  Each Node = intent + acceptance + parent/children              │
│  Humans and agents co-create / review / edit here               │
└───────────┬─────────────────────────────────────────────────────┘
            │ each Node carries 0..N
            ▼
┌─────────────────────────────────────────────────────────────────┐
│  Execution Layer                           mostly hidden        │
│  Run pool + sub-agent concurrency                               │
│  Each Run is one execution attempt                              │
│  Runs isolate their scratchpads; never conflict                 │
└───────────┬─────────────────────────────────────────────────────┘
            │ all Run artifacts feed into
            ▼
┌─────────────────────────────────────────────────────────────────┐
│  State Layer                              on-demand, ephemeral  │
│  "State-from-docs" function                                     │
│  Input:  Node's artifacts + historical records                  │
│  Output: current state / progress / confidence / next_step      │
│  NO long-running agent process                                  │
│  Every request = fresh re-computation                           │
└─────────────────────────────────────────────────────────────────┘
```

### State Layer philosophy: "docs are source of truth"

State does not live in agent processes; state lives in documents. The intended product behavior is that the agent computing state acts like a pure function `f(docs, records) → state`. It can be run at any time, on any historical commit, to answer "what was the state at moment T?"

Intended benefits:

- Immunity to agent drift, deadlock, and crash-loss
- Replayability of State Layer computation
- Alignment with `auto-research`'s existing "git is the ratchet" philosophy
- Reduced stale-cache risk **if** every user-visible status is derived from the same durable record set. The current research loop does not fully satisfy this yet; for example, the real v6 run ended with a stale `loop_status.json` after manual interruption even though `state.json` and `results.tsv` had advanced further.

### State Layer function signature

```
state_at(node, focus_depth, render_layer, rollup_depth) → {
  state,            # this Node's own state
  rollup_state,     # aggregated state walking down to rollup_depth
  rollup_blockers,  # high-severity issues surfaced from descendants
  progress,         # numeric 0-100, based on structured acceptance
  confidence,       # 0-1, the judge's confidence in this state
  next_step         # "if you want to push this Node forward, do X"
}
```

`rollup_depth` and `render_layer` are **independent axes**:

- `rollup_depth`: how deep the State Layer walks into descendants for aggregation (session-scoped user preference)
- `render_layer`: how much of the focused Node to show (0/1/2, determined by user interaction)

---

## 4. Node schema

```
Node {
  id:              uuid
  intent:          string         # single sentence (the Node's essence)
  parent_id:       uuid | null    # null for root
  children:        [uuid]         # this Node's KRs (themselves Nodes)

  acceptance:      acceptance     # prose (draft) or structured (active+)
  local_policy:    policy | null  # optional boundary constraints
                                  # appended at this level only

  runs:            [run_id]       # pointers to Execution Layer
  artifacts:       [artifact]     # produced by Runs or uploaded by humans
  docs:            [doc]          # human-authored notes (not Run output)
  review_log:      [event]        # append-only human review events

  # All fields below are derived by the State Layer.
  # Never hand-authored.
  state:              computed
  progress:           computed
  confidence:         computed
  next_step:          computed
  effective_policy:   computed    # intersect(ancestors.local_policy
                                  #           + self.local_policy)
}
```

Four required fields: `id`, `intent`, `parent_id`, `acceptance`. Without these four, a Node is not legal.

`runs`, `artifacts`, `docs`, `review_log` are **append-only**. This preserves audit history and makes replay possible.

`local_policy` is optional. Nodes without it inherit the ancestor chain directly. This aligns with the "humans only impose constraints where constraints matter" philosophy.

---

## 5. Boundary model

```
effective_policy(node) = intersect(
    ancestors(node).map(n => n.local_policy).flatten()
    + node.local_policy
)
```

**Key properties:**

- `effective_policy` is **derived**, never stored. It is recomputed on every query.
- The `intersect` operation is **monotonic**: adding more constraints can only tighten, never loosen. This is a structural invariant, not enforced by application code.
- Editing any Node's `local_policy` triggers **contract-change reconciliation** for all in-flight Runs dispatched under the previous effective_policy (see Section 10).

---

## 6. Node lifecycle phases

Five phases:

```
  ┌─────────┐
  │  Draft  │   prose acceptance
  └────┬────┘   exploration Runs only
       │        no progress computation (judge-only)
       │
       │ freeze trigger (A / B / C)
       │ + human approves structured acceptance
       ▼
  ┌─────────┐
  │ Active  │   structured acceptance
  └────┬────┘   executional Runs allowed
       │        exploration Runs still allowed
       │        deterministic progress computation
       │
       │ primary Run's agent declares synthesis ready
       ▼
  ┌──────────┐
  │ In-Review│   no new Runs
  └────┬─────┘   artifact state frozen
       │
       ├── human approves ──> ┌──────────┐
       │                       │ Complete │   terminal
       │                       └──────────┘
       │
       └── human rejects ──> back to Active
                              (optional: append
                               tighter local_policy)

  Any phase can be explicitly archived:
  Draft / Active / In-Review / Complete ──> ┌──────────┐
                                             │ Archived │
                                             └──────────┘
```

### Phase invariants

1. **Freeze is one-way.** Draft → Active is legal; Active → Draft is not. Once a Node has structured acceptance, it can only be edited within that structure, never reverted to prose.
2. **Complete is terminal.** No phase → Complete → any other phase. If a user discovers that a "complete" Node is not actually complete, the correct path is to create a new child Node capturing the missing work, not to revert the parent.
3. **In-Review is an execution barrier.** No new Run dispatches are allowed while a Node is In-Review. This guarantees human review is aligned with a frozen artifact snapshot, not a moving target.

### Born-structured children

When a parent Node with structured acceptance auto-spawns children via its acceptance criteria (for example, "complete when children A, B, C are complete"), those children are **born structured**. They enter directly in Active phase with acceptance derived from the parent, skipping Draft entirely.

This implies a cascade rule: a parent must complete its own freeze before it can auto-spawn born-structured children.

### Archived versus Complete

Archived and Complete are both terminal but semantically distinct. **Complete = "did it"**. **Archived = "stopped doing it"**. An archived Node may have been abandoned, superseded, re-scoped, or found to be the wrong goal. Failure is a reason for archiving, not a separate phase. The system records `archived_reason` as a structured field on Archived Nodes.

---

## 7. Acceptance format: hybrid three-layer

Acceptance is **hybrid three-layer**, not prose-only, not structured-only, and not BDD.

```
structured_acceptance = {
  # Layer 1: deterministic assertions
  assertions: [
    { id, kind, text, check_fn }
  ],

  # Layer 2: numeric Key Results (baseline/target/current)
  metrics: [
    { id, name, baseline, target, measurement_fn }
  ],

  # Layer 3: LLM-judge rubric dimensions
  rubric: [
    { id, dimension, description, scale, judge_prompt_ref }
  ],

  # Completion logic combining the three layers
  done_when: {
    all_assertions_pass,
    all_metrics_meet_target,
    rubric_min_score,
    rubric_priority_dim_score,
    require_human_approval
  }
}
```

### Three layers, three questions

- **Assertions answer "did you do it?"** (deterministic, binary)
- **Metrics answer "how much did you do?"** (numeric gradient)
- **Rubric answers "how well did you do it?"** (LLM-judge subjective, calibrated against human gold set)

Each layer is optional. A simple Node may have only two assertions and no metrics or rubric. A complex Node may have all three.

### Why not BDD

Given-When-Then was considered and rejected. 2026 production LLM eval frameworks (Ragas, Langfuse, OpenAI Evals, DeepEval, Galileo) have all abandoned BDD because agent outputs cannot be exhaustively enumerated as predefined scenarios. BDD remains reasonable for traditional software testing and unreasonable for agent evaluation.

### Why hybrid partially matches existing auto-research

This schema is a useful structural analogue to the pattern already running in the `runs/<topic>/` pipeline, but the match is not exact:

| auto-research today | Acceptance schema slot |
|---|---|
| `benchmark.json` items | benchmarked acceptance questions plus must-include constraints; analogous to part of `assertions`, but not executable assertions themselves |
| Persisted LLM-judge quality dimensions with pairwise verdict and `dimension_scores` | `rubric` |
| `overall_score → keep or discard` | heuristic decision gate; analogous to part of `done_when`, but not the full product completion combinator |
| Deterministic evaluator pass rate | future `metrics` slot; not independently exercised in the real v6 run artifacts |

The OKR Dashboard takes the parts of this pattern that are already useful and promotes them into product primitives. The real v6 run clearly validates the rubric layer because non-empty `dimension_scores` were persisted on all 20 judged iterations; it does **not** yet validate an independent metrics layer inside the run artifacts.

### Judge calibration is a first-class concern

Each rubric dimension should be bound to a `calibration_set` (a gold set of human-scored examples). On each judge run, the system should auto-compute correlation against the calibration set. 2026 production targets:

- **Cronbach's alpha > 0.7** (internal consistency across independent runs)
- **Spearman ρ > 0.8** against human expert gold

Dimensions that fall below threshold are automatically downgraded to "requires human review" until recalibrated. The need for this is run-backed: the real v6 run produced 10 persisted invariant artifacts catching judge contradictions, including `verdict_score_mismatch`, `unacknowledged_regressions`, and `dismissal_without_mergeables`. This formalizes the `CalibrationRun` and `GraderTrustScore` concepts drafted in the v5 research iteration.

### Industry sources

Hybrid (deterministic + rubric) is the 2026 production consensus for LLM / agent evaluation. Verified references:

- Hebbia: [Evaluating AI Agents — A Hybrid Deterministic and Rubric-Based Framework](https://www.hebbia.com/blog/evaluating-ai-agents-a-hybrid-deterministic-and-rubric-based-framework)
- Confident AI: [LLM Evaluation Metrics Guide](https://www.confident-ai.com/blog/llm-evaluation-metrics-everything-you-need-for-llm-evaluation)
- Galileo: [Agent Evaluation Framework 2026](https://galileo.ai/blog/agent-evaluation-framework-metrics-rubrics-benchmarks)
- Ragas: [Align an LLM as a Judge](https://docs.ragas.io/en/stable/howtos/applications/align-llm-as-judge/)
- Langfuse: [LLM-as-a-Judge](https://langfuse.com/docs/evaluation/evaluation-methods/llm-as-a-judge)
- DeepEval: [AI Agent Evaluation](https://deepeval.com/guides/guides-ai-agent-evaluation)

OKR numeric-KR orthodoxy:

- Synergita: [OKR Best Practices 2026](https://www.synergita.com/blog/okr-best-practices/)
- Atlassian: [OKRs Ultimate Guide](https://www.atlassian.com/agile/agile-at-scale/okr)

---

## 8. Freeze triggers: A / B / C

Prose acceptance is frozen into structured acceptance via one of three archetype triggers. The system supports all three.

This trigger taxonomy is discussion-backed product design. The current v6 research prototype did not exercise these freeze transitions end-to-end.

### Archetype A: Readiness-based (agent-proposed)

- **Trigger**: agent detects maturity signals (enough docs, stable children, converged discussion, parent freshly frozen).
- **Action**: agent reads prose + docs + parent context and proposes a candidate structured acceptance, item by item, with reasoning. Queues for human co-authoring review.
- **Role**: power path for thoughtful users.

### Archetype B: Action-gated (primary forcing function)

- **Trigger**: first attempt to dispatch an executional Run to a Node with prose acceptance.
- **Action**: system gates the dispatch with three choices: freeze now / proceed as exploration / cancel.
- **Role**: primary forcing function. Guarantees no executional work happens against a prose target without explicit user acknowledgment.

### Archetype C: Evidence-driven (safety net)

- **C1: Run disagreement.** When two or more Runs against the same prose Node produce conflicting interpretations of "done," the system flags the acceptance as under-specified and triggers Archetype A.
- **C2: Sustained activity.** When N or more Run iterations have occurred on a prose Node, the system nudges for freeze.
- **Role**: safety net for Nodes that bypassed A and B.

### Archetype B refusal path: proceed-with-prose + label

When the user at Archetype B says "I just want to run in prose mode," the Run is dispatched but labeled `exploration`. Its artifacts enter `review_log` only; they do not contribute to progress computation. This preserves exploration freedom without violating the State Layer's progress invariant.

**In prose mode, Runs are observations, not executions.**

### Freeze is a co-authoring session, not a form

The freeze flow is a **persistable, resumable, interruptible conversation**, not a one-shot form:

1. Agent reads Node context.
2. Agent proposes assertion 1 with reasoning. User edits / rejects / appends own / skips layer / pauses / resets.
3. Agent proposes assertion 2 with reasoning. Same interaction.
4. (Assertions complete)
5. Agent proposes metric 1 with reasoning.
6. ...
7. Agent proposes rubric 1 with reasoning.
8. ...
9. Agent proposes `done_when` combinator.
10. User commits the full schema; Node transitions Draft → Active.

The session may be paused mid-layer. On resume, the agent re-reads the current partial schema plus the Node's docs (fresh context) and continues from the paused point. Already-confirmed items are not reopened.

### Three hard requirements on freeze sessions

1. **Every proposal must carry reasoning.** An agent cannot offer an assertion in isolation; it must cite the Node docs passage (or the parent context) that motivated it. This mirrors the judge-with-evidence pattern in production LLM eval practice.
2. **User interruption is a first-class action**, not an edge case. The defined interruption actions are: `edit_current_proposal`, `reject_and_retry`, `append_my_own`, `skip_this_layer`, `pause_session`, `reset_to_prose`.
3. **Session state persists.** On pause, the partial schema is saved. On resume, the agent re-reads context fresh. Already-confirmed items do not reopen.

---

## 9. Fork-Return-Synthesize (multi-Run merging)

When a Node needs multi-angle execution, its primary Run dispatches parallel sub-Runs under the Claude Code sub-agent pattern:

```
[Node "validate X"]
   │
   ├─ Run P (primary / dispatcher)
   │    │
   │    │ parallel dispatch of 3 sub-Runs, isolated scratchpads
   │    │
   │    ├── sub-Run A (angle: "how approach A works")
   │    │     own scratchpad: sub-A/
   │    │     returns: {findings, concerns, confidence,
   │    │               needs_human_review}
   │    │
   │    ├── sub-Run B (angle: "how approach B works")
   │    │     own scratchpad: sub-B/
   │    │     returns: {...}
   │    │
   │    └── sub-Run C (angle: "first principles")
   │          own scratchpad: sub-C/
   │          returns: {...}
   │
   │ Run P reads A/B/C structured returns
   │ Run P uses its own agent reasoning to synthesize
   │ Run P decides which needs_human_review flags to bubble up
   │
   ▼
Node's primary artifact (the only thing the user sees by default)
```

### Five design decisions

1. **Scratchpad isolation.** Sub-Runs write only to their own directories. Physical isolation means no git conflicts, no merge algorithm needed.
2. **Structured returns, not prose blobs.** Parent agents read typed records and apply deterministic reasoning, not document parsing.
3. **Synthesis is agent reasoning**, not a merge algorithm. The parent Run's agent decides how to combine sub-Run returns; the system does not impose a merge function.
4. **Sub-Runs are terminal.** They run to completion, return, and die. All state lives in written scratchpad files.
5. **Sub-Runs are invisible by default.** Only the parent Run's synthesis is visible on the dashboard. Users may drill down into sub-Run scratchpads but must actively choose to.

### Synthesis audit

Every synthesis step writes a **synthesis reasoning log** describing what sub-Run outputs were read, what was kept, what was merged, what was discarded, and why. This log is part of the Node's audit artifacts.

For `high_stakes` Nodes (set via `local_policy`), synthesis completion forces human review before the Node can advance. This is the direct application of boundary-flows-down to synthesis gating.

---

## 10. Run dispatch contract

### Five Run types

| Type | Purpose | Can modify canonical? | Allowed phase |
|---|---|---|---|
| `research_iteration` | New knowledge_base + benchmark answers | Yes | Active |
| `synthesis` | Merge sub-Run artifacts into primary artifact | Yes | Active |
| `audit` | Read-only review, produces review report | No | Any |
| `exploration` | Observational work under prose acceptance | No | Draft / Active |
| `reconcile` | Response to contract change | No | Active (with in-flight Run) |

The can-modify-canonical column is the hardest invariant. Run types that cannot modify canonical are prevented from doing so at the data layer, not by agent prompt discipline.

### Run input

Frozen at dispatch time:

```
Run.input = {
  run_id, node_id, parent_run_id, type,

  intent_snapshot,        # Node.intent at dispatch time (deep copy)
  acceptance_snapshot,    # Node.acceptance at dispatch time
                          # null for exploration / audit / reconcile
  effective_policy,       # computed at dispatch time, frozen
  parent_context,         # read-only snapshot of ancestor summaries

  angle,                  # non-null for parallel sub-Runs
                          # null for primary Run
  scratchpad_path,        # exclusive write directory
  budget: { tokens_max, iterations_max, wallclock_max_s },
  tool_permissions,       # subset of effective_policy, type-gated
  runtime                 # "codex" | "claude" | "cli" | ...
}
```

All input fields are **deep snapshots, not pointers**. This is essential for auditability: opening a historical Run must reveal exactly what contract it was working against.

### Run output

Populated only when Run enters a terminal state:

```
Run.output = {
  findings,              # synthesis-facing summary
  concerns,              # list of flagged items for parent attention
  confidence,            # 0-1 self-confidence score
  needs_human_review,    # bool: agent's escalation flag

  artifacts,             # artifact file pointers in scratchpad
  canonical_updates,     # proposed changes to Node canonical artifact
                         # gated: only research_iteration and synthesis
                         # may populate this

  spawned_sub_runs,      # run_ids of sub-Runs this Run dispatched
  termination_reason,    # success / budget_exhausted / aborted / error
  audit_trail            # append-only event log with reasoning
}
```

### Three Run-output invariants

1. **`canonical_updates` is Run-type-gated at the data layer.** Only `research_iteration` and `synthesis` may populate it. Others are rejected by the schema. This is how Task #11's observational-vs-executional distinction becomes a structural invariant rather than a prompt-level convention.
2. **`findings / concerns / confidence / needs_human_review` is the parent-readable contract.** Parent Runs synthesize based on these four fields. Raw artifacts are available for drill-down but are not part of the synthesis reading set.
3. **`audit_trail` is append-only during the Run**, and frozen once the Run terminates. Every tool call and reasoning decision appends one entry with timestamp and rationale.

### Terminal states

```
dispatched → running → completed | budget_exhausted | aborted | failed
```

Each terminal state is handled differently during parent synthesis:

- **`completed`**: parent reads output normally.
- **`budget_exhausted`**: parent may dispatch a replacement Run with a larger budget.
- **`aborted`**: parent executes reconcile recommendations.
- **`failed`**: parent may spawn a replacement Run with the failed audit_trail as context.

---

## 11. Contract change reconciliation

Editing **any** contract field (intent, acceptance, local_policy) on a Node with in-flight Runs triggers the reconcile protocol:

1. The system notifies all in-flight Runs that their contract snapshot is stale.
2. Each Run's agent re-evaluates the new contract against its current state and decides: continue / abort / restart.
3. The Run's decision is logged to `audit_trail`.
4. The parent Run (if any) is informed so its synthesis can account for the change.

This generalizes from the original "acceptance-change" use case to cover **all** contract fields. One protocol handles acceptance edits, policy edits, and any future contract-level field.

This is a discussion-backed design choice, not something the v6 prototype validated directly.

### Why not the alternatives

- **Hard block** (no editing during in-flight Runs): violates the "human always in control" philosophy.
- **Soft fork** (auto-version Nodes on contract change): bloats the OKR tree into a version tree.
- **Agent reconcile**: makes the agent do what agents are for, reason about contract changes in context.

---

## 12. Escalation mechanism

Human review escalation is **agent-driven, not structural**.

- A Run's agent decides whether its findings warrant human review by setting `needs_human_review = true` in its structured return.
- When a parent Run reads a child's output, the parent's agent decides whether to propagate the flag further up, suppress it, or handle it locally.
- The flag only becomes user-visible when some agent in the ancestor chain actively promotes it to the Node level.

This avoids hard-coded "materially affects parent" promotion rules and keeps sub-Run structural visibility at zero. The tech spec will define:

1. **Return schema** with the `needs_human_review` field and supporting signal fields.
2. **Parent agent promotion guidance**: prompt-level instructions for how an agent should decide whether to bubble, suppress, merge, or locally-resolve flags from its children.

Escalation is a prompt-engineered behavior, not a data-model primitive.

Treat this as a chosen simplification, not as a run-validated fact.

---

## 13. UX philosophy: agent-default vs human-default

All atomic actions on a Node are **legitimate**. What varies is **the default executor** and **when they are surfaced** (progressive disclosure; see Section 14).

### Agent-default actions

Agent does these automatically; humans intervene only when the agent errs:

- Triggering new Runs
- Multi-Run synthesis
- Bubbling or suppressing `needs_human_review` flags
- Spawning implicit sub-Runs during exploration

### Human-default actions

Humans must act; agents may propose but cannot execute:

- Editing `intent` (any Run's target depends on it)
- Editing `acceptance` (triggers reconcile)
- Editing `local_policy` (the boundary interface)
- Approving or rejecting synthesis at In-Review
- Archiving or deleting a Node

### Agent-proposes, human-executes

Collaborative actions where agent suggests and human commits:

- Creating explicit child Nodes (agent may suggest in `next_step`)
- Manually merging sibling Run artifacts (override path when the agent's synthesis is wrong)

This two-way classification is the operational concretization of the "agent is contractor, human is client" metaphor.

---

## 14. Progressive disclosure: three render layers

The system has **three render layers** for any focused Node, and they are independent of `rollup_depth`.

This section is discussion-backed UI architecture. The v6 run supports the importance of preview, approval, blocked-state, and evidence-review surfaces, but it does not validate this exact three-layer rendering model.

```
Layer 0: Card            — scanning view, tree/list
Layer 1: Panel           — focused view, primary workspace for one Node
Layer 2: Detail overlay  — drill-down into a specific Run or audit trail
```

### Layer 0: Card

**Shows**: `intent`, `state`, `rollup_state`, `progress`, `rollup_blockers_count`, `needs_human_review` flag, lifecycle phase icon.

**Actions**: click to focus, expand or collapse children, reorder siblings.

### Layer 1: Panel

**Shows**: Layer 0 fields + `acceptance`, `next_step`, `confidence`, `effective_policy` summary, recent `review_log`, recent Run summaries (type, state, findings snippet), children as Layer 0 cards.

**Actions**: all human-default actions (edit intent / acceptance / local_policy, archive, add child, trigger Run, approve or reject In-Review synthesis, clear review flags, add human note). Click any Run summary → Layer 2.

### Layer 2: Detail overlay

**Shows**: full `Run.input` snapshot, full `Run.output`, `audit_trail` timeline, `spawned_sub_runs` tree, scratchpad file browser (read-only), side-by-side comparison with other Runs on the same Node.

**Actions**: export Run artifact, tag Run as reference, dispatch reconcile Run, return to Layer 1.

**Hard rule**: Layer 2 never edits contract. Contract edits must return to Layer 1.

### Three render-layer invariants

1. **Layer 0 reads only State Layer rollup output**, never Run artifacts directly. This is what makes rendering a 1000+ Node tree responsive.
2. **Layer 1 is the single contract-change surface.** Layer 0 navigates; Layer 2 observes. Contract edits must pass through Layer 1. This centralizes the audit surface for reconcile-trigger logic.
3. **Layer 2 must enable full execution replay.** `Run.input` snapshot + `audit_trail` + scratchpad files together must be sufficient to reconstruct, replay, or re-review the execution.

### Rollup depth is session-scoped

`rollup_depth` is a user preference, not a Node property. Different users have different aggregation preferences. The State Layer API accepts `rollup_depth` as a parameter; the front end passes the user's current session preference.

### Singleton Layer 1

A single dashboard pane can have only **one** Node in Layer 1 at a time. Opening a new Node's Layer 1 closes the previous one. Users who want to compare Nodes side-by-side must use an explicit multi-pane feature, not rely on modal state.

---

## 15. Open questions and known risks

These were flagged during discussion but not fully resolved. They should be revisited during tech spec drafting.

1. **Synthesis quality depends on parent agent reasoning.** If the parent Run's agent is weak, synthesis errors can silently discard load-bearing sub-Run findings. Mitigations: mandatory synthesis audit logs, and `high_stakes` local_policy forcing human review before advance.
2. **Judge calibration drift.** Rubric LLM judges can drift over time as models change or context shifts. The tech spec should specify how often calibration correlation is recomputed and what action is taken on drift.
   The real v6 run strengthens this concern rather than resolving it: 10 invariant artifacts were emitted under real Claude judging, including 6 `verdict_score_mismatch` hits.
3. **Budget semantics during reconcile.** When a contract change triggers reconcile, what happens to already-consumed budget? Not yet decided.
4. **Multi-user concurrency.** The current model assumes one user per Node at a time. Team use (multiple humans editing the same Node) requires CRDT-style coordination or pessimistic locking. Scoped out of the foundational decisions but load-bearing for team deployments.
5. **Cross-goal knowledge reuse.** The v5 draft proposed `KnowledgeArtifact` and `CrossGoalLink` primitives. Not reopened in this discussion but remain in scope for the tech spec.
6. **`measurement_fn` execution environment.** Structured acceptance's `measurement_fn` field needs an execution runtime (sandboxed Python? LLM call? SQL query?). At least two types should be supported in MVP.
7. **Prototype-proof gap.** The real v6 run improved the research score from the v5 best of `0.89` to a v6 best of `0.90`, but that is still evidence about research synthesis quality, not proof that the proposed Node/Run product model works. The next decisive proof remains one instrumented KR prototype that survives clarification, approval, launch, denial, replan, resume, verification, and acceptance without state ambiguity.

---

## 16. Mapping to the existing auto-research system

The `auto-research` repo is the research prototype, not the product. But its current pipeline is the field-tested prototype for several product primitives:

| auto-research today | Product primitive |
|---|---|
| `runs/<topic>/topic.md` | research topic seed; analogous to an Objective statement |
| `topic.md` quality dimensions | Rubric dimensions |
| `benchmark.json` items | benchmark questions and must-include constraints; partial analogue to structured acceptance, not executable assertions |
| Persisted judge `dimension_scores` and pairwise verdict | Rubric grading |
| Deterministic evaluator pass rate | Future metrics slot; not independently present in the real v6 run artifacts |
| `runs/<topic>/judge_feedback.md` | review-log analogue |
| `runs/<topic>/human_feedback.md` | human notes and steering context, not yet a proven `local_policy` equivalent |
| Iteration keep/discard + git ratchet | partial Run provenance and retained-best ratchet |
| v5 draft `CalibrationRun`, `GraderTrustScore` | conceptual precursor for judge calibration subsystem |
| v5 draft `BudgetPolicy`, `CostLedgerEntry` | conceptual precursor for budget-related policy fields |
| v5 draft `NotificationRoute` | conceptual precursor for escalation routing |
| v5 draft `KnowledgeArtifact`, `CrossGoalLink` | Open question #5 |
| v5 draft `ExternalActionContract` | conceptual precursor for tool permissions in `effective_policy` |

The OKR Dashboard product does **not discard** this research. It promotes the patterns that survived both discussion and real-run scrutiny into product primitives in a new repo. The real research scores are: v5 best `0.89`, v6 best `0.90`. That is evidence of content convergence and slightly better synthesis, not proof that the full product ontology is already validated.

---

## 17. Next steps

1. **Keep the evidence classes explicit.** Before each tech-spec decision, mark it as run-backed, discussion-backed, external-backed, or still-open. Do not collapse these into one bucket called "validated."
2. **Build one instrumented KR prototype.** The next decisive proof is not more prose. It is one KR that survives clarification, approval, governed launch, denial, replan, resume, verification, and acceptance with replayable state and auditability.
3. **Red-team the architecture.** Run CEO-mode inversion reflex across every locked decision: "what breaks first under adversarial conditions?" Catch failure modes not surfaced during Socratic validation.
4. **Draft tech spec skeleton.** Using this document as input, write the tech spec table of contents in the new (yet-to-be-created) product repo.
5. **Seed the new repo.** Once the skeleton is approved, stand up the product repo with initial schemas and the first implementation milestone.

This document is the filtered handoff artifact for the prototype-proof, red-team, and tech-spec drafting stages.

---

## Locked decisions: index

The table below is intentionally filtered. "Locked" here means "current preferred foundation for tech-spec drafting," not "already validated in a working product."

| # | Decision | Section | Current evidence status |
|---|---|---|---|
| 1 | Core architectural invariant: goal-binding | §1 | Discussion-backed, external-backed |
| 2 | Atomic units: Node (intent) and Run (execution), not 1:1 | §2 | Discussion-backed |
| 3 | Three-layer model: Intent / Execution / State | §3 | Discussion-backed, run-inspired |
| 4 | Node schema with 4 required fields | §4 | Discussion-backed |
| 5 | Boundary: derived + local_policy, monotonic intersect | §5 | Discussion-backed |
| 6 | Node lifecycle: 5 phases with 3 invariants | §6 | Discussion-backed, external-backed |
| 7 | Acceptance format: hybrid three-layer (assertions + metrics + rubric), BDD excluded | §7 | Partially run-backed (rubric layer), discussion-backed |
| 8 | Freeze triggers: A readiness, B action-gated (primary), C evidence-driven | §8 | Discussion-backed |
| 9 | Archetype B refusal: proceed-with-prose + `exploration` label | §8 | Discussion-backed |
| 10 | Freeze is a co-authoring session with persistent state | §8 | Discussion-backed, external-backed |
| 11 | Fork-Return-Synthesize with structured returns and scratchpad isolation | §9 | Discussion-backed, external-backed |
| 12 | Run dispatch contract: 5 types, immutable input snapshot, type-gated `canonical_updates` | §10 | Discussion-backed |
| 13 | Contract-change reconcile protocol (generalized across all contract fields) | §11 | Discussion-backed |
| 14 | Escalation is agent-driven, not structural | §12 | Discussion-backed |
| 15 | Progressive disclosure: 3 render layers, Layer 1 is the single contract-change surface | §14 | Discussion-backed, external-backed |

---

## Document metadata

- **Discussion branch**: `autoresearch/goal-managed-agent-orchestration-v6`
- **Discussion mode**: Socratic with CEO-mode pressure-testing (Brainstorming skill + plan-ceo-review skill)
- **External-research-backed where cited**: 2026 searches of Hebbia, Confident AI, Ragas, Langfuse, DeepEval, Galileo, Synergita OKR guide, Atlassian OKR guide
- **Research inheritance**: this document captures decisions and hypotheses that emerged during the v1-v6 auto-research iterations on the goal-managed-agent framework, then filters them by evidence strength instead of treating them as equally validated
- **Status**: filtered foundations; mix of run-backed conclusions, discussion-backed design choices, and explicit open questions; ready for prototype proof, red-team, and tech spec drafting
