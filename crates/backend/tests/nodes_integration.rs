//! Integration tests for Node CRUD API + phase lifecycle enforcement.
//!
//! These tests exercise the full HTTP stack: axum server -> SQLite DB -> HTTP responses.
//! Each test gets a fresh temporary database.
//! All test names are prefixed with `nodes_` so `cargo test -- nodes` matches them.

use reqwest::Client;
use serde_json::{Value, json};
use std::net::SocketAddr;

/// Helper: start the axum server on a random port with a temp DB, return (addr, tempdir_guard).
async fn start_server() -> (SocketAddr, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

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

    (addr, tmp)
}

fn base_url(addr: SocketAddr) -> String {
    format!("http://{}", addr)
}

#[tokio::test]
async fn nodes_health_returns_ok() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{}/api/health", base_url(addr)))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

// ---------------------------------------------------------------------------
// AC1: POST /api/nodes creates Node, returns JSON with all fields
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_create_returns_all_fields() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Build the thing",
            "acceptance_json": "{\"type\":\"prose\",\"text\":\"it works\"}"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 201);
    let body: Value = resp.json().await.unwrap();

    // Verify all Node fields are present
    assert!(body["id"].is_string());
    assert_eq!(body["intent"], "Build the thing");
    assert!(body["parent_id"].is_null());
    assert_eq!(body["phase"], "draft");
    assert!(body["acceptance_json"].is_string());
    assert!(body["created_at"].is_string());
    assert!(body["updated_at"].is_string());
}

#[tokio::test]
async fn nodes_create_with_parent_and_policy() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    // Create parent
    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Parent node",
            "local_policy_json": "{\"tokens_max\":100000}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(parent_resp.status(), 201);
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    // Create child
    let child_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Child node",
            "parent_id": parent_id,
            "local_policy_json": "{\"tokens_max\":50000}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(child_resp.status(), 201);
    let child: Value = child_resp.json().await.unwrap();
    assert_eq!(child["parent_id"], parent_id);
}

#[tokio::test]
async fn nodes_create_rejects_invalid_local_policy_json() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Bad policy",
            "local_policy_json": "{\"tokens_max\":\"not-a-number\"}"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn nodes_create_rejects_policy_that_exceeds_parent_constraints() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Parent node",
            "local_policy_json": "{\"tokens_max\":100000,\"allowed_tools\":[\"read\"]}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(parent_resp.status(), 201);
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    let child_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Child node",
            "parent_id": parent_id,
            "local_policy_json": "{\"tokens_max\":200000,\"allowed_tools\":[\"read\",\"exec\"]}"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(child_resp.status(), 400);
}

#[tokio::test]
async fn nodes_create_rejects_review_required_false_under_required_parent() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Parent node",
            "local_policy_json": "{\"review_required\":true}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(parent_resp.status(), 201);
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    let child_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Child node",
            "parent_id": parent_id,
            "local_policy_json": "{\"review_required\":false}"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(child_resp.status(), 400);
}

// ---------------------------------------------------------------------------
// AC2: PATCH /api/nodes/:id with { "phase": "draft" } returns 400
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_patch_rejects_phase_field() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "test"}))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let patch_resp = client
        .patch(format!("{}/api/nodes/{}", base_url(addr), id))
        .json(&json!({"phase": "draft"}))
        .send()
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), 400);
}

// ---------------------------------------------------------------------------
// AC3: PATCH /api/nodes/:id with { "intent": "new intent" } returns 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_patch_updates_intent() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "old intent"}))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let patch_resp = client
        .patch(format!("{}/api/nodes/{}", base_url(addr), id))
        .json(&json!({"intent": "new intent"}))
        .send()
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), 200);

    let updated: Value = patch_resp.json().await.unwrap();
    assert_eq!(updated["intent"], "new intent");
}

// ---------------------------------------------------------------------------
// GET /api/nodes — list top-level nodes only
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_list_returns_only_top_level() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "root"}))
        .send()
        .await
        .unwrap();
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "child", "parent_id": parent_id}))
        .send()
        .await
        .unwrap();

    let list_resp = client
        .get(format!("{}/api/nodes", base_url(addr)))
        .send()
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let nodes: Vec<Value> = list_resp.json().await.unwrap();

    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["intent"], "root");
}

// ---------------------------------------------------------------------------
// GET /api/nodes/:id — get single node
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_get_single() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "my node"}))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let get_resp = client
        .get(format!("{}/api/nodes/{}", base_url(addr), id))
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 200);
    let fetched: Value = get_resp.json().await.unwrap();
    assert_eq!(fetched["id"], id);
    assert_eq!(fetched["intent"], "my node");
}

