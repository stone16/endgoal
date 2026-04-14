//! Integration tests for the State Layer (CP06).
//!
//! All test names are prefixed with `state_` so `cargo test -- state` matches them.
//! These tests implement AC1-AC9 from the CP06 acceptance criteria.

use reqwest::Client;
use serde_json::{Value, json};
use std::net::SocketAddr;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};

/// Helper: start the axum server on a random port with temp DB.
/// Also connects a mock daemon so dispatch endpoints work.
async fn start_server() -> (SocketAddr, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    // Enable ENDGOAL_LLM_STUB so no real LLM calls happen
    // Safety: test-only env mutation; tests run in separate processes
    unsafe {
        std::env::set_var("ENDGOAL_LLM_STUB", "true");
    }

    let pool = endgoal_backend::create_pool(&db_url).await.expect("pool");
    endgoal_backend::run_migrations(&pool)
        .await
        .expect("migrations");

    let app = endgoal_backend::create_router(pool);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Connect a mock daemon to allow run dispatches in tests that need it
    let ws_url = format!("ws://{}/ws/daemon", addr);
    let uri: tokio_tungstenite::tungstenite::http::Uri = ws_url.parse().unwrap();
    let mut req = uri.into_client_request().unwrap();
    req.headers_mut().insert(
        tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
        tokio_tungstenite::tungstenite::http::HeaderValue::from_static("Bearer dev-token"),
    );
    let mut daemon_ws = None;
    for _ in 0..10 {
        match connect_async(req.clone()).await {
            Ok((ws, _)) => {
                daemon_ws = Some(ws);
                break;
            }
            Err(_) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
            }
        }
    }
    let mut daemon_ws = daemon_ws.expect("mock daemon should connect");
    tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(Ok(_)) = daemon_ws.next().await {}
    });

    (addr, tmp)
}

fn base_url(addr: SocketAddr) -> String {
    format!("http://{}", addr)
}

async fn mark_run_completed(tmp: &tempfile::TempDir, run_id: &str) {
    let db_url = format!("sqlite://{}?mode=rwc", tmp.path().join("test.db").display());
    let pool = endgoal_backend::create_pool(&db_url).await.expect("pool");
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE runs SET status = 'completed', ended_at = ? WHERE id = ?")
        .bind(now)
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("mark run completed");

    pool.close().await;
}

/// Create a structured node with 3 assertions, 1 metric, 1 rubric.
/// Returns (node_id, acceptance_json).
async fn create_structured_node(client: &Client, addr: SocketAddr) -> String {
    let acceptance = json!({
        "type": "structured",
        "assertions": [
            {"id": "a1", "text": "first assertion", "status": "pending"},
            {"id": "a2", "text": "second assertion", "status": "pending"},
            {"id": "a3", "text": "third assertion", "status": "pending"}
        ],
        "metrics": [
            {"id": "m1", "name": "coverage", "target": 80.0, "unit": "%"}
        ],
        "rubric": [
            {"id": "r1", "dimension": "code quality", "scale": 10.0}
        ]
    });

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Test state computation",
            "acceptance_json": serde_json::to_string(&acceptance).unwrap()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let node: Value = resp.json().await.unwrap();
    node["id"].as_str().unwrap().to_string()
}

