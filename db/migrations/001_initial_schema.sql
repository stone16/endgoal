-- Initial schema for EndGoal
-- Tables: nodes, node_docs, review_log, runs, run_events, freeze_sessions

CREATE TABLE nodes (
    id TEXT PRIMARY KEY NOT NULL,
    intent TEXT NOT NULL,
    parent_id TEXT REFERENCES nodes(id),
    phase TEXT NOT NULL DEFAULT 'draft',
    acceptance_json TEXT NOT NULL DEFAULT '{"type":"prose","text":""}',
    local_policy_json TEXT,
    canonical_artifact_text TEXT,
    canonical_updated_by_run_id TEXT,
    next_step_cache TEXT,
    next_step_cache_for_run_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE node_docs (
    id TEXT PRIMARY KEY NOT NULL,
    node_id TEXT NOT NULL REFERENCES nodes(id),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE review_log (
    id TEXT PRIMARY KEY NOT NULL,
    node_id TEXT NOT NULL REFERENCES nodes(id),
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    details_json TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE runs (
    id TEXT PRIMARY KEY NOT NULL,
    node_id TEXT NOT NULL REFERENCES nodes(id),
    type TEXT NOT NULL,
    status TEXT NOT NULL,
    runtime TEXT NOT NULL,
    input_snapshot_json TEXT,
    output_json TEXT,
    scratchpad_path TEXT,
    started_at TEXT,
    ended_at TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE run_events (
    id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id),
    seq INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    data_text TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_run_events_run_seq ON run_events(run_id, seq);

CREATE TABLE freeze_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    node_id TEXT NOT NULL REFERENCES nodes(id),
    approved_items_json TEXT NOT NULL DEFAULT '[]',
    current_layer TEXT NOT NULL DEFAULT 'assertions',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT,
    updated_at TEXT
);