#[tokio::test]
async fn nodes_get_nonexistent_returns_404() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{}/api/nodes/nonexistent-id", base_url(addr)))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// GET /api/nodes/:id/children — list direct children
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_get_children() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "parent"}))
        .send()
        .await
        .unwrap();
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    for i in 0..2 {
        client
            .post(format!("{}/api/nodes", base_url(addr)))
            .json(&json!({"intent": format!("child-{i}"), "parent_id": parent_id}))
            .send()
            .await
            .unwrap();
    }

    let resp = client
        .get(format!(
            "{}/api/nodes/{}/children",
            base_url(addr),
            parent_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let children: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(children.len(), 2);
}

// ---------------------------------------------------------------------------
// AC4: GET /api/nodes/:id/ancestors for depth-3 returns [root, parent]
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_ancestors_depth_3() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let root_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "root"}))
        .send()
        .await
        .unwrap();
    let root: Value = root_resp.json().await.unwrap();
    let root_id = root["id"].as_str().unwrap();

    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "parent", "parent_id": root_id}))
        .send()
        .await
        .unwrap();
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    let leaf_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "leaf", "parent_id": parent_id}))
        .send()
        .await
        .unwrap();
    let leaf: Value = leaf_resp.json().await.unwrap();
    let leaf_id = leaf["id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/nodes/{}/ancestors",
            base_url(addr),
            leaf_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ancestors: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(ancestors.len(), 2, "depth-3 node should have 2 ancestors");
    assert_eq!(ancestors[0]["id"], root_id, "first ancestor should be root");
    assert_eq!(
        ancestors[1]["id"], parent_id,
        "second ancestor should be parent"
    );
}

// ---------------------------------------------------------------------------
// AC5: effective_policy — child tightens parent policy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_effective_policy_child_tightens() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "parent",
            "local_policy_json": "{\"tokens_max\":100000}"
        }))
        .send()
        .await
        .unwrap();
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    let child_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "child",
            "parent_id": parent_id,
            "local_policy_json": "{\"tokens_max\":50000}"
        }))
        .send()
        .await
        .unwrap();
    let child: Value = child_resp.json().await.unwrap();
    let child_id = child["id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/nodes/{}/effective-policy",
            base_url(addr),
            child_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let policy: Value = resp.json().await.unwrap();
    assert_eq!(policy["tokens_max"], 50000, "child tightens tokens_max");
}

#[tokio::test]
async fn nodes_effective_policy_parent_only() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "parent",
            "local_policy_json": "{\"iterations_max\":10}"
        }))
        .send()
        .await
        .unwrap();
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    let child_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "child",
            "parent_id": parent_id
        }))
        .send()
        .await
        .unwrap();
    let child: Value = child_resp.json().await.unwrap();
    let child_id = child["id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/nodes/{}/effective-policy",
            base_url(addr),
            child_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let policy: Value = resp.json().await.unwrap();
    assert_eq!(policy["iterations_max"], 10, "inherits parent policy");
}

#[tokio::test]
async fn nodes_effective_policy_review_required_or() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "parent",
            "local_policy_json": "{\"review_required\":false}"
        }))
        .send()
        .await
        .unwrap();
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    let child_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "child",
            "parent_id": parent_id,
            "local_policy_json": "{\"review_required\":true}"
        }))
        .send()
        .await
        .unwrap();
    let child: Value = child_resp.json().await.unwrap();
    let child_id = child["id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/nodes/{}/effective-policy",
            base_url(addr),
            child_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let policy: Value = resp.json().await.unwrap();
    assert_eq!(policy["review_required"], true, "OR: true wins");
}

#[tokio::test]
async fn nodes_effective_policy_allowed_tools_intersection() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "parent",
            "local_policy_json": "{\"allowed_tools\":[\"read\",\"write\",\"exec\"]}"
        }))
        .send()
        .await
        .unwrap();
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    let child_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "child",
            "parent_id": parent_id,
            "local_policy_json": "{\"allowed_tools\":[\"read\",\"write\"]}"
        }))
        .send()
        .await
        .unwrap();
    let child: Value = child_resp.json().await.unwrap();
    let child_id = child["id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/nodes/{}/effective-policy",
            base_url(addr),
            child_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let policy: Value = resp.json().await.unwrap();
    let tools: Vec<String> = serde_json::from_value(policy["allowed_tools"].clone()).unwrap();
    assert_eq!(tools.len(), 2);
    assert!(tools.contains(&"read".to_string()));
    assert!(tools.contains(&"write".to_string()));
}

