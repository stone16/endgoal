use axum::{
    Json, Router,
    extract::{Path, Query, State, WebSocketUpgrade, ws},
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse,
        sse::{Event, Sse},
    },
    routing::{any, get, patch, post},
};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePool;
use std::{
    collections::{HashSet, VecDeque},
    convert::Infallible,
    sync::{Arc, RwLock},
};
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;

use crate::errors::AppError;
use crate::hub::Hub;
use crate::llm::{LlmClient, create_llm_client};
use crate::shared::types::{
    Acceptance, Assertion, FreezeLayerCompleteEvent, FreezeProposal, Metric, Node, NodeState,
    Phase, Policy, RubricDimension, Run, RunDispatch, RunInput, StructuredAcceptance,
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
pub struct FreezeRespondRequest {
    pub session_id: String,
    pub user_response: String,
    pub action: String,
    pub approved_item_json: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FreezeCommitRequest {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct FreezeActiveResponse {
    pub session_id: String,
    pub approved_items_json: String,
    pub current_layer: String,
}

#[derive(Debug, Serialize)]
pub struct FreezeStartResponse {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RejectNodeRequest {
    pub reason: Option<String>,
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
        .route("/api/health", get(health))
        .route("/api/nodes", get(list_nodes).post(create_node))
        .route(
            "/api/nodes/{id}",
            get(get_node).patch(patch_node).delete(delete_node),
        )
        .route("/api/nodes/{id}/children", get(get_children))
        .route("/api/nodes/{id}/ancestors", get(get_ancestors))
        .route(
            "/api/nodes/{id}/effective-policy",
            get(get_effective_policy),
        )
        .route("/api/nodes/{id}/state", get(get_node_state))
        .route("/api/nodes/{id}/activate", post(activate_node))
        .route("/api/nodes/{id}/review", post(review_node))
        .route("/api/nodes/{id}/runs", get(list_runs).post(dispatch_run))
        .route(
            "/api/nodes/{id}/freeze/active",
            get(get_active_freeze_session),
        )
        .route("/api/nodes/{id}/freeze/start", post(start_freeze_session))
        .route(
            "/api/nodes/{id}/freeze/respond",
            post(respond_freeze_session),
        )
        .route("/api/nodes/{id}/freeze/commit", post(commit_freeze_session))
        .route("/api/nodes/{id}/approve", post(approve_node))
        .route("/api/nodes/{id}/reject", post(reject_node))
        .route("/api/runs/{id}", get(get_run))
        .route("/api/runs/{id}/stream", get(stream_run_events))
        .route("/api/runs/{id}/output", patch(patch_run_output))
        .route("/ws/daemon", any(ws_daemon_handler))
        .route("/ws/frontend", any(ws_frontend_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ---------------------------------------------------------------------------
// GET /api/health — process readiness
// ---------------------------------------------------------------------------

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
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
    let acceptance: Acceptance = serde_json::from_str(&acceptance)
        .map_err(|e| AppError::BadRequest(format!("invalid acceptance_json: {e}")))?;
    let acceptance = serde_json::to_string(&acceptance)
        .map_err(|e| AppError::Internal(format!("failed to serialize acceptance: {e}")))?;

    let local_policy_json = if let Some(policy_json) = req.local_policy_json {
        let policy: Policy = serde_json::from_str(&policy_json)
            .map_err(|e| AppError::BadRequest(format!("invalid local_policy_json: {e}")))?;

        if let Some(ref parent_id) = req.parent_id {
            let _parent = fetch_node(&state.pool, parent_id).await?;
            let parent_effective = compute_effective_policy(&state.pool, parent_id).await?;
            validate_policy_does_not_exceed_base(&parent_effective, &policy)?;
        }

        Some(
            serde_json::to_string(&policy)
                .map_err(|e| AppError::Internal(format!("failed to serialize policy: {e}")))?,
        )
    } else {
        None
    };

    sqlx::query(
        "INSERT INTO nodes (id, intent, parent_id, phase, acceptance_json, local_policy_json, created_at, updated_at)
         VALUES (?, ?, ?, 'draft', ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&req.intent)
    .bind(&req.parent_id)
    .bind(&acceptance)
    .bind(&local_policy_json)
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

async fn list_nodes(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Node>>, AppError> {
    let rows = sqlx::query_as::<_, NodeRow>(
        "SELECT id, intent, parent_id, phase, acceptance_json, local_policy_json,
                canonical_artifact_text, canonical_updated_by_run_id,
                next_step_cache, next_step_cache_for_run_id,
                created_at, updated_at
         FROM nodes WHERE parent_id IS NULL ORDER BY created_at",
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
         FROM nodes WHERE parent_id = ? ORDER BY created_at",
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
         ORDER BY depth DESC",
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

    let _existing = fetch_node(&state.pool, &id).await?;

    // Validate local_policy monotonicity if being updated
    let local_policy_to_write = if let Some(ref new_policy_str) = req.local_policy {
        let new_policy: Policy = serde_json::from_str(new_policy_str)
            .map_err(|e| AppError::BadRequest(format!("invalid policy JSON: {e}")))?;
        validate_policy_update_monotonicity(&state.pool, &id, &new_policy).await?;
        Some(
            serde_json::to_string(&new_policy)
                .map_err(|e| AppError::Internal(format!("failed to serialize policy: {e}")))?,
        )
    } else {
        None
    };

    let now = chrono::Utc::now().to_rfc3339();

    if let Some(ref intent) = req.intent {
        sqlx::query("UPDATE nodes SET intent = ?, updated_at = ? WHERE id = ?")
            .bind(intent)
            .bind(&now)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if let Some(ref local_policy) = local_policy_to_write {
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
    validate_numeric_policy_tightening("tokens_max", existing.tokens_max, new.tokens_max)?;
    validate_numeric_policy_tightening(
        "iterations_max",
        existing.iterations_max,
        new.iterations_max,
    )?;
    validate_numeric_policy_tightening(
        "wallclock_max_s",
        existing.wallclock_max_s,
        new.wallclock_max_s,
    )?;
    validate_allowed_tools_tightening(&existing.allowed_tools, &new.allowed_tools, false)?;

    if existing.review_required == Some(true) && new.review_required != Some(true) {
        return Err(AppError::BadRequest(
            "review_required cannot be loosened from true".into(),
        ));
    }

    Ok(())
}

fn validate_numeric_policy_tightening(
    name: &str,
    existing: Option<u64>,
    new: Option<u64>,
) -> Result<(), AppError> {
    match (existing, new) {
        (Some(existing_val), Some(new_val)) if new_val > existing_val => Err(AppError::BadRequest(
            format!("{name} can only be tightened: {existing_val} -> {new_val} is loosening",),
        )),
        (Some(_), None) => Err(AppError::BadRequest(format!(
            "{name} cannot be removed from an effective policy",
        ))),
        _ => Ok(()),
    }
}

fn validate_allowed_tools_tightening(
    existing: &Option<Vec<String>>,
    new: &Option<Vec<String>>,
    allow_omission: bool,
) -> Result<(), AppError> {
    match (existing, new) {
        (Some(existing_tools), Some(new_tools)) => {
            let existing_set: HashSet<&str> = existing_tools.iter().map(String::as_str).collect();
            let added: Vec<&str> = new_tools
                .iter()
                .map(String::as_str)
                .filter(|tool| !existing_set.contains(tool))
                .collect();

            if !added.is_empty() {
                return Err(AppError::BadRequest(format!(
                    "allowed_tools can only be narrowed; added tools: {}",
                    added.join(", ")
                )));
            }

            Ok(())
        }
        (Some(_), None) if !allow_omission => Err(AppError::BadRequest(
            "allowed_tools cannot be removed from an effective policy".into(),
        )),
        _ => Ok(()),
    }
}

fn validate_policy_does_not_exceed_base(base: &Policy, new_local: &Policy) -> Result<(), AppError> {
    validate_explicit_numeric_policy_tightening(
        "tokens_max",
        base.tokens_max,
        new_local.tokens_max,
    )?;
    validate_explicit_numeric_policy_tightening(
        "iterations_max",
        base.iterations_max,
        new_local.iterations_max,
    )?;
    validate_explicit_numeric_policy_tightening(
        "wallclock_max_s",
        base.wallclock_max_s,
        new_local.wallclock_max_s,
    )?;
    validate_allowed_tools_tightening(&base.allowed_tools, &new_local.allowed_tools, true)?;

    if base.review_required == Some(true) && new_local.review_required == Some(false) {
        return Err(AppError::BadRequest(
            "review_required cannot contradict an effective review requirement".into(),
        ));
    }

    Ok(())
}

fn validate_explicit_numeric_policy_tightening(
    name: &str,
    base: Option<u64>,
    new_local: Option<u64>,
) -> Result<(), AppError> {
    match (base, new_local) {
        (Some(existing_val), Some(new_val)) if new_val > existing_val => Err(AppError::BadRequest(
            format!("{name} cannot loosen effective policy: {existing_val} -> {new_val}",),
        )),
        _ => Ok(()),
    }
}

async fn validate_policy_update_monotonicity(
    pool: &SqlitePool,
    node_id: &str,
    new_local: &Policy,
) -> Result<(), AppError> {
    let current_effective = compute_effective_policy(pool, node_id).await?;
    let ancestor_effective = compute_ancestor_effective_policy(pool, node_id).await?;
    validate_policy_does_not_exceed_base(&ancestor_effective, new_local)?;

    let candidate_effective = policy_with_added_constraints(ancestor_effective, new_local);
    validate_policy_monotonicity(&current_effective, &candidate_effective)?;

    Ok(())
}

fn validate_policy_value_keys(policy: &serde_json::Value) -> Result<(), AppError> {
    let object = policy
        .as_object()
        .ok_or_else(|| AppError::BadRequest("tighter_policy must be a JSON object".into()))?;

    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "tokens_max"
                | "iterations_max"
                | "wallclock_max_s"
                | "allowed_tools"
                | "review_required"
        ) {
            return Err(AppError::BadRequest(format!(
                "unsupported tighter_policy key: {key}"
            )));
        }
    }

    Ok(())
}

fn policy_has_any_constraint(policy: &Policy) -> bool {
    policy.tokens_max.is_some()
        || policy.iterations_max.is_some()
        || policy.wallclock_max_s.is_some()
        || policy.allowed_tools.is_some()
        || policy.review_required.is_some()
}

fn empty_policy() -> Policy {
    Policy {
        tokens_max: None,
        iterations_max: None,
        wallclock_max_s: None,
        allowed_tools: None,
        review_required: None,
    }
}

fn policy_with_added_constraints(mut base: Policy, policy: &Policy) -> Policy {
    merge_policy_constraints(&mut base, policy);
    base
}

fn merge_policy_constraints(merged: &mut Policy, policy: &Policy) {
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
        merged.allowed_tools = Some(match merged.allowed_tools.take() {
            Some(existing) => existing
                .into_iter()
                .filter(|tool| tools.contains(tool))
                .collect(),
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

// ---------------------------------------------------------------------------
// DELETE /api/nodes/:id — soft delete (set phase=archived)
// ---------------------------------------------------------------------------

async fn delete_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Node>, AppError> {
    let existing = fetch_node(&state.pool, &id).await?;

    // Complete nodes cannot transition
    if existing.phase == Phase::Complete {
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

fn daemon_token_from_headers(headers: &HeaderMap) -> &str {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("")
}

fn authorize_daemon_headers(headers: &HeaderMap) -> Result<(), AppError> {
    if daemon_token_from_headers(headers) != expected_daemon_token() {
        return Err(AppError::Unauthorized("invalid token".into()));
    }

    Ok(())
}

fn broadcast_run_and_node_updated(hub: &Arc<RwLock<Hub>>, run_id: &str, node_id: &str) {
    let hub_guard = hub.read().unwrap();
    hub_guard.broadcast(&serde_json::json!({ "type": "run:updated", "id": run_id }).to_string());
    hub_guard.broadcast(&serde_json::json!({ "type": "node:updated", "id": node_id }).to_string());
}

async fn ws_daemon_handler(
    State(state): State<Arc<AppState>>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
) -> axum::response::Response {
    if let Err(err) = authorize_daemon_headers(&headers) {
        return err.into_response();
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

    // Mark all runs that were owned by this daemon connection as failed.
    let now = chrono::Utc::now().to_rfc3339();
    let failed_runs = match sqlx::query_as::<_, (String, String)>(
        "SELECT id, node_id FROM runs WHERE status IN ('dispatched', 'running')",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("[hub] Failed to fetch in-flight runs before disconnect: {e}");
            vec![]
        }
    };

    if let Err(e) = sqlx::query(
        "UPDATE runs SET status = 'failed', ended_at = ? WHERE status IN ('dispatched', 'running')",
    )
    .bind(&now)
    .execute(&pool)
    .await
    {
        eprintln!("[hub] Failed to mark running runs as failed: {e}");
    }

    for (run_id, node_id) in failed_runs {
        broadcast_run_and_node_updated(&hub, &run_id, &node_id);
    }

    {
        let mut hub_guard = hub.write().unwrap();
        hub_guard.daemon = None;
    }

    println!("[hub] Daemon WS disconnected, in-flight runs marked failed");
}

/// Process a single inbound message from the daemon.
async fn process_daemon_message(
    pool: &SqlitePool,
    hub: &Arc<RwLock<Hub>>,
    text: &str,
) -> Result<(), String> {
    let msg: WsDaemonMessage =
        serde_json::from_str(text).map_err(|e| format!("failed to parse daemon message: {e}"))?;

    match msg {
        WsDaemonMessage::Event(event) => {
            let now = chrono::Utc::now().to_rfc3339();
            let run_id = &event.run_id;

            // Fetch the run to get node_id
            let run_row = sqlx::query_as::<_, (String, String)>(
                "SELECT node_id, status FROM runs WHERE id = ?",
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
                sqlx::query("UPDATE runs SET status = 'running', started_at = ? WHERE id = ?")
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
                 VALUES (?, ?, ?, ?, ?, ?)",
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
            let run_row = sqlx::query_as::<_, (String,)>("SELECT node_id FROM runs WHERE id = ?")
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
            sqlx::query("UPDATE runs SET status = ?, ended_at = ? WHERE id = ?")
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
                    &serde_json::json!({ "type": "run:updated", "id": run_id }).to_string(),
                );
                hub_guard.broadcast(
                    &serde_json::json!({ "type": "node:updated", "id": node_id }).to_string(),
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

    let parent_context = crate::state_layer::assemble_parent_context(&state.pool, &node_id).await?;

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
         VALUES (?, ?, ?, 'dispatched', ?, ?, ?)",
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

    let dispatched_to_daemon = state.hub.read().unwrap().send_to_daemon(&dispatch_json);

    if !dispatched_to_daemon {
        let failed_at = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE runs SET status = 'failed', ended_at = ? WHERE id = ?")
            .bind(&failed_at)
            .bind(&run_id)
            .execute(&state.pool)
            .await?;

        return Err(AppError::ServiceUnavailable(
            "daemon disconnected during dispatch".into(),
        ));
    }

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
         FROM runs WHERE node_id = ? ORDER BY created_at",
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
// GET /api/runs/:id/stream — replay or live-stream run event rows as SSE
// ---------------------------------------------------------------------------

async fn stream_run_events(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<Sse<futures::stream::BoxStream<'static, Result<Event, Infallible>>>, AppError> {
    use futures::StreamExt;

    let run = fetch_run(&state.pool, &run_id).await?;

    if is_terminal_run_status(&run.status) {
        let rows = fetch_run_event_rows_after(&state.pool, &run_id, -1).await?;
        let stream = futures::stream::iter(
            rows.into_iter()
                .map(|row| Ok(run_event_stream_sse_event(row))),
        )
        .boxed();
        return Ok(Sse::new(stream));
    }

    let stream_state = RunEventLiveStreamState {
        pool: state.pool.clone(),
        run_id,
        last_seq: -1,
        pending: VecDeque::new(),
        interval: tokio::time::interval(std::time::Duration::from_millis(200)),
        done: false,
    };
    let stream = futures::stream::unfold(stream_state, next_live_run_event).boxed();
    Ok(Sse::new(stream))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct RunEventStreamRow {
    run_id: String,
    seq: i64,
    event_type: String,
    data_text: Option<String>,
    created_at: String,
}

struct RunEventLiveStreamState {
    pool: SqlitePool,
    run_id: String,
    last_seq: i64,
    pending: VecDeque<RunEventStreamRow>,
    interval: tokio::time::Interval,
    done: bool,
}

async fn next_live_run_event(
    mut state: RunEventLiveStreamState,
) -> Option<(Result<Event, Infallible>, RunEventLiveStreamState)> {
    loop {
        if let Some(row) = state.pending.pop_front() {
            return Some((Ok(run_event_stream_sse_event(row)), state));
        }

        if state.done {
            return None;
        }

        state.interval.tick().await;

        match fetch_run_event_rows_after(&state.pool, &state.run_id, state.last_seq).await {
            Ok(rows) => {
                for row in rows {
                    state.last_seq = state.last_seq.max(row.seq);
                    state.pending.push_back(row);
                }
            }
            Err(err) => {
                state.done = true;
                return Some((
                    Ok(run_event_stream_error_event(format!(
                        "failed to read run events: {err}"
                    ))),
                    state,
                ));
            }
        }

        match fetch_run_status(&state.pool, &state.run_id).await {
            Ok(status) => {
                if is_terminal_run_status(&status) {
                    state.done = true;
                }
            }
            Err(err) => {
                state.done = true;
                return Some((
                    Ok(run_event_stream_error_event(format!(
                        "failed to read run status: {err}"
                    ))),
                    state,
                ));
            }
        }
    }
}

async fn fetch_run_event_rows_after(
    pool: &SqlitePool,
    run_id: &str,
    last_seq: i64,
) -> Result<Vec<RunEventStreamRow>, sqlx::Error> {
    sqlx::query_as::<_, RunEventStreamRow>(
        "SELECT run_id, seq, event_type, data_text, created_at
         FROM run_events
         WHERE run_id = ? AND seq > ?
         ORDER BY seq",
    )
    .bind(run_id)
    .bind(last_seq)
    .fetch_all(pool)
    .await
}

async fn fetch_run_status(pool: &SqlitePool, run_id: &str) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT status FROM runs WHERE id = ?")
        .bind(run_id)
        .fetch_one(pool)
        .await
}

fn is_terminal_run_status(status: &str) -> bool {
    matches!(status, "completed" | "complete" | "failed")
}

fn run_event_stream_sse_event(row: RunEventStreamRow) -> Event {
    let event_type = row.event_type.clone();
    let body = serde_json::to_string(&row).unwrap_or_else(|err| {
        serde_json::json!({
            "run_id": row.run_id,
            "seq": row.seq,
            "event_type": "system",
            "data_text": format!("failed to serialize run event: {err}"),
            "created_at": row.created_at,
        })
        .to_string()
    });

    Event::default().event(event_type).data(body)
}

fn run_event_stream_error_event(message: String) -> Event {
    let body = serde_json::json!({
        "run_id": null,
        "seq": null,
        "event_type": "system",
        "data_text": message,
        "created_at": chrono::Utc::now().to_rfc3339(),
    })
    .to_string();

    Event::default().event("system").data(body)
}

// ---------------------------------------------------------------------------
// PATCH /api/runs/:id/output — write output_json
// ---------------------------------------------------------------------------

async fn patch_run_output(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
    Json(output): Json<serde_json::Value>,
) -> Result<Json<Run>, AppError> {
    authorize_daemon_headers(&headers)?;

    // Verify run exists
    let existing = fetch_run(&state.pool, &run_id).await?;

    let output_str = serde_json::to_string(&output)
        .map_err(|e| AppError::Internal(format!("failed to serialize output: {e}")))?;

    sqlx::query("UPDATE runs SET output_json = ? WHERE id = ?")
        .bind(&output_str)
        .bind(&run_id)
        .execute(&state.pool)
        .await?;

    let run = fetch_run(&state.pool, &run_id).await?;
    broadcast_run_and_node_updated(&state.hub, &run_id, &existing.node_id);
    Ok(Json(run))
}

// ---------------------------------------------------------------------------
// GET /api/nodes/:id/freeze/active — active freeze session if one exists
// ---------------------------------------------------------------------------

async fn get_active_freeze_session(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
) -> Result<Json<Option<FreezeActiveResponse>>, AppError> {
    let _ = fetch_node(&state.pool, &node_id).await?;

    let row = sqlx::query_as::<_, FreezeSessionRow>(
        "SELECT id, approved_items_json, current_layer, status
         FROM freeze_sessions
         WHERE node_id = ? AND status = 'active'
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(&node_id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(Json(row.map(|session| FreezeActiveResponse {
        session_id: session.id,
        approved_items_json: session.approved_items_json,
        current_layer: session.current_layer,
    })))
}

// ---------------------------------------------------------------------------
// POST /api/nodes/:id/freeze/start — begin a freeze session
// ---------------------------------------------------------------------------

async fn start_freeze_session(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
) -> Result<(StatusCode, Json<FreezeStartResponse>), AppError> {
    let _ = fetch_node(&state.pool, &node_id).await?;

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE freeze_sessions
         SET status = 'abandoned', updated_at = ?
         WHERE node_id = ? AND status = 'active'",
    )
    .bind(&now)
    .bind(&node_id)
    .execute(&state.pool)
    .await?;

    let session_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO freeze_sessions (
             id, node_id, approved_items_json, current_layer, status, created_at, updated_at
         ) VALUES (?, ?, '[]', 'assertions', 'active', ?, ?)",
    )
    .bind(&session_id)
    .bind(&node_id)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(FreezeStartResponse { session_id }),
    ))
}

// ---------------------------------------------------------------------------
// POST /api/nodes/:id/freeze/respond — persist response and stream next item
// ---------------------------------------------------------------------------

async fn respond_freeze_session(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Json(req): Json<FreezeRespondRequest>,
) -> Result<Sse<futures::stream::BoxStream<'static, Result<Event, Infallible>>>, AppError> {
    use futures::StreamExt;

    let mut session = fetch_freeze_session(&state.pool, &node_id, &req.session_id).await?;
    ensure_freeze_session_active(&session)?;

    match req.action.as_str() {
        "start" | "reject" => {}
        "approve" | "edit" => {
            let approved_item_json = req.approved_item_json.ok_or_else(|| {
                AppError::BadRequest("approved_item_json is required for approve/edit".into())
            })?;
            append_approved_item(
                &state.pool,
                &session.id,
                &session.current_layer,
                &approved_item_json,
            )
            .await?;
            session = fetch_freeze_session(&state.pool, &node_id, &req.session_id).await?;
        }
        "skip_layer" => {
            let next_layer = next_freeze_layer(&session.current_layer);
            let stored_layer = next_layer.unwrap_or("complete");
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "UPDATE freeze_sessions SET current_layer = ?, updated_at = ? WHERE id = ?",
            )
            .bind(stored_layer)
            .bind(&now)
            .bind(&session.id)
            .execute(&state.pool)
            .await?;

            let event = FreezeLayerCompleteEvent {
                event_type: "layer_complete".to_string(),
                layer: freeze_layer_label(&session.current_layer).to_string(),
                next_layer: next_layer.map(str::to_string),
            };
            let body = serde_json::to_string(&event).unwrap_or_else(|err| {
                serde_json::json!({
                    "event_type": "error",
                    "message": format!("failed to serialize layer complete event: {err}"),
                })
                .to_string()
            });
            let stream = futures::stream::once(async move {
                Ok(Event::default().event("layer_complete").data(body))
            })
            .boxed();
            return Ok(Sse::new(stream));
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "unsupported freeze response action: {other}"
            )));
        }
    }

    let node = fetch_node(&state.pool, &node_id).await?;
    let prompt = build_freeze_prompt(&state.pool, &node, &session, &req.user_response).await?;
    let layer = freeze_layer_label(&session.current_layer).to_string();
    let item_json = freeze_item_json(&layer, &node, &session.approved_items_json)?;
    let source_quote = node.intent.clone();
    let cancellation_token = CancellationToken::new();
    let cancel_on_drop = CancelOnDrop(cancellation_token.clone());
    let mut llm_stream = state.llm.stream(&prompt, cancellation_token);
    let stream = futures::stream::once(async move {
        let _cancel_on_drop = cancel_on_drop;
        let reasoning = match llm_stream.next().await {
            Some(Ok(chunk)) => chunk,
            Some(Err(err)) => format!("proposal stream error: {err}"),
            None => "No proposal reasoning returned".to_string(),
        };
        let proposal = FreezeProposal {
            event_type: "proposal".to_string(),
            layer,
            item_json,
            reasoning,
            source_quote,
        };
        let body = serde_json::to_string(&proposal).unwrap_or_else(|err| {
            serde_json::json!({
                "event_type": "error",
                "message": format!("failed to serialize freeze proposal: {err}"),
            })
            .to_string()
        });
        Ok(Event::default().event("proposal").data(body))
    })
    .boxed();

    Ok(Sse::new(stream))
}

// ---------------------------------------------------------------------------
// POST /api/nodes/:id/freeze/commit — write structured acceptance
// ---------------------------------------------------------------------------

async fn commit_freeze_session(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Json(req): Json<FreezeCommitRequest>,
) -> Result<Json<Node>, AppError> {
    let session = fetch_freeze_session(&state.pool, &node_id, &req.session_id).await?;

    if session.status == "committed" {
        return Err(AppError::Conflict(
            "freeze session already committed".into(),
        ));
    }
    ensure_freeze_session_active(&session)?;

    let acceptance = structured_acceptance_from_approved_items(&session.approved_items_json)?;
    let acceptance_json = serde_json::to_string(&Acceptance::Structured(acceptance))
        .map_err(|e| AppError::Internal(format!("failed to serialize acceptance: {e}")))?;
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE nodes
         SET acceptance_json = ?,
             phase = CASE WHEN phase = 'draft' THEN 'active' ELSE phase END,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(&acceptance_json)
    .bind(&now)
    .bind(&node_id)
    .execute(&state.pool)
    .await?;

    sqlx::query("UPDATE freeze_sessions SET status = 'committed', updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&session.id)
        .execute(&state.pool)
        .await?;

    let node = fetch_node(&state.pool, &node_id).await?;
    broadcast_node_updated(&state.hub, &node_id);
    Ok(Json(node))
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

    let mut review_details = serde_json::Map::new();
    let mut policy_str_to_write = None;

    if let Some(Json(req)) = body {
        if let Some(reason) = req.reason {
            let reason = reason.trim().to_string();

            if !reason.is_empty() {
                review_details.insert("reason".to_string(), serde_json::Value::String(reason));
            }
        }

        if let Some(tighter_policy) = req.tighter_policy {
            validate_policy_value_keys(&tighter_policy)?;
            let new_policy: Policy = serde_json::from_value(tighter_policy.clone())
                .map_err(|e| AppError::BadRequest(format!("invalid tighter_policy: {e}")))?;

            if !policy_has_any_constraint(&new_policy) {
                return Err(AppError::BadRequest(
                    "tighter_policy must include at least one constraint".into(),
                ));
            }

            let current_effective = compute_effective_policy(&state.pool, &id).await?;
            validate_policy_does_not_exceed_base(&current_effective, &new_policy)?;

            let policy_to_write = if let Some(ref existing_policy_str) = existing.local_policy_json
            {
                let existing_policy: Policy = serde_json::from_str(existing_policy_str)
                    .map_err(|e| AppError::Internal(format!("invalid existing policy: {e}")))?;
                policy_with_added_constraints(existing_policy, &new_policy)
            } else {
                new_policy
            };

            validate_policy_update_monotonicity(&state.pool, &id, &policy_to_write).await?;

            let policy_str = serde_json::to_string(&policy_to_write)
                .map_err(|e| AppError::Internal(format!("failed to serialize policy: {e}")))?;
            policy_str_to_write = Some(policy_str);
            review_details.insert("tighter_policy".to_string(), tighter_policy);
        }
    }

    let now = chrono::Utc::now().to_rfc3339();

    if let Some(policy_str) = policy_str_to_write {
        sqlx::query(
            "UPDATE nodes SET phase = 'active', local_policy_json = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&policy_str)
        .bind(&now)
        .bind(&id)
        .execute(&state.pool)
        .await?;
    } else {
        sqlx::query("UPDATE nodes SET phase = 'active', updated_at = ? WHERE id = ?")
            .bind(&now)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    if !review_details.is_empty() {
        insert_review_log(
            &state.pool,
            &id,
            "human",
            "reject",
            Some(serde_json::Value::Object(review_details)),
            &now,
        )
        .await?;
    }

    let node = fetch_node(&state.pool, &id).await?;
    broadcast_node_updated(&state.hub, &id);
    Ok(Json(node))
}

// ---------------------------------------------------------------------------
// DB helper types and functions
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct FreezeSessionRow {
    id: String,
    approved_items_json: String,
    current_layer: String,
    status: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApprovedFreezeItem {
    layer: String,
    item_json: String,
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

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

async fn fetch_freeze_session(
    pool: &SqlitePool,
    node_id: &str,
    session_id: &str,
) -> Result<FreezeSessionRow, AppError> {
    sqlx::query_as::<_, FreezeSessionRow>(
        "SELECT id, approved_items_json, current_layer, status
         FROM freeze_sessions
         WHERE id = ? AND node_id = ?",
    )
    .bind(session_id)
    .bind(node_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("freeze session {session_id} not found")))
}

fn ensure_freeze_session_active(session: &FreezeSessionRow) -> Result<(), AppError> {
    if session.status != "active" {
        return Err(AppError::Conflict(format!(
            "freeze session is not active: {}",
            session.status
        )));
    }

    Ok(())
}

async fn insert_review_log(
    pool: &SqlitePool,
    node_id: &str,
    actor: &str,
    action: &str,
    details: Option<serde_json::Value>,
    now: &str,
) -> Result<(), AppError> {
    let details_json = details
        .map(|value| serde_json::to_string(&value))
        .transpose()
        .map_err(|e| AppError::Internal(format!("failed to serialize review details: {e}")))?;

    sqlx::query(
        "INSERT INTO review_log (id, node_id, actor, action, details_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(node_id)
    .bind(actor)
    .bind(action)
    .bind(details_json)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

async fn append_approved_item(
    pool: &SqlitePool,
    session_id: &str,
    layer: &str,
    item_json: &str,
) -> Result<(), AppError> {
    let current_json: String =
        sqlx::query_scalar("SELECT approved_items_json FROM freeze_sessions WHERE id = ?")
            .bind(session_id)
            .fetch_one(pool)
            .await?;
    let mut items = parse_approved_items(&current_json)?;
    items.push(ApprovedFreezeItem {
        layer: layer.to_string(),
        item_json: item_json.to_string(),
    });
    let next_json = serde_json::to_string(&items)
        .map_err(|e| AppError::Internal(format!("failed to serialize approved items: {e}")))?;
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE freeze_sessions SET approved_items_json = ?, updated_at = ? WHERE id = ?")
        .bind(&next_json)
        .bind(&now)
        .bind(session_id)
        .execute(pool)
        .await?;

    Ok(())
}

fn parse_approved_items(approved_items_json: &str) -> Result<Vec<ApprovedFreezeItem>, AppError> {
    serde_json::from_str(approved_items_json)
        .map_err(|e| AppError::BadRequest(format!("invalid approved_items_json: {e}")))
}

fn next_freeze_layer(layer: &str) -> Option<&'static str> {
    match layer {
        "assertions" => Some("metrics"),
        "metrics" => Some("rubric"),
        "rubric" => None,
        _ => Some("assertions"),
    }
}

fn freeze_layer_label(layer: &str) -> &'static str {
    match layer {
        "assertions" => "assertion",
        "metrics" => "metric",
        "rubric" => "rubric",
        _ => "assertion",
    }
}

async fn build_freeze_prompt(
    pool: &SqlitePool,
    node: &Node,
    session: &FreezeSessionRow,
    user_response: &str,
) -> Result<String, AppError> {
    let ancestors = fetch_ancestor_nodes(pool, &node.id).await?;
    let docs: Vec<String> = sqlx::query_scalar("SELECT content FROM node_docs WHERE node_id = ?")
        .bind(&node.id)
        .fetch_all(pool)
        .await?;

    Ok(format!(
        "Node intent: {}\nCurrent layer: {}\nUser response: {}\nApproved items: {}\nParent context: {}\nDocs: {}",
        node.intent,
        session.current_layer,
        user_response,
        session.approved_items_json,
        serde_json::to_string(&ancestors)
            .map_err(|e| AppError::Internal(format!("failed to serialize ancestors: {e}")))?,
        serde_json::to_string(&docs)
            .map_err(|e| AppError::Internal(format!("failed to serialize docs: {e}")))?,
    ))
}

fn freeze_item_json(
    layer: &str,
    node: &Node,
    approved_items_json: &str,
) -> Result<String, AppError> {
    let id = next_freeze_item_id(layer, approved_items_json)?;
    let value = match layer {
        "assertion" => serde_json::json!({
            "id": id,
            "text": format!("{} is satisfied", node.intent),
            "status": "pending"
        }),
        "metric" => serde_json::json!({
            "id": id,
            "name": "completion",
            "target": 1.0
        }),
        "rubric" => serde_json::json!({
            "id": id,
            "dimension": "quality",
            "scale": 10.0
        }),
        _ => serde_json::json!({
            "id": id,
            "text": format!("{} is satisfied", node.intent),
            "status": "pending"
        }),
    };

    serde_json::to_string(&value)
        .map_err(|e| AppError::Internal(format!("failed to serialize freeze item: {e}")))
}

fn next_freeze_item_id(layer: &str, approved_items_json: &str) -> Result<String, AppError> {
    let prefix = match layer {
        "metric" | "metrics" => "m",
        "rubric" => "r",
        _ => "a",
    };
    let matching_layers: &[&str] = match layer {
        "metric" | "metrics" => &["metric", "metrics"],
        "rubric" => &["rubric"],
        _ => &["assertion", "assertions"],
    };
    let mut existing_ids = HashSet::new();

    for item in parse_approved_items(approved_items_json)? {
        if !matching_layers.contains(&item.layer.as_str()) {
            continue;
        }

        let value: serde_json::Value = serde_json::from_str(&item.item_json)
            .map_err(|e| AppError::BadRequest(format!("invalid approved item_json: {e}")))?;

        if let Some(id) = value.get("id").and_then(serde_json::Value::as_str) {
            existing_ids.insert(id.to_string());
        }
    }

    for index in 1.. {
        let candidate = format!("{prefix}{index}");

        if !existing_ids.contains(&candidate) {
            return Ok(candidate);
        }
    }

    unreachable!("unbounded integer range should always yield a freeze item id")
}

fn structured_acceptance_from_approved_items(
    approved_items_json: &str,
) -> Result<StructuredAcceptance, AppError> {
    let items = parse_approved_items(approved_items_json)?;
    if items.is_empty() {
        return Err(AppError::BadRequest(
            "cannot commit freeze session without approved items".into(),
        ));
    }

    let mut assertions = Vec::new();
    let mut metrics = Vec::new();
    let mut rubric = Vec::new();

    for item in items {
        match item.layer.as_str() {
            "assertions" | "assertion" => {
                assertions.push(serde_json::from_str::<Assertion>(&item.item_json).map_err(
                    |e| AppError::BadRequest(format!("invalid assertion item_json: {e}")),
                )?);
            }
            "metrics" | "metric" => {
                metrics.push(
                    serde_json::from_str::<Metric>(&item.item_json).map_err(|e| {
                        AppError::BadRequest(format!("invalid metric item_json: {e}"))
                    })?,
                );
            }
            "rubric" => {
                rubric.push(
                    serde_json::from_str::<RubricDimension>(&item.item_json).map_err(|e| {
                        AppError::BadRequest(format!("invalid rubric item_json: {e}"))
                    })?,
                );
            }
            other => {
                return Err(AppError::BadRequest(format!(
                    "unknown approved item layer: {other}"
                )));
            }
        }
    }

    Ok(StructuredAcceptance {
        assertions,
        metrics,
        rubric,
    })
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
         FROM nodes WHERE id = ?",
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
         FROM runs WHERE id = ?",
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
         ORDER BY depth DESC",
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
         ORDER BY depth ASC",
    )
    .bind(node_id)
    .fetch_all(pool)
    .await?;

    let mut merged = empty_policy();

    for row in &rows {
        if let Some(ref json_str) = row.local_policy_json {
            if let Ok(policy) = serde_json::from_str::<Policy>(json_str) {
                merge_policy_constraints(&mut merged, &policy);
            }
        }
    }

    Ok(merged)
}

async fn compute_ancestor_effective_policy(
    pool: &SqlitePool,
    node_id: &str,
) -> Result<Policy, AppError> {
    let rows: Vec<PolicyRow> = sqlx::query_as::<_, PolicyRow>(
        "WITH RECURSIVE chain(id, parent_id, local_policy_json, depth) AS (
            SELECT parent.id, parent.parent_id, parent.local_policy_json, 1
            FROM nodes child
            INNER JOIN nodes parent ON child.parent_id = parent.id
            WHERE child.id = ?
            UNION ALL
            SELECT n.id, n.parent_id, n.local_policy_json, c.depth + 1
            FROM nodes n
            INNER JOIN chain c ON c.parent_id = n.id
         )
         SELECT local_policy_json FROM chain
         WHERE local_policy_json IS NOT NULL
         ORDER BY depth ASC",
    )
    .bind(node_id)
    .fetch_all(pool)
    .await?;

    let mut merged = empty_policy();
    for row in &rows {
        if let Some(ref json_str) = row.local_policy_json {
            if let Ok(policy) = serde_json::from_str::<Policy>(json_str) {
                merge_policy_constraints(&mut merged, &policy);
            }
        }
    }

    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(
        tokens_max: Option<u64>,
        iterations_max: Option<u64>,
        wallclock_max_s: Option<u64>,
        allowed_tools: Option<Vec<&str>>,
        review_required: Option<bool>,
    ) -> Policy {
        Policy {
            tokens_max,
            iterations_max,
            wallclock_max_s,
            allowed_tools: allowed_tools
                .map(|tools| tools.into_iter().map(str::to_string).collect()),
            review_required,
        }
    }

    fn test_node() -> Node {
        Node {
            id: "node-1".to_string(),
            intent: "ship the prototype".to_string(),
            parent_id: None,
            phase: Phase::Active,
            acceptance_json: r#"{"type":"structured","assertions":[],"metrics":[],"rubric":[]}"#
                .to_string(),
            local_policy_json: None,
            canonical_artifact_text: None,
            canonical_updated_by_run_id: None,
            next_step_cache: None,
            next_step_cache_for_run_id: None,
            created_at: "2026-04-12T00:00:00Z".to_string(),
            updated_at: "2026-04-12T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn policy_validation_rejects_loosening_and_unknown_shapes() {
        let existing = policy(
            Some(100),
            Some(10),
            Some(60),
            Some(vec!["read", "write"]),
            Some(true),
        );
        assert!(
            validate_policy_monotonicity(
                &existing,
                &policy(Some(50), Some(5), Some(30), Some(vec!["read"]), Some(true))
            )
            .is_ok()
        );

        assert!(
            validate_policy_monotonicity(&existing, &policy(Some(101), None, None, None, None))
                .is_err()
        );
        assert!(
            validate_policy_monotonicity(&existing, &policy(None, Some(11), None, None, None))
                .is_err()
        );
        assert!(
            validate_policy_monotonicity(&existing, &policy(None, None, Some(61), None, None))
                .is_err()
        );
        assert!(
            validate_policy_monotonicity(&existing, &policy(None, None, None, None, Some(false)))
                .is_err()
        );

        assert!(validate_policy_value_keys(&serde_json::json!({"tokens_max": 50})).is_ok());
        assert!(validate_policy_value_keys(&serde_json::json!(["tokens_max"])).is_err());
        assert!(validate_policy_value_keys(&serde_json::json!({"reason": "too broad"})).is_err());

        assert!(!policy_has_any_constraint(&policy(
            None, None, None, None, None
        )));
        assert!(policy_has_any_constraint(&policy(
            None,
            None,
            None,
            Some(vec!["read"]),
            None
        )));
    }

    #[test]
    fn freeze_layer_helpers_cover_all_layers() {
        assert_eq!(next_freeze_layer("assertions"), Some("metrics"));
        assert_eq!(next_freeze_layer("metrics"), Some("rubric"));
        assert_eq!(next_freeze_layer("rubric"), None);
        assert_eq!(next_freeze_layer("unexpected"), Some("assertions"));

        assert_eq!(freeze_layer_label("assertions"), "assertion");
        assert_eq!(freeze_layer_label("metrics"), "metric");
        assert_eq!(freeze_layer_label("rubric"), "rubric");
        assert_eq!(freeze_layer_label("unexpected"), "assertion");
    }

    #[test]
    fn freeze_item_json_generates_layer_specific_unique_items() {
        let node = test_node();
        let approved = serde_json::to_string(&vec![
            ApprovedFreezeItem {
                layer: "assertions".to_string(),
                item_json: r#"{"id":"a1","text":"old","status":"pending"}"#.to_string(),
            },
            ApprovedFreezeItem {
                layer: "metrics".to_string(),
                item_json: r#"{"id":"m1","name":"old","target":1.0}"#.to_string(),
            },
            ApprovedFreezeItem {
                layer: "rubric".to_string(),
                item_json: r#"{"id":"r1","dimension":"old","scale":10.0}"#.to_string(),
            },
        ])
        .expect("approved json");

        let assertion: serde_json::Value =
            serde_json::from_str(&freeze_item_json("assertion", &node, &approved).unwrap())
                .unwrap();
        let metric: serde_json::Value =
            serde_json::from_str(&freeze_item_json("metric", &node, &approved).unwrap()).unwrap();
        let rubric: serde_json::Value =
            serde_json::from_str(&freeze_item_json("rubric", &node, &approved).unwrap()).unwrap();
        let fallback: serde_json::Value =
            serde_json::from_str(&freeze_item_json("unknown", &node, &approved).unwrap()).unwrap();

        assert_eq!(assertion["id"], "a2");
        assert_eq!(metric["id"], "m2");
        assert_eq!(rubric["id"], "r2");
        assert_eq!(fallback["id"], "a2");
        assert!(next_freeze_item_id("metric", "not json").is_err());
        assert!(
            next_freeze_item_id("metric", r#"[{"layer":"metrics","item_json":"not json"}]"#)
                .is_err()
        );
    }

    #[test]
    fn structured_acceptance_from_items_handles_valid_and_invalid_layers() {
        let approved = serde_json::to_string(&vec![
            ApprovedFreezeItem {
                layer: "assertion".to_string(),
                item_json: r#"{"id":"a1","text":"done","status":"pending"}"#.to_string(),
            },
            ApprovedFreezeItem {
                layer: "metric".to_string(),
                item_json: r#"{"id":"m1","name":"completion","target":1.0}"#.to_string(),
            },
            ApprovedFreezeItem {
                layer: "rubric".to_string(),
                item_json: r#"{"id":"r1","dimension":"quality","scale":10.0}"#.to_string(),
            },
        ])
        .expect("approved json");
        let structured = structured_acceptance_from_approved_items(&approved).unwrap();
        assert_eq!(structured.assertions.len(), 1);
        assert_eq!(structured.metrics.len(), 1);
        assert_eq!(structured.rubric.len(), 1);

        assert!(structured_acceptance_from_approved_items("[]").is_err());
        assert!(
            structured_acceptance_from_approved_items(r#"[{"layer":"unknown","item_json":"{}"}]"#)
                .is_err()
        );
        assert!(
            structured_acceptance_from_approved_items(r#"[{"layer":"metric","item_json":"{}"}]"#)
                .is_err()
        );
        assert!(
            structured_acceptance_from_approved_items(r#"[{"layer":"rubric","item_json":"{}"}]"#)
                .is_err()
        );
    }

    #[test]
    fn ensure_freeze_session_active_rejects_inactive_sessions() {
        let active = FreezeSessionRow {
            id: "s1".to_string(),
            approved_items_json: "[]".to_string(),
            current_layer: "assertions".to_string(),
            status: "active".to_string(),
        };
        assert!(ensure_freeze_session_active(&active).is_ok());

        let inactive = FreezeSessionRow {
            status: "committed".to_string(),
            ..active
        };
        assert!(ensure_freeze_session_active(&inactive).is_err());
    }

    #[tokio::test]
    async fn live_stream_state_returns_pending_done_and_error_events() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let row = RunEventStreamRow {
            run_id: "run-1".to_string(),
            seq: 1,
            event_type: "stdout".to_string(),
            data_text: Some("hello".to_string()),
            created_at: "2026-04-12T00:00:00Z".to_string(),
        };
        let pending_state = RunEventLiveStreamState {
            pool: pool.clone(),
            run_id: "run-1".to_string(),
            last_seq: 0,
            pending: VecDeque::from([row]),
            interval: tokio::time::interval(std::time::Duration::from_millis(1)),
            done: false,
        };
        assert!(next_live_run_event(pending_state).await.is_some());

        let done_state = RunEventLiveStreamState {
            pool: pool.clone(),
            run_id: "run-1".to_string(),
            last_seq: 0,
            pending: VecDeque::new(),
            interval: tokio::time::interval(std::time::Duration::from_millis(1)),
            done: true,
        };
        assert!(next_live_run_event(done_state).await.is_none());

        pool.close().await;
        let error_state = RunEventLiveStreamState {
            pool,
            run_id: "run-1".to_string(),
            last_seq: 0,
            pending: VecDeque::new(),
            interval: tokio::time::interval(std::time::Duration::from_millis(1)),
            done: false,
        };
        assert!(next_live_run_event(error_state).await.is_some());

        let stream_row = RunEventStreamRow {
            run_id: "run-2".to_string(),
            seq: 2,
            event_type: "stderr".to_string(),
            data_text: None,
            created_at: "2026-04-12T00:00:00Z".to_string(),
        };
        let _ = run_event_stream_sse_event(stream_row);
        let _ = run_event_stream_error_event("failed".to_string());
    }

    #[tokio::test]
    async fn fetch_run_status_and_rows_cover_query_helpers() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let db_path = tmp.path().join("stream.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = crate::create_pool(&db_url).await.expect("pool");
        crate::run_migrations(&pool).await.expect("migrations");
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO nodes (id, intent, phase, acceptance_json, created_at, updated_at)
             VALUES ('n1', 'Node', 'active', '{\"type\":\"structured\",\"assertions\":[],\"metrics\":[],\"rubric\":[]}', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO runs (id, node_id, type, status, runtime, input_snapshot_json, created_at)
             VALUES ('r1', 'n1', 'research_iteration', 'completed', 'echo', '{}', ?)",
        )
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO run_events (id, run_id, seq, event_type, data_text, created_at)
             VALUES ('e1', 'r1', 1, 'stdout', 'hello', ?)",
        )
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(fetch_run_status(&pool, "r1").await.unwrap(), "completed");
        let rows = fetch_run_event_rows_after(&pool, "r1", 0).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert!(is_terminal_run_status("completed"));
        assert!(is_terminal_run_status("complete"));
        assert!(is_terminal_run_status("failed"));
        assert!(!is_terminal_run_status("running"));

        let normal_state = RunEventLiveStreamState {
            pool: pool.clone(),
            run_id: "r1".to_string(),
            last_seq: 0,
            pending: VecDeque::new(),
            interval: tokio::time::interval(std::time::Duration::from_millis(1)),
            done: false,
        };
        assert!(next_live_run_event(normal_state).await.is_some());

        let app_state = Arc::new(AppState {
            pool: pool.clone(),
            hub: Arc::new(RwLock::new(Hub::new())),
            llm: Arc::new(crate::llm::StubLlmClient),
        });
        assert!(
            stream_run_events(State(Arc::clone(&app_state)), Path("r1".to_string()))
                .await
                .is_ok()
        );

        sqlx::query(
            "INSERT INTO runs (id, node_id, type, status, runtime, input_snapshot_json, created_at)
             VALUES ('r2', 'n1', 'research_iteration', 'running', 'echo', '{}', ?)",
        )
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            stream_run_events(State(app_state), Path("r2".to_string()))
                .await
                .is_ok()
        );
    }
}