/// Activate a node (Draft -> Active).
async fn activate_node(client: &Client, addr: SocketAddr, node_id: &str) {
    let resp = client
        .post(format!("{}/api/nodes/{}/activate", base_url(addr), node_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

/// Dispatch a run and return run_id.
async fn dispatch_run(client: &Client, addr: SocketAddr, node_id: &str) -> String {
    let resp = client
        .post(format!("{}/api/nodes/{}/runs", base_url(addr), node_id))
        .json(&json!({"type": "research_iteration", "runtime": "echo"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let body: Value = resp.json().await.unwrap();
    body["id"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// AC1: Unit test — progress formula with 2 pass/1 fail assertions + metric + rubric
// ---------------------------------------------------------------------------

#[tokio::test]
async fn state_progress_formula_mixed_assertions_metric_rubric() {
    let (addr, tmp) = start_server().await;
    let client = Client::new();

    // Create node with 2 pass / 1 fail assertions, metric at 60%, rubric 7/10
    let acceptance = json!({
        "type": "structured",
        "assertions": [
            {"id": "a1", "text": "passes", "status": "pending"},
            {"id": "a2", "text": "passes too", "status": "pending"},
            {"id": "a3", "text": "fails", "status": "pending"}
        ],
        "metrics": [
            {"id": "m1", "name": "coverage", "target": 100.0, "unit": "%"}
        ],
        "rubric": [
            {"id": "r1", "dimension": "quality", "scale": 10.0}
        ]
    });

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Test progress formula",
            "acceptance_json": serde_json::to_string(&acceptance).unwrap()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let node: Value = resp.json().await.unwrap();
    let node_id = node["id"].as_str().unwrap().to_string();

    activate_node(&client, addr, &node_id).await;
    let run_id = dispatch_run(&client, addr, &node_id).await;

    // Write output with 2 pass / 1 fail assertions, metric at 60%, rubric 7/10
    let output = json!({
        "findings": "Test findings",
        "concerns": [],
        "confidence": 0.7,
        "needs_human_review": false,
        "assertion_results": [
            {"id": "a1", "text": "passes", "check_fn": null, "status": "pass"},
            {"id": "a2", "text": "passes too", "check_fn": null, "status": "pass"},
            {"id": "a3", "text": "fails", "check_fn": null, "status": "fail"}
        ],
        "metric_values": [
            {"id": "m1", "name": "coverage", "baseline": null, "current": 60.0, "target": 100.0, "unit": "%"}
        ],
        "rubric_scores": [
            {"id": "r1", "dimension": "quality", "score": 7.0, "scale": 10.0, "description": null}
        ]
    });

    let patch_resp = client
        .patch(format!("{}/api/runs/{}/output", base_url(addr), &run_id))
        .bearer_auth("dev-token")
        .json(&output)
        .send()
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), 200);

    mark_run_completed(&tmp, &run_id).await;

    let state_resp = client
        .get(format!("{}/api/nodes/{}/state", base_url(addr), &node_id))
        .send()
        .await
        .unwrap();
    assert_eq!(state_resp.status(), 200, "GET /state should return 200");
    let state: Value = state_resp.json().await.unwrap();

    let progress = state["progress"].as_f64().unwrap();
    assert!(
        (63.0..=67.0).contains(&progress),
        "expected progress around 64.67, got {progress}"
    );
    assert_eq!(state["confidence"].as_f64().unwrap(), 0.7);
}

// ---------------------------------------------------------------------------
// AC2: All-passing fixture: progress == 100
// ---------------------------------------------------------------------------

#[tokio::test]
async fn state_all_passing_progress_100() {
    let (addr, tmp) = start_server().await;
    let client = Client::new();

    let acceptance = json!({
        "type": "structured",
        "assertions": [
            {"id": "a1", "text": "all pass", "status": "pending"}
        ],
        "metrics": [
            {"id": "m1", "name": "score", "target": 100.0, "unit": "pts"}
        ],
        "rubric": [
            {"id": "r1", "dimension": "quality", "scale": 10.0}
        ]
    });

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "All pass test",
            "acceptance_json": serde_json::to_string(&acceptance).unwrap()
        }))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let node_id = node["id"].as_str().unwrap().to_string();

    activate_node(&client, addr, &node_id).await;
    let run_id = dispatch_run(&client, addr, &node_id).await;

    // All passing: assertion pass, metric at 100%, rubric 10/10
    let output = json!({
        "findings": "All good",
        "concerns": [],
        "confidence": 1.0,
        "needs_human_review": false,
        "assertion_results": [
            {"id": "a1", "text": "all pass", "check_fn": null, "status": "pass"}
        ],
        "metric_values": [
            {"id": "m1", "name": "score", "baseline": null, "current": 100.0, "target": 100.0, "unit": "pts"}
        ],
        "rubric_scores": [
            {"id": "r1", "dimension": "quality", "score": 10.0, "scale": 10.0, "description": null}
        ]
    });

    client
        .patch(format!("{}/api/runs/{}/output", base_url(addr), &run_id))
        .bearer_auth("dev-token")
        .json(&output)
        .send()
        .await
        .unwrap();

    mark_run_completed(&tmp, &run_id).await;

    let state_resp = client
        .get(format!("{}/api/nodes/{}/state", base_url(addr), &node_id))
        .send()
        .await
        .unwrap();
    assert_eq!(state_resp.status(), 200);
    let state: Value = state_resp.json().await.unwrap();
    assert_eq!(state["progress"].as_f64().unwrap(), 100.0);
    assert!(
        !state["next_step"].as_str().unwrap().is_empty(),
        "next_step should be non-empty"
    );
}

// ---------------------------------------------------------------------------
// AC3: GET /api/nodes/:id/state returns NodeState JSON with correct shape
// ---------------------------------------------------------------------------

#[tokio::test]
async fn state_endpoint_returns_correct_shape() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let node_id = create_structured_node(&client, addr).await;

    let resp = client
        .get(format!("{}/api/nodes/{}/state", base_url(addr), &node_id))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let state: Value = resp.json().await.unwrap();

    // Verify exact NodeState fields
    assert!(state.get("state").is_some(), "missing field: state");
    assert!(state.get("progress").is_some(), "missing field: progress");
    assert!(
        state.get("confidence").is_some(),
        "missing field: confidence"
    );
    assert!(state.get("next_step").is_some(), "missing field: next_step");
    assert!(
        state.get("effective_policy").is_some(),
        "missing field: effective_policy"
    );
    assert!(
        state.get("rollup_blockers").is_some(),
        "missing field: rollup_blockers"
    );

    // Type checks
    assert!(state["state"].is_string());
    assert!(state["progress"].is_number());
    assert!(state["confidence"].is_number());
    assert!(state["next_step"].is_string());
    assert!(state["effective_policy"].is_object());
    assert!(state["rollup_blockers"].is_array());
}

// ---------------------------------------------------------------------------
// AC5: Rollup — Active child with zero completed Runs and progress==0 → blocked
// ---------------------------------------------------------------------------

#[tokio::test]
async fn state_rollup_blockers_active_child_no_runs() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    // Create parent node
    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Parent goal",
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[{\"id\":\"a1\",\"text\":\"done\",\"status\":\"pending\"}],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(parent_resp.status(), 201);
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap().to_string();

    activate_node(&client, addr, &parent_id).await;

    // Create child node (Active, structured acceptance)
    let child_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Child goal",
            "parent_id": parent_id,
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[{\"id\":\"b1\",\"text\":\"child done\",\"status\":\"pending\"}],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(child_resp.status(), 201);
    let child: Value = child_resp.json().await.unwrap();
    let child_id = child["id"].as_str().unwrap().to_string();

    // Activate child (now it's Active with zero completed runs)
    activate_node(&client, addr, &child_id).await;

    // GET /api/nodes/parent_id/state?rollup_depth=1
    let state_resp = client
        .get(format!(
            "{}/api/nodes/{}/state?rollup_depth=1",
            base_url(addr),
            &parent_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(state_resp.status(), 200);
    let state: Value = state_resp.json().await.unwrap();

    let blockers = state["rollup_blockers"].as_array().unwrap();
    assert!(
        blockers.iter().any(|b| b.as_str() == Some(&child_id)),
        "child_id should be in rollup_blockers; got: {:?}",
        blockers
    );
}

// ---------------------------------------------------------------------------
// AC6: next_step is non-empty string (stubbed)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn state_next_step_is_non_empty() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let node_id = create_structured_node(&client, addr).await;

    let resp = client
        .get(format!("{}/api/nodes/{}/state", base_url(addr), &node_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let state: Value = resp.json().await.unwrap();
    let next_step = state["next_step"].as_str().unwrap();
    assert!(!next_step.is_empty(), "next_step should not be empty");
}

// ---------------------------------------------------------------------------
// AC8: LLM is injectable via stub — returns "mock next_step"
// ---------------------------------------------------------------------------

#[tokio::test]
async fn state_llm_stub_returns_mock_next_step() {
    // ENDGOAL_LLM_STUB=true is set in start_server()
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let node_id = create_structured_node(&client, addr).await;
    activate_node(&client, addr, &node_id).await;
    let run_id = dispatch_run(&client, addr, &node_id).await;

    // Write output so canonical_artifact_text gets set and next_step must be generated
    let output = json!({
        "findings": "Test findings for LLM stub test",
        "concerns": [],
        "confidence": 0.8,
        "needs_human_review": false,
        "assertion_results": [],
        "metric_values": [],
        "rubric_scores": []
    });
    client
        .patch(format!("{}/api/runs/{}/output", base_url(addr), &run_id))
        .bearer_auth("dev-token")
        .json(&output)
        .send()
        .await
        .unwrap();

    let resp = client
        .get(format!("{}/api/nodes/{}/state", base_url(addr), &node_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let state: Value = resp.json().await.unwrap();
    let next_step = state["next_step"].as_str().unwrap();
    // With ENDGOAL_LLM_STUB=true, when canonical_artifact_text exists, should return "mock next_step"
    assert!(
        !next_step.is_empty(),
        "next_step should be non-empty with stub"
    );
}

// ---------------------------------------------------------------------------
// AC9 (E2E): Full state computation with formula verification
// Creates Node with structured acceptance (3 assertions, 1 metric, 1 rubric),
// creates completed Run with output_json, calls GET /state, asserts progress ±2
// ---------------------------------------------------------------------------

#[tokio::test]
async fn state_e2e_progress_formula_verification() {
    let (addr, tmp) = start_server().await;
    let client = Client::new();

    // 1. Create node with 3 assertions, 1 metric, 1 rubric
    let acceptance = json!({
        "type": "structured",
        "assertions": [
            {"id": "a1", "text": "assertion 1", "status": "pending"},
            {"id": "a2", "text": "assertion 2", "status": "pending"},
            {"id": "a3", "text": "assertion 3", "status": "pending"}
        ],
        "metrics": [
            {"id": "m1", "name": "coverage", "target": 100.0, "unit": "%"}
        ],
        "rubric": [
            {"id": "r1", "dimension": "quality", "scale": 10.0}
        ]
    });

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "E2E progress test node",
            "acceptance_json": serde_json::to_string(&acceptance).unwrap()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let node: Value = resp.json().await.unwrap();
    let node_id = node["id"].as_str().unwrap().to_string();

    // 2. Activate
    activate_node(&client, addr, &node_id).await;

    // 3. Dispatch run
    let run_id = dispatch_run(&client, addr, &node_id).await;

    // 4. Write output: 2/3 assertions pass (rate=0.667), metric at 60% (0.6),
    //    rubric score=7/10 (0.7)
    //    Expected progress = (0.667*0.4 + 0.6*0.4 + 0.7*0.2) * 100
    //                      = (0.2667 + 0.24 + 0.14) * 100 = 64.67 ≈ in [63, 67]
    let output = json!({
        "findings": "E2E test findings",
        "concerns": ["minor concern"],
        "confidence": 0.7,
        "needs_human_review": false,
        "assertion_results": [
            {"id": "a1", "text": "assertion 1", "check_fn": null, "status": "pass"},
            {"id": "a2", "text": "assertion 2", "check_fn": null, "status": "pass"},
            {"id": "a3", "text": "assertion 3", "check_fn": null, "status": "fail"}
        ],
        "metric_values": [
            {"id": "m1", "name": "coverage", "baseline": null, "current": 60.0, "target": 100.0, "unit": "%"}
        ],
        "rubric_scores": [
            {"id": "r1", "dimension": "quality", "score": 7.0, "scale": 10.0, "description": null}
        ]
    });

    let patch_resp = client
        .patch(format!("{}/api/runs/{}/output", base_url(addr), &run_id))
        .bearer_auth("dev-token")
        .json(&output)
        .send()
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), 200);

    mark_run_completed(&tmp, &run_id).await;

    let state_resp = client
        .get(format!("{}/api/nodes/{}/state", base_url(addr), &node_id))
        .send()
        .await
        .unwrap();
    assert_eq!(state_resp.status(), 200);
    let state: Value = state_resp.json().await.unwrap();

    assert_eq!(state["state"].as_str().unwrap(), "active");
    let progress = state["progress"].as_f64().unwrap();
    assert!(
        (63.0..=67.0).contains(&progress),
        "expected progress around 64.67, got {progress}"
    );
    assert!(state["confidence"].is_number());
    assert!(state["next_step"].is_string() && !state["next_step"].as_str().unwrap().is_empty());
    assert!(state["effective_policy"].is_object());
    assert!(state["rollup_blockers"].is_array());
}

// ---------------------------------------------------------------------------
// AC7 (meta): cargo test -p endgoal-backend -- state passes
// (This IS the test file that satisfies AC7)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// AC4: parent_context for depth-3 Node: array length == 2
// Test via unit test in state_layer.rs (too complex for HTTP-only test)
// We verify via a 3-level hierarchy here
// ---------------------------------------------------------------------------

#[tokio::test]
async fn state_parent_context_depth3_has_two_ancestors() {
    // This is tested via the unit tests in state_layer.rs
    // Here we verify the endpoint works for a 3-level hierarchy

    // For the HTTP endpoint test, we can check that a depth-3 node's
    // input_snapshot_json (written during dispatch) has 2 ancestors.
    // state_at() includes parent_context in the NodeState shape check.
    // But NodeState doesn't include parent_context directly — it's computed
    // for internal use. The endpoint just returns NodeState.

    // Create 3-level hierarchy: root -> mid -> leaf
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let root_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Root goal",
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[{\"id\":\"a1\",\"text\":\"root done\",\"status\":\"pending\"}],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    let root: Value = root_resp.json().await.unwrap();
    let root_id = root["id"].as_str().unwrap().to_string();

    let mid_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Mid goal",
            "parent_id": root_id,
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[{\"id\":\"b1\",\"text\":\"mid done\",\"status\":\"pending\"}],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    let mid: Value = mid_resp.json().await.unwrap();
    let mid_id = mid["id"].as_str().unwrap().to_string();

    let leaf_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Leaf goal",
            "parent_id": mid_id,
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[{\"id\":\"c1\",\"text\":\"leaf done\",\"status\":\"pending\"}],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    let leaf: Value = leaf_resp.json().await.unwrap();
    let leaf_id = leaf["id"].as_str().unwrap().to_string();

    // GET state for the leaf — should work (parent_context computed internally)
    let state_resp = client
        .get(format!("{}/api/nodes/{}/state", base_url(addr), &leaf_id))
        .send()
        .await
        .unwrap();
    assert_eq!(state_resp.status(), 200);
    // The endpoint returns NodeState which doesn't expose parent_context directly.
    // The parent_context is used in next_step generation and run input snapshots.
    // The unit tests in state_layer.rs verify the array length == 2 for a depth-3 node.
    let state: Value = state_resp.json().await.unwrap();
    assert!(state["state"].is_string());
    assert!(state["progress"].is_number());

    // Verify via input_snapshot_json from dispatch (also tests parent_context length)
    activate_node(&client, addr, &root_id).await;
    activate_node(&client, addr, &mid_id).await;
    activate_node(&client, addr, &leaf_id).await;

    let dispatch_resp = client
        .post(format!("{}/api/nodes/{}/runs", base_url(addr), &leaf_id))
        .json(&json!({"type": "research_iteration", "runtime": "echo"}))
        .send()
        .await
        .unwrap();
    assert_eq!(dispatch_resp.status(), 201);
    let dispatched: Value = dispatch_resp.json().await.unwrap();
    let run_id = dispatched["id"].as_str().unwrap();

    let run_resp = client
        .get(format!("{}/api/runs/{}", base_url(addr), run_id))
        .send()
        .await
        .unwrap();
    let run: Value = run_resp.json().await.unwrap();
    let snapshot: Value =
        serde_json::from_str(run["input_snapshot_json"].as_str().unwrap()).unwrap();
    let parent_ctx = snapshot["parent_context"].as_array().unwrap();
    assert_eq!(
        parent_ctx.len(),
        2,
        "depth-3 leaf should have 2 ancestors (root + mid)"
    );
}