// ---------------------------------------------------------------------------
// AC6: POST /api/nodes/:id/review — phase transitions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_review_active_transitions_to_in_review() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "test",
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let activate_resp = client
        .post(format!("{}/api/nodes/{}/activate", base_url(addr), id))
        .send()
        .await
        .unwrap();
    assert_eq!(
        activate_resp.status(),
        200,
        "activate should succeed with structured acceptance"
    );

    let review_resp = client
        .post(format!("{}/api/nodes/{}/review", base_url(addr), id))
        .send()
        .await
        .unwrap();
    assert_eq!(review_resp.status(), 200);
    let updated: Value = review_resp.json().await.unwrap();
    assert_eq!(updated["phase"], "in_review");
}

#[tokio::test]
async fn nodes_review_non_active_returns_400() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "test"}))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let review_resp = client
        .post(format!("{}/api/nodes/{}/review", base_url(addr), id))
        .send()
        .await
        .unwrap();
    assert_eq!(review_resp.status(), 400, "review on draft should fail");
}

// ---------------------------------------------------------------------------
// DELETE /api/nodes/:id — soft delete (sets phase=archived)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_delete_sets_archived() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({"intent": "to be archived"}))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let del_resp = client
        .delete(format!("{}/api/nodes/{}", base_url(addr), id))
        .send()
        .await
        .unwrap();
    assert_eq!(del_resp.status(), 200);

    let get_resp = client
        .get(format!("{}/api/nodes/{}", base_url(addr), id))
        .send()
        .await
        .unwrap();
    let archived: Value = get_resp.json().await.unwrap();
    assert_eq!(archived["phase"], "archived");
}

// ---------------------------------------------------------------------------
// Phase invariant tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_activate_with_prose_acceptance_returns_400() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "test",
            "acceptance_json": "{\"type\":\"prose\",\"text\":\"vague\"}"
        }))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let activate_resp = client
        .post(format!("{}/api/nodes/{}/activate", base_url(addr), id))
        .send()
        .await
        .unwrap();
    assert_eq!(
        activate_resp.status(),
        400,
        "Draft->Active with prose should fail"
    );
}

#[tokio::test]
async fn nodes_active_to_draft_via_patch_returns_400() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "test",
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    client
        .post(format!("{}/api/nodes/{}/activate", base_url(addr), id))
        .send()
        .await
        .unwrap();

    let patch_resp = client
        .patch(format!("{}/api/nodes/{}", base_url(addr), id))
        .json(&json!({"phase": "draft"}))
        .send()
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), 400);
}

// ---------------------------------------------------------------------------
// Policy monotonicity: local_policy updates must be tightening only
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_patch_policy_rejects_loosening() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "test",
            "local_policy_json": "{\"tokens_max\":50000}"
        }))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let patch_resp = client
        .patch(format!("{}/api/nodes/{}", base_url(addr), id))
        .json(&json!({"local_policy": "{\"tokens_max\":100000}"}))
        .send()
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), 400, "loosening policy should fail");
}

#[tokio::test]
async fn nodes_patch_policy_allows_tightening() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "test",
            "local_policy_json": "{\"tokens_max\":100000}"
        }))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let patch_resp = client
        .patch(format!("{}/api/nodes/{}", base_url(addr), id))
        .json(&json!({"local_policy": "{\"tokens_max\":50000}"}))
        .send()
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), 200, "tightening policy should succeed");
}

#[tokio::test]
async fn nodes_patch_policy_rejects_removing_existing_constraints() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "test",
            "local_policy_json": "{\"tokens_max\":100000,\"allowed_tools\":[\"read\",\"write\"],\"review_required\":true}"
        }))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let patch_resp = client
        .patch(format!("{}/api/nodes/{}", base_url(addr), id))
        .json(&json!({"local_policy": "{\"iterations_max\":5}"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        patch_resp.status(),
        400,
        "replacement policy must not remove existing restrictions"
    );
}

#[tokio::test]
async fn nodes_patch_policy_rejects_widening_allowed_tools() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "test",
            "local_policy_json": "{\"allowed_tools\":[\"read\",\"write\"]}"
        }))
        .send()
        .await
        .unwrap();
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap();

    let patch_resp = client
        .patch(format!("{}/api/nodes/{}", base_url(addr), id))
        .json(&json!({"local_policy": "{\"allowed_tools\":[\"read\",\"exec\"]}"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        patch_resp.status(),
        400,
        "allowed_tools can only be narrowed to a subset"
    );
}

#[tokio::test]
async fn nodes_patch_policy_rejects_loosening_parent_effective_policy() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    let parent_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "parent",
            "local_policy_json": "{\"tokens_max\":100000,\"allowed_tools\":[\"read\"]}"
        }))
        .send()
        .await
        .unwrap();
    let parent: Value = parent_resp.json().await.unwrap();
    let parent_id = parent["id"].as_str().unwrap();

    let child_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "child",
            "parent_id": parent_id
        }))
        .send()
        .await
        .unwrap();
    let child: Value = child_resp.json().await.unwrap();
    let child_id = child["id"].as_str().unwrap();

    let patch_resp = client
        .patch(format!("{}/api/nodes/{}", base_url(addr), child_id))
        .json(&json!({"local_policy": "{\"tokens_max\":200000,\"allowed_tools\":[\"read\",\"exec\"]}"}))
        .send()
        .await
        .unwrap();
    assert_eq!(
        patch_resp.status(),
        400,
        "new child policy must not loosen inherited parent restrictions"
    );
}

