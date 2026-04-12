use axum::{
    Json, Router,
    extract::{Path, Query, State, WebSocketUpgrade, ws},
    http::StatusCode,
    response::IntoResponse,
    routing::{any, get, patch, post},
};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use std::sync::{Arc, RwLock};
use tower_http::cors::CorsLayer;

use crate::errors::AppError;
use crate::hub::Hub;
use crate::llm::{LlmClient, create_llm_client};
use crate::shared::types::{
    Acceptance, AncestorSummary, Node, NodeState, Phase, Policy, Run, RunDispatch, RunInput,
    WsDaemonMessage,
};

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct AppState {
    pub pool: SqlitePool,
    pub hub: Arc<RwLock<Hub>>,
    pub llm: Arc<dyn LlmClient>,
}

// ---------------------------------------------------------------------------
// Request/Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateNodeRequest {
    pub intent: String,
    pub parent_id: Option<String>,
    pub acceptance_json: Option<String>,
    pub local_policy_json: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchNodeRequest {
    pub intent: Option<String>,
    pub local_policy: Option<String>,
    /// Rejected if present — phase changes happen via dedicated endpoints.
    pub phase: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DispatchRunRequest {
    #[serde(rename = "type")]
    pub run_type: String,
    pub runtime: String,
}

#[derive(Debug, Deserialize)]
pub struct RejectNodeRequest {
    pub tighter_policy: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct StateQueryParams {
    pub rollup_depth: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyResponse {
    pub tokens_max: Option<u64>,
    pub iterations_max: Option<u64>,
    pub wallclock_max_s: Option<u64>,
    pub allowed_tools: Option<Vec<String>>,
    pub review_required: Option<bool>,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn create_router(pool: SqlitePool) -> Router {
    let hub = Arc::new(RwLock::new(Hub::new()));
    let llm: Arc<dyn LlmClient> = Arc::from(create_llm_client());
    let state = Arc::new(AppState { pool, hub, llm });

    Router::new()
        .route("/api/nodes", get(list_nodes).post(create_node))
        .route("/api/nodes/{id}", get(get_node).patch(patch_node).delete(delete_node))
        .route("/api/nodes/{id}/children", get(get_children))
        .route("/api/nodes/{id}/ancestors", get(get_ancestors))
        .route("/api/nodes/{id}/effective-policy", get(get_effective_policy))
        .route("/api/nodes/{id}/state", get(get_node_state))
        .route("/api/nodes/{id}/activate", post(activate_node))
        .route("/api/nodes/{id}/review", post(review_node))
        .route("/api/nodes/{id}/runs", get(list_runs).post(dispatch_run))
        .route("/api/nodes/{id}/approve", post(approve_node))
        .route("/api/nodes/{id}/reject", post(reject_node))
        .route("/api/runs/{id}", get(get_run))
        .route("/api/runs/{id}/output", patch(patch_run_output))
        .route("/ws/daemon", any(ws_daemon_handler))
        .route("/ws/frontend", any(ws_frontend_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Broadcast helper
// ---------------------------------------------------------------------------

fn broadcast_node_updated(hub: &Arc<RwLock<Hub>>, node_id: &str) {
    let msg = serde_json::json!({ "type": "node:updated", "id": node_id }).to_string();
    hub.read().unwrap().broadcast(&msg);
}

// ---------------------------------------------------------------------------
// POST /api/nodes — create node
// ---------------------------------------------------------------------------

async fn create_node(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNodeRequest>,
) -> Result<(StatusCode, Json<Node>), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let acceptance = req
        .acceptance_json
        .unwrap_or_else(|| r#"{"type":"prose","text":""}"#.to_string());

    sqlx::query(
        "INSERT INTO nodes (id, intent, parent_id, phase, acceptance_json, local_policy_json, created_at, updated_at)
         VALUES (?, ?, ?, 'draft', ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&req.intent)
    .bind(&req.parent_id)
    .bind(&acceptance)
    .bind(&req.local_policy_json)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    let node = fetch_node(&state.pool, &id).await?;
    broadcast_node_updated(&state.hub, &id);
    Ok((StatusCode::CREATED, Json(node)))
}

// ---------------------------------------------------------------------------
// GET /api/nodes — list top-level nodes (parent_id IS NULL)
// ---------------------------------------------------------------------------

async fn list_nodes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Node>>, AppError> {
    let rows = sqlx::query_as::<_, NodeRow>(
        "SELECT id, intent, parent_id, phase, acceptance_json, local_policy_json,
                canonical_artifact_text, canonical_updated_by_run_id,
                next_step_cache, next_step_cache_for_run_id,
                created_at, updated_at
         FROM nodes WHERE parent_id IS NULL ORDER BY created_at"
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows.into_iter().map(|r| r.into_node()).collect()))
}

// ---------------------------------------------------------------------------
// GET /api/nodes/:id — single node
// ---------------------------------------------------------------------------

async fn get_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Node>, AppError> {
    let node = fetch_node(&state.pool, &id).await?;
    Ok(Json(node))
}

// ---------------------------------------------------------------------------
// GET /api/nodes/:id/children — direct children
// ---------------------------------------------------------------------------

async fn get_children(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Node>>, AppError> {
    let rows = sqlx::query_as::<_, NodeRow>(
        "SELECT id, intent, parent_id, phase, acceptance_json, local_policy_json,
                canonical_artifact_text, canonical_updated_by_run_id,
                next_step_cache, next_step_cache_for_run_id,
                created_at, updated_at
         FROM nodes WHERE parent_id = ? ORDER BY created_at"
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows.into_iter().map(|r| r.into_node()).collect()))
}

// ---------------------------------------------------------------------------
// GET /api/nodes/:id/ancestors — ancestor chain [root, ..., parent] (NOT self)
// ---------------------------------------------------------------------------

async fn get_ancestors(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Node>>, AppError> {
    // Recursive CTE to walk up the ancestor chain
    let rows = sqlx::query_as::<_, NodeRow>(
        "WITH RECURSIVE ancestors(id, intent, parent_id, phase, acceptance_json, local_policy_json,
                                  canonical_artifact_text, canonical_updated_by_run_id,
                                  next_step_cache, next_step_cache_for_run_id,
                                  created_at, updated_at, depth) AS (
            -- Start from the node's parent (exclude self)
            SELECT n.id, n.intent, n.parent_id, n.phase, n.acceptance_json, n.local_policy_json,
                   n.canonical_artifact_text, n.canonical_updated_by_run_id,
                   n.next_step_cache, n.next_step_cache_for_run_id,
                   n.created_at, n.updated_at, 1
            FROM nodes n
            INNER JOIN nodes child ON child.parent_id = n.id
            WHERE child.id = ?
            UNION ALL
            SELECT p.id, p.intent, p.parent_id, p.phase, p.acceptance_json, p.local_policy_json,
                   p.canonical_artifact_text, p.canonical_updated_by_run_id,
                   p.next_step_cache, p.next_step_cache_for_run_id,
                   p.created_at, p.updated_at, a.depth + 1
            FROM nodes p
            INNER JOIN ancestors a ON a.parent_id = p.id
         )
         SELECT id, intent, parent_id, phase, acceptance_json, local_policy_json,
                canonical_artifact_text, canonical_updated_by_run_id,
                next_step_cache, next_step_cache_for_run_id,
                created_at, updated_at
         FROM ancestors
         ORDER BY depth DESC"
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows.into_iter().map(|r| r.into_node()).collect()))
}

// ---------------------------------------------------------------------------
// GET /api/nodes/:id/effective-policy — merged policy via recursive CTE
// ---------------------------------------------------------------------------

async fn get_effective_policy(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PolicyResponse>, AppError> {
    // Verify node exists
    let _ = fetch_node(&state.pool, &id).await?;

    let policy = compute_effective_policy(&state.pool, &id).await?;
    Ok(Json(PolicyResponse {
        tokens_max: policy.tokens_max,
        iterations_max: policy.iterations_max,
        wallclock_max_s: policy.wallclock_max_s,
        allowed_tools: policy.allowed_tools,
        review_required: policy.review_required,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/nodes/:id/state — compute and return NodeState
// ---------------------------------------------------------------------------

async fn get_node_state(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<StateQueryParams>,
) -> Result<Json<NodeState>, AppError> {
    let rollup_depth = params.rollup_depth.unwrap_or(1);
    let node_state =
        crate::state_layer::state_at(&state.pool, &id, rollup_depth, state.llm.as_ref()).await?;
    Ok(Json(node_state))
}

// ---------------------------------------------------------------------------
// PATCH /api/nodes/:id — update intent and/or local_policy only
// ---------------------------------------------------------------------------

async fn patch_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<PatchNodeRequest>,
) -> Result<Json<Node>, AppError> {
    // Reject if phase field is present
    if req.phase.is_some() {
        return Err(AppError::BadRequest(
            "phase cannot be set via PATCH; use dedicated phase transition endpoints".into(),
        ));
    }

    let existing = fetch_node(&state.pool, &id).await?;

    // Validate local_policy monotonicity if being updated
    if let Some(ref new_policy_str) = req.local_policy {
        if let Some(ref existing_policy_str) = existing.local_policy_json {
            let existing_policy: Policy = serde_json::from_str(existing_policy_str)
                .map_err(|e| AppError::Internal(format!("invalid existing policy: {e}")))?;
            let new_policy: Policy = serde_json::from_str(new_policy_str)
                .map_err(|e| AppError::BadRequest(format!("invalid policy JSON: {e}")))?;

            validate_policy_monotonicity(&existing_policy, &new_policy)?;
        }
    }

    let now = chrono::Utc::now().to_rfc3339();

    if let Some(ref intent) = req.intent {
        sqlx::query("UPDATE nodes SET intent = ?, updated_at = ? WHERE id = ?")
            .bind(intent)
            .bind(&now)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(ref local_policy) = req.local_policy {
        sqlx::query("UPDATE nodes SET local_policy_json = ?, updated_at = ? WHERE id = ?")
            .bind(local_policy)
            .bind(&now)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    let node = fetch_node(&state.pool, &id).await?;
    broadcast_node_updated(&state.hub, &id);
    Ok(Json(node))
}

/// Validate that a new policy only tightens (never loosens) compared to existing.
fn validate_policy_monotonicity(existing: &Policy, new: &Policy) -> Result<(), AppError> {
    // tokens_max: new must be <= existing (tighter)
    if let (Some(existing_val), Some(new_val)) = (existing.tokens_max, new.tokens_max) {
        if new_val > existing_val {
            return Err(AppError::BadRequest(format!(
                "tokens_max can only be tightened: {} -> {} is loosening",
                existing_val, new_val
            )));
        }
    }

    // iterations_max: new must be <= existing
    if let (Some(existing_val), Some(new_val)) = (existing.iterations_max, new.iterations_max) {
        if new_val > existing_val {
            return Err(AppError::BadRequest(format!(
                "iterations_max can only be tightened: {} -> {} is loosening",
                existing_val, new_val
            )));
        }
    }

    // wallclock_max_s: new must be <= existing
    if let (Some(existing_val), Some(new_val)) = (existing.wallclock_max_s, new.wallclock_max_s) {
        if new_val > existing_val {
            return Err(AppError::BadRequest(format!(
                "wallclock_max_s can only be tightened: {} -> {} is loosening",
                existing_val, new_val
            )));
        }
    }

    // review_required: cannot go from true to false
    if let (Some(true), Some(false)) = (existing.review_required, new.review_required) {
        return Err(AppError::BadRequest(
            "review_required cannot be loosened from true to false".into(),
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// DELETE /api/nodes/:id — soft delete (set phase=archived)
// ---------------------------------------------------------------------------

async fn delete_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Node>, AppError> {
    let existing = fetch_node(&state.pool, &id).await?;
    let phase: Phase = existing.phase.to_string().parse().unwrap();

    // Complete nodes cannot transition
    if phase == Phase::Complete {
        return Err(AppError::BadRequest(
            "cannot archive a completed node".into(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE nodes SET phase = 'archived', updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    let node = fetch_node(&state.pool, &id).await?;
    broadcast_node_updated(&state.hub, &id);
    Ok(Json(node))
}

// ---------------------------------------------------------------------------
// POST /api/nodes/:id/activate — Draft -> Active
// ---------------------------------------------------------------------------

async fn activate_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Node>, AppError> {
    let existing = fetch_node(&state.pool, &id).await?;

    if existing.phase != Phase::Draft {
        return Err(AppError::BadRequest(format!(
            "can only activate a draft node; current phase is {}",
            existing.phase
        )));
    }

    // Draft->Active blocked if acceptance is prose
    let acceptance: Acceptance = serde_json::from_str(&existing.acceptance_json)
        .map_err(|e| AppError::Internal(format!("invalid acceptance_json: {e}")))?;
    if matches!(acceptance, Acceptance::Prose { .. }) {
        return Err(AppError::BadRequest(
            "cannot activate a node with prose acceptance; structured acceptance required".into(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE nodes SET phase = 'active', updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    let node = fetch_node(&state.pool, &id).await?;
    broadcast_node_updated(&state.hub, &id);
    Ok(Json(node))
}

// ---------------------------------------------------------------------------
// POST /api/nodes/:id/review — Active -> In-Review
// ---------------------------------------------------------------------------

async fn review_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Node>, AppError> {
    let existing = fetch_node(&state.pool, &id).await?;

    if existing.phase != Phase::Active {
        return Err(AppError::BadRequest(format!(
            "can only review an active node; current phase is {}",
            existing.phase
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE nodes SET phase = 'in_review', updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    let node = fetch_node(&state.pool, &id).await?;
    broadcast_node_updated(&state.hub, &id);
    Ok(Json(node))
}

// ---------------------------------------------------------------------------
// WS /ws/daemon — daemon WebSocket
// ---------------------------------------------------------------------------

/// Expected daemon token for authentication.
fn expected_daemon_token() -> String {
    std::env::var("ENDGOAL_DAEMON_TOKEN").unwrap_or_else(|_| "dev-token".to_string())
}

async fn ws_daemon_handler(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    // Authenticate via Bearer token
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if token != expected_daemon_token() {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }

    ws.on_upgrade(move |socket| handle_daemon_ws(socket, state))
}

async fn handle_daemon_ws(socket: ws::WebSocket, state: Arc<AppState>) {
    println!("[hub] Daemon WS connected");

    // Create an mpsc channel so `dispatch_run` can send messages to this daemon
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Register daemon in hub
    {
        let mut hub = state.hub.write().unwrap();
        hub.daemon = Some(tx);
    }

    // Run two concurrent tasks:
    //   1. Forward outbound messages from the channel to the WS socket
    //   2. Read inbound messages from the WS socket and process them
    // We split the socket into sink + stream.
    let (mut ws_sink, mut ws_stream) = {
        use futures::StreamExt;
        socket.split()
    };

    // Outbound task: forward hub -> daemon
    let outbound = tokio::spawn(async move {
        use futures::SinkExt;
        while let Some(msg) = rx.recv().await {
            if ws_sink.send(ws::Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Inbound task: process daemon -> backend messages
    let pool = state.pool.clone();
    let hub = Arc::clone(&state.hub);

    while let Some(msg) = {
        use futures::StreamExt;
        ws_stream.next().await
    } {
        match msg {
            Ok(ws::Message::Text(text)) => {
                if let Err(e) = process_daemon_message(&pool, &hub, &text).await {
                    eprintln!("[hub] Error processing daemon message: {e}");
                }
            }
            Ok(ws::Message::Close(_)) => {
                println!("[hub] Daemon WS closed gracefully");
                break;
            }
            Ok(_) => {} // Ignore binary, ping, pong
            Err(e) => {
                eprintln!("[hub] Daemon WS error: {e}");
                break;
            }
        }
    }

    // Daemon disconnected: abort outbound task and clear hub entry
    outbound.abort();

    // Mark all running runs as failed
    let now = chrono::Utc::now().to_rfc3339();
    if let Err(e) = sqlx::query(
        "UPDATE runs SET status = 'failed', ended_at = ? WHERE status = 'running'"
    )
    .bind(&now)
    .execute(&pool)
    .await
    {
        eprintln!("[hub] Failed to mark running runs as failed: {e}");
    }

    {
        let mut hub_guard = hub.write().unwrap();
        hub_guard.daemon = None;
    }

    println!("[hub] Daemon WS disconnected, running runs marked failed");
}

/// Process a single inbound message from the daemon.
async fn process_daemon_message(
    pool: &SqlitePool,
    hub: &Arc<RwLock<Hub>>,
    text: &str,
) -> Result<(), String> {
    let msg: WsDaemonMessage = serde_json::from_str(text)
        .map_err(|e| format!("failed to parse daemon message: {e}"))?;

    match msg {
        WsDaemonMessage::Event(event) => {
            let now = chrono::Utc::now().to_rfc3339();
            let run_id = &event.run_id;

            // Fetch the run to get node_id
            let run_row = sqlx::query_as::<_, (String, String)>(
                "SELECT node_id, status FROM runs WHERE id = ?"
            )
            .bind(run_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

            let (node_id, current_status) = match run_row {
                Some(r) => r,
                None => {
                    eprintln!("[hub] RunEvent for unknown run_id {run_id}");
                    return Ok(());
                }
            };

            // On first RunEvent: flip status to "running" and stamp started_at
            if current_status == "dispatched" {
                sqlx::query(
                    "UPDATE runs SET status = 'running', started_at = ? WHERE id = ?"
                )
                .bind(&now)
                .bind(run_id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            }

            // Write to run_events table
            let event_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO run_events (id, run_id, seq, event_type, data_text, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(&event_id)
            .bind(run_id)
            .bind(event.seq)
            .bind(&event.event_type)
            .bind(&event.data_text)
            .bind(&now)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

            // Broadcast run:updated to all frontend clients
            let msg = serde_json::json!({ "type": "run:updated", "id": run_id }).to_string();
            hub.read().unwrap().broadcast(&msg);

            let _ = node_id; // node_id available if needed for future broadcasts
        }

        WsDaemonMessage::Terminal(terminal) => {
            let run_id = &terminal.run_id;
            let now = chrono::Utc::now().to_rfc3339();

            // Fetch node_id for this run
            let run_row = sqlx::query_as::<_, (String,)>(
                "SELECT node_id FROM runs WHERE id = ?"
            )
            .bind(run_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

            let node_id = match run_row {
                Some((nid,)) => nid,
                None => {
                    eprintln!("[hub] RunTerminal for unknown run_id {run_id}");
                    return Ok(());
                }
            };

            // Update run status in DB
            sqlx::query(
                "UPDATE runs SET status = ?, ended_at = ? WHERE id = ?"
            )
            .bind(&terminal.status)
            .bind(&now)
            .bind(run_id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

            // Broadcast run:updated and node:updated
            {
                let hub_guard = hub.read().unwrap();
                hub_guard.broadcast(
                    &serde_json::json!({ "type": "run:updated", "id": run_id }).to_string()
                );
                hub_guard.broadcast(
                    &serde_json::json!({ "type": "node:updated", "id": node_id }).to_string()
                );
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// WS /ws/frontend — frontend WebSocket
// ---------------------------------------------------------------------------

async fn ws_frontend_handler(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_frontend_ws(socket, state))
}

async fn handle_frontend_ws(socket: ws::WebSocket, state: Arc<AppState>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Register client in hub
    let client_id = {
        let mut hub = state.hub.write().unwrap();
        hub.add_client(tx)
    };

    println!("[hub] Frontend client {} connected", client_id);

    let (mut ws_sink, mut ws_stream) = {
        use futures::StreamExt;
        socket.split()
    };

    // Outbound: hub -> frontend
    let outbound = tokio::spawn(async move {
        use futures::SinkExt;
        while let Some(msg) = rx.recv().await {
            if ws_sink.send(ws::Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Inbound: frontend -> backend (receive-only, no processing needed)
    while let Some(msg) = {
        use futures::StreamExt;
        ws_stream.next().await
    } {
        match msg {
            Ok(ws::Message::Close(_)) => break,
            Ok(_) => {} // Frontend sends nothing
            Err(_) => break,
        }
    }

    outbound.abort();

    {
        let mut hub = state.hub.write().unwrap();
        hub.remove_client(client_id);
    }

    println!("[hub] Frontend client {} disconnected", client_id);
}

// ---------------------------------------------------------------------------
// POST /api/nodes/:id/runs — dispatch a run with enforcement
// ---------------------------------------------------------------------------

async fn dispatch_run(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Json(req): Json<DispatchRunRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let node = fetch_node(&state.pool, &node_id).await?;

    // Enforcement rule 1: Node phase must be Active
    if node.phase != Phase::Active {
        // In-Review gets a special error code
        if node.phase == Phase::InReview {
            return Err(AppError::Unprocessable("in_review_gate".into()));
        }
        return Err(AppError::Unprocessable("wrong_phase".into()));
    }

    // Enforcement rule 2: acceptance must be Structured (unless exploration)
    if req.run_type != "exploration" {
        let acceptance: Acceptance = serde_json::from_str(&node.acceptance_json)
            .map_err(|e| AppError::Internal(format!("invalid acceptance_json: {e}")))?;
        if matches!(acceptance, Acceptance::Prose { .. }) {
            return Err(AppError::Unprocessable("requires_freeze".into()));
        }
    }

    // Enforcement rule 3: daemon must be connected (503 Service Unavailable)
    if !state.hub.read().unwrap().has_daemon() {
        return Err(AppError::ServiceUnavailable("no daemon connected".into()));
    }

    // Build the input_snapshot_json
    let acceptance: Acceptance = serde_json::from_str(&node.acceptance_json)
        .map_err(|e| AppError::Internal(format!("invalid acceptance_json: {e}")))?;

    let effective_policy = compute_effective_policy(&state.pool, &node_id).await?;

    let ancestors = fetch_ancestor_nodes(&state.pool, &node_id).await?;
    let parent_context: Vec<AncestorSummary> = ancestors
        .into_iter()
        .map(|a| {
            let acc_summary = a.acceptance_json.clone();
            AncestorSummary {
                id: a.id,
                intent: a.intent,
                phase: a.phase,
                acceptance_summary: acc_summary,
                canonical_summary: a.canonical_artifact_text,
                progress: 0,
            }
        })
        .collect();

    let run_input = RunInput {
        intent: node.intent.clone(),
        acceptance,
        effective_policy,
        parent_context,
        node_docs: vec![],
    };

    let input_snapshot_json = serde_json::to_string(&run_input)
        .map_err(|e| AppError::Internal(format!("failed to serialize input snapshot: {e}")))?;

    // Write Run row to DB
    let run_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO runs (id, node_id, type, status, runtime, input_snapshot_json, created_at)
         VALUES (?, ?, ?, 'dispatched', ?, ?, ?)"
    )
    .bind(&run_id)
    .bind(&node_id)
    .bind(&req.run_type)
    .bind(&req.runtime)
    .bind(&input_snapshot_json)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    // Build RunDispatch and send to daemon
    let dispatch = RunDispatch {
        run_id: run_id.clone(),
        input: run_input,
        runtime: req.runtime.clone(),
    };
    let dispatch_json = serde_json::to_string(&dispatch)
        .map_err(|e| AppError::Internal(format!("failed to serialize RunDispatch: {e}")))?;

    state.hub.read().unwrap().send_to_daemon(&dispatch_json);

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": run_id, "status": "dispatched" })),
    ))
}

// ---------------------------------------------------------------------------
// GET /api/nodes/:id/runs — list runs for node
// ---------------------------------------------------------------------------

async fn list_runs(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
) -> Result<Json<Vec<Run>>, AppError> {
    // Verify node exists
    let _ = fetch_node(&state.pool, &node_id).await?;

    let rows = sqlx::query_as::<_, RunRow>(
        "SELECT id, node_id, type, status, runtime, input_snapshot_json, output_json,
                scratchpad_path, started_at, ended_at, created_at
         FROM runs WHERE node_id = ? ORDER BY created_at"
    )
    .bind(&node_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows.into_iter().map(|r| r.into_run()).collect()))
}

// ---------------------------------------------------------------------------
// GET /api/runs/:id — get single run
// ---------------------------------------------------------------------------

async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<Json<Run>, AppError> {
    let run = fetch_run(&state.pool, &run_id).await?;
    Ok(Json(run))
}

// ---------------------------------------------------------------------------
// PATCH /api/runs/:id/output — write output_json
// ---------------------------------------------------------------------------

async fn patch_run_output(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
    Json(output): Json<serde_json::Value>,
) -> Result<Json<Run>, AppError> {
    // Verify run exists
    let _ = fetch_run(&state.pool, &run_id).await?;

    let output_str = serde_json::to_string(&output)
        .map_err(|e| AppError::Internal(format!("failed to serialize output: {e}")))?;

    sqlx::query("UPDATE runs SET output_json = ? WHERE id = ?")
        .bind(&output_str)
        .bind(&run_id)
        .execute(&state.pool)
        .await?;

    let run = fetch_run(&state.pool, &run_id).await?;
    Ok(Json(run))
}

// ---------------------------------------------------------------------------
// POST /api/nodes/:id/approve — In-Review -> Complete
// ---------------------------------------------------------------------------

async fn approve_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Node>, AppError> {
    let existing = fetch_node(&state.pool, &id).await?;

    if existing.phase != Phase::InReview {
        return Err(AppError::BadRequest(format!(
            "can only approve an in-review node; current phase is {}",
            existing.phase
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE nodes SET phase = 'complete', updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    let node = fetch_node(&state.pool, &id).await?;
    broadcast_node_updated(&state.hub, &id);
    Ok(Json(node))
}

// ---------------------------------------------------------------------------
// POST /api/nodes/:id/reject — In-Review -> Active, optional tighter_policy
// ---------------------------------------------------------------------------

async fn reject_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Option<Json<RejectNodeRequest>>,
) -> Result<Json<Node>, AppError> {
    let existing = fetch_node(&state.pool, &id).await?;

    if existing.phase != Phase::InReview {
        return Err(AppError::BadRequest(format!(
            "can only reject an in-review node; current phase is {}",
            existing.phase
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE nodes SET phase = 'active', updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    // Apply tighter_policy if provided
    if let Some(Json(req)) = body {
        if let Some(tighter_policy) = req.tighter_policy {
            let policy_str = serde_json::to_string(&tighter_policy)
                .map_err(|e| AppError::Internal(format!("failed to serialize policy: {e}")))?;
            sqlx::query("UPDATE nodes SET local_policy_json = ?, updated_at = ? WHERE id = ?")
                .bind(&policy_str)
                .bind(&now)
                .bind(&id)
                .execute(&state.pool)
                .await?;
        }
    }

    let node = fetch_node(&state.pool, &id).await?;
    broadcast_node_updated(&state.hub, &id);
    Ok(Json(node))
}

// ---------------------------------------------------------------------------
// DB helper types and functions
// ---------------------------------------------------------------------------

/// Row type for sqlx query_as. Mirrors the nodes table columns.
#[derive(Debug, sqlx::FromRow)]
struct NodeRow {
    id: String,
    intent: String,
    parent_id: Option<String>,
    phase: String,
    acceptance_json: String,
    local_policy_json: Option<String>,
    canonical_artifact_text: Option<String>,
    canonical_updated_by_run_id: Option<String>,
    next_step_cache: Option<String>,
    next_step_cache_for_run_id: Option<String>,
    created_at: String,
    updated_at: String,
}

impl NodeRow {
    fn into_node(self) -> Node {
        Node {
            id: self.id,
            intent: self.intent,
            parent_id: self.parent_id,
            phase: self.phase.parse::<Phase>().unwrap_or(Phase::Draft),
            acceptance_json: self.acceptance_json,
            local_policy_json: self.local_policy_json,
            canonical_artifact_text: self.canonical_artifact_text,
            canonical_updated_by_run_id: self.canonical_updated_by_run_id,
            next_step_cache: self.next_step_cache,
            next_step_cache_for_run_id: self.next_step_cache_for_run_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Minimal row for the effective_policy CTE.
#[derive(Debug, sqlx::FromRow)]
struct PolicyRow {
    local_policy_json: Option<String>,
}

/// Fetch a single node by ID.
async fn fetch_node(pool: &SqlitePool, id: &str) -> Result<Node, AppError> {
    let row = sqlx::query_as::<_, NodeRow>(
        "SELECT id, intent, parent_id, phase, acceptance_json, local_policy_json,
                canonical_artifact_text, canonical_updated_by_run_id,
                next_step_cache, next_step_cache_for_run_id,
                created_at, updated_at
         FROM nodes WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("node {id} not found")))?;

    Ok(row.into_node())
}

/// Row type for runs table.
#[derive(Debug, sqlx::FromRow)]
struct RunRow {
    id: String,
    node_id: String,
    #[sqlx(rename = "type")]
    run_type: String,
    status: String,
    runtime: String,
    input_snapshot_json: Option<String>,
    output_json: Option<String>,
    scratchpad_path: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    created_at: String,
}

impl RunRow {
    fn into_run(self) -> Run {
        Run {
            id: self.id,
            node_id: self.node_id,
            run_type: self.run_type,
            status: self.status,
            runtime: self.runtime,
            input_snapshot_json: self.input_snapshot_json,
            output_json: self.output_json,
            scratchpad_path: self.scratchpad_path,
            started_at: self.started_at,
            ended_at: self.ended_at,
            created_at: self.created_at,
        }
    }
}

/// Fetch a single run by ID.
async fn fetch_run(pool: &SqlitePool, id: &str) -> Result<Run, AppError> {
    let row = sqlx::query_as::<_, RunRow>(
        "SELECT id, node_id, type, status, runtime, input_snapshot_json, output_json,
                scratchpad_path, started_at, ended_at, created_at
         FROM runs WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("run {id} not found")))?;

    Ok(row.into_run())
}

/// Fetch ancestor nodes (excluding self), ordered root-first.
async fn fetch_ancestor_nodes(pool: &SqlitePool, node_id: &str) -> Result<Vec<Node>, AppError> {
    let rows = sqlx::query_as::<_, NodeRow>(
        "WITH RECURSIVE ancestors(id, intent, parent_id, phase, acceptance_json, local_policy_json,
                                  canonical_artifact_text, canonical_updated_by_run_id,
                                  next_step_cache, next_step_cache_for_run_id,
                                  created_at, updated_at, depth) AS (
            SELECT n.id, n.intent, n.parent_id, n.phase, n.acceptance_json, n.local_policy_json,
                   n.canonical_artifact_text, n.canonical_updated_by_run_id,
                   n.next_step_cache, n.next_step_cache_for_run_id,
                   n.created_at, n.updated_at, 1
            FROM nodes n
            INNER JOIN nodes child ON child.parent_id = n.id
            WHERE child.id = ?
            UNION ALL
            SELECT p.id, p.intent, p.parent_id, p.phase, p.acceptance_json, p.local_policy_json,
                   p.canonical_artifact_text, p.canonical_updated_by_run_id,
                   p.next_step_cache, p.next_step_cache_for_run_id,
                   p.created_at, p.updated_at, a.depth + 1
            FROM nodes p
            INNER JOIN ancestors a ON a.parent_id = p.id
         )
         SELECT id, intent, parent_id, phase, acceptance_json, local_policy_json,
                canonical_artifact_text, canonical_updated_by_run_id,
                next_step_cache, next_step_cache_for_run_id,
                created_at, updated_at
         FROM ancestors
         ORDER BY depth DESC"
    )
    .bind(node_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.into_node()).collect())
}

/// Compute the effective (merged) policy for a node via recursive CTE.
async fn compute_effective_policy(pool: &SqlitePool, node_id: &str) -> Result<Policy, AppError> {
    let rows: Vec<PolicyRow> = sqlx::query_as::<_, PolicyRow>(
        "WITH RECURSIVE chain(id, parent_id, local_policy_json, depth) AS (
            SELECT id, parent_id, local_policy_json, 0
            FROM nodes WHERE id = ?
            UNION ALL
            SELECT n.id, n.parent_id, n.local_policy_json, c.depth + 1
            FROM nodes n
            INNER JOIN chain c ON c.parent_id = n.id
         )
         SELECT local_policy_json FROM chain
         WHERE local_policy_json IS NOT NULL
         ORDER BY depth ASC"
    )
    .bind(node_id)
    .fetch_all(pool)
    .await?;

    let mut merged = Policy {
        tokens_max: None,
        iterations_max: None,
        wallclock_max_s: None,
        allowed_tools: None,
        review_required: None,
    };

    for row in &rows {
        if let Some(ref json_str) = row.local_policy_json {
            if let Ok(policy) = serde_json::from_str::<Policy>(json_str) {
                if let Some(val) = policy.tokens_max {
                    merged.tokens_max = Some(match merged.tokens_max {
                        Some(existing) => existing.min(val),
                        None => val,
                    });
                }
                if let Some(val) = policy.iterations_max {
                    merged.iterations_max = Some(match merged.iterations_max {
                        Some(existing) => existing.min(val),
                        None => val,
                    });
                }
                if let Some(val) = policy.wallclock_max_s {
                    merged.wallclock_max_s = Some(match merged.wallclock_max_s {
                        Some(existing) => existing.min(val),
                        None => val,
                    });
                }
                if let Some(ref tools) = policy.allowed_tools {
                    merged.allowed_tools = Some(match merged.allowed_tools {
                        Some(existing) => {
                            existing.into_iter().filter(|t| tools.contains(t)).collect()
                        }
                        None => tools.clone(),
                    });
                }
                if let Some(val) = policy.review_required {
                    merged.review_required = Some(match merged.review_required {
                        Some(existing) => existing || val,
                        None => val,
                    });
                }
            }
        }
    }

    Ok(merged)
}