// ---------------------------------------------------------------------------
// AC8: E2E integration test — 3-level hierarchy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nodes_e2e_three_level_hierarchy() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    // 1. Create root node with policy
    let root_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Build product",
            "local_policy_json": "{\"tokens_max\":100000,\"iterations_max\":20,\"review_required\":true}",
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(root_resp.status(), 201);
    let root: Value = root_resp.json().await.unwrap();
    let root_id = root["id"].as_str().unwrap();

    // 2. Create middle node
    let mid_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Build backend",
            "parent_id": root_id,
            "local_policy_json": "{\"tokens_max\":75000,\"allowed_tools\":[\"read\",\"write\"]}",
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(mid_resp.status(), 201);
    let mid: Value = mid_resp.json().await.unwrap();
    let mid_id = mid["id"].as_str().unwrap();

    // 3. Create leaf node
    let leaf_resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Implement API",
            "parent_id": mid_id,
            "local_policy_json": "{\"tokens_max\":50000}",
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(leaf_resp.status(), 201);
    let leaf: Value = leaf_resp.json().await.unwrap();
    let leaf_id = leaf["id"].as_str().unwrap();

    // 4. Verify ancestor chain for leaf: [root, mid]
    let ancestors_resp = client
        .get(format!(
            "{}/api/nodes/{}/ancestors",
            base_url(addr),
            leaf_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(ancestors_resp.status(), 200);
    let ancestors: Vec<Value> = ancestors_resp.json().await.unwrap();
    assert_eq!(ancestors.len(), 2);
    assert_eq!(ancestors[0]["id"], root_id);
    assert_eq!(ancestors[1]["id"], mid_id);

    // 5. Verify effective policy for leaf
    let policy_resp = client
        .get(format!(
            "{}/api/nodes/{}/effective-policy",
            base_url(addr),
            leaf_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(policy_resp.status(), 200);
    let policy: Value = policy_resp.json().await.unwrap();
    assert_eq!(policy["tokens_max"], 50000, "leaf tightens to 50000");
    assert_eq!(policy["iterations_max"], 20, "inherits from root");
    assert_eq!(policy["review_required"], true, "inherits from root");
    let tools: Vec<String> = serde_json::from_value(policy["allowed_tools"].clone()).unwrap();
    assert!(tools.contains(&"read".to_string()));
    assert!(tools.contains(&"write".to_string()));

    // 6. Verify children of root = [mid]
    let children_resp = client
        .get(format!("{}/api/nodes/{}/children", base_url(addr), root_id))
        .send()
        .await
        .unwrap();
    assert_eq!(children_resp.status(), 200);
    let children: Vec<Value> = children_resp.json().await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0]["id"], mid_id);

    // 7. Verify phase transitions work through HTTP
    let activate_resp = client
        .post(format!("{}/api/nodes/{}/activate", base_url(addr), leaf_id))
        .send()
        .await
        .unwrap();
    assert_eq!(activate_resp.status(), 200);
    let activated: Value = activate_resp.json().await.unwrap();
    assert_eq!(activated["phase"], "active");

    let review_resp = client
        .post(format!("{}/api/nodes/{}/review", base_url(addr), leaf_id))
        .send()
        .await
        .unwrap();
    assert_eq!(review_resp.status(), 200);
    let reviewed: Value = review_resp.json().await.unwrap();
    assert_eq!(reviewed["phase"], "in_review");

    // 8. Verify top-level listing
    let list_resp = client
        .get(format!("{}/api/nodes", base_url(addr)))
        .send()
        .await
        .unwrap();
    let top_nodes: Vec<Value> = list_resp.json().await.unwrap();
    assert_eq!(top_nodes.len(), 1, "only root should be top-level");
    assert_eq!(top_nodes[0]["id"], root_id);

    // 9. Soft-delete mid
    let del_resp = client
        .delete(format!("{}/api/nodes/{}", base_url(addr), mid_id))
        .send()
        .await
        .unwrap();
    assert_eq!(del_resp.status(), 200);

    let get_mid = client
        .get(format!("{}/api/nodes/{}", base_url(addr), mid_id))
        .send()
        .await
        .unwrap();
    let mid_after: Value = get_mid.json().await.unwrap();
    assert_eq!(mid_after["phase"], "archived");
}
