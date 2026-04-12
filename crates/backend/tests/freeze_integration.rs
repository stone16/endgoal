//! Integration tests for freeze session backend endpoints (CP10).
//!
//! All test names are prefixed with `freeze_` so `cargo test -- freeze`
//! runs the checkpoint suite.

use reqwest::Client;
use serde_json::{Value, json};
use std::net::SocketAddr;

async fn start_server() -> (SocketAddr, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

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

    (addr, tmp)
}

fn base_url(addr: SocketAddr) -> String {
    format!("http://{}", addr)
}

async fn create_prose_node(client: &Client, addr: SocketAddr) -> String {
    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Freeze this goal",
            "acceptance_json": "{\"type\":\"prose\",\"text\":\"turn this into structured acceptance\"}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let node: Value = resp.json().await.unwrap();
    node["id"].as_str().unwrap().to_string()
}

async fn start_freeze_session(client: &Client, addr: SocketAddr, node_id: &str) -> String {
    let resp = client
        .post(format!(
            "{}/api/nodes/{}/freeze/start",
            base_url(addr),
            node_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: Value = resp.json().await.unwrap();
    body["session_id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn freeze_active_returns_null_then_active_session() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();
    let node_id = create_prose_node(&client, addr).await;

    let empty_resp = client
        .get(format!(
            "{}/api/nodes/{}/freeze/active",
            base_url(addr),
            node_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(empty_resp.status(), 200);
    let empty_body: Value = empty_resp.json().await.unwrap();
    assert!(empty_body.is_null());

    let session_id = start_freeze_session(&client, addr, &node_id).await;

    let active_resp = client
        .get(format!(
            "{}/api/nodes/{}/freeze/active",
            base_url(addr),
            node_id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(active_resp.status(), 200);
    let active_body: Value = active_resp.json().await.unwrap();
    assert_eq!(active_body["session_id"], session_id);
    assert_eq!(active_body["approved_items_json"], "[]");
    assert_eq!(active_body["current_layer"], "assertions");
}

#[tokio::test]
async fn freeze_start_abandons_previous_active_session() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();
    let node_id = create_prose_node(&client, addr).await;

    let first_session_id = start_freeze_session(&client, addr, &node_id).await;
    let second_session_id = start_freeze_session(&client, addr, &node_id).await;

    assert_ne!(first_session_id, second_session_id);

    let active_resp = client
        .get(format!(
            "{}/api/nodes/{}/freeze/active",
            base_url(addr),
            node_id
        ))
        .send()
        .await
        .unwrap();
    let active_body: Value = active_resp.json().await.unwrap();
    assert_eq!(active_body["session_id"], second_session_id);
}

#[tokio::test]
async fn freeze_respond_start_streams_assertion_proposal() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();
    let node_id = create_prose_node(&client, addr).await;
    let session_id = start_freeze_session(&client, addr, &node_id).await;

    let resp = client
        .post(format!(
            "{}/api/nodes/{}/freeze/respond",
            base_url(addr),
            node_id
        ))
        .json(&json!({
            "session_id": session_id,
            "user_response": "",
            "action": "start"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let stream_text = resp.text().await.unwrap();
    assert!(stream_text.contains("event: proposal"), "{stream_text}");
    assert!(
        stream_text.contains(r#""event_type":"proposal""#),
        "{stream_text}"
    );
    assert!(
        stream_text.contains(r#""layer":"assertion""#),
        "{stream_text}"
    );
    assert!(
        stream_text.contains(r#""item_json":"{\"id\":\"a1\""#),
        "{stream_text}"
    );
    assert!(
        stream_text.contains(r#""reasoning":"mock proposal""#),
        "{stream_text}"
    );
    assert!(
        stream_text.contains(r#""source_quote":"Freeze this goal""#),
        "{stream_text}"
    );
}

#[tokio::test]
async fn freeze_approve_persists_item_before_next_proposal() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();
    let node_id = create_prose_node(&client, addr).await;
    let session_id = start_freeze_session(&client, addr, &node_id).await;

    let item_json = json!({
        "id": "a-custom",
        "text": "custom assertion",
        "status": "pending"
    })
    .to_string();

    let resp = client
        .post(format!(
            "{}/api/nodes/{}/freeze/respond",
            base_url(addr),
            node_id
        ))
        .json(&json!({
            "session_id": session_id,
            "user_response": "looks good",
            "action": "approve",
            "approved_item_json": item_json
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let active_resp = client
        .get(format!(
            "{}/api/nodes/{}/freeze/active",
            base_url(addr),
            node_id
        ))
        .send()
        .await
        .unwrap();
    let active_body: Value = active_resp.json().await.unwrap();
    assert!(
        active_body["approved_items_json"]
            .as_str()
            .unwrap()
            .contains("custom assertion")
    );
}

#[tokio::test]
async fn freeze_skip_layer_advances_to_metrics() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();
    let node_id = create_prose_node(&client, addr).await;
    let session_id = start_freeze_session(&client, addr, &node_id).await;

    let resp = client
        .post(format!(
            "{}/api/nodes/{}/freeze/respond",
            base_url(addr),
            node_id
        ))
        .json(&json!({
            "session_id": session_id,
            "user_response": "skip assertions",
            "action": "skip_layer"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let stream_text = resp.text().await.unwrap();
    assert!(
        stream_text.contains("event: layer_complete"),
        "{stream_text}"
    );
    assert!(
        stream_text.contains(r#""next_layer":"metrics""#),
        "{stream_text}"
    );

    let active_resp = client
        .get(format!(
            "{}/api/nodes/{}/freeze/active",
            base_url(addr),
            node_id
        ))
        .send()
        .await
        .unwrap();
    let active_body: Value = active_resp.json().await.unwrap();
    assert_eq!(active_body["current_layer"], "metrics");

    let metric_resp = client
        .post(format!(
            "{}/api/nodes/{}/freeze/respond",
            base_url(addr),
            node_id
        ))
        .json(&json!({
            "session_id": session_id,
            "user_response": "start metrics",
            "action": "start"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(metric_resp.status(), 200);
    let metric_stream_text = metric_resp.text().await.unwrap();
    assert!(
        metric_stream_text.contains(r#""layer":"metric""#),
        "{metric_stream_text}"
    );
}

#[tokio::test]
async fn freeze_commit_writes_structured_acceptance_and_409s_on_repeat() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();
    let node_id = create_prose_node(&client, addr).await;
    let session_id = start_freeze_session(&client, addr, &node_id).await;

    let item_json = json!({
        "id": "a-commit",
        "text": "committed assertion",
        "status": "pending"
    })
    .to_string();
    let approve_resp = client
        .post(format!(
            "{}/api/nodes/{}/freeze/respond",
            base_url(addr),
            node_id
        ))
        .json(&json!({
            "session_id": session_id,
            "user_response": "approve",
            "action": "approve",
            "approved_item_json": item_json
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(approve_resp.status(), 200);

    let commit_resp = client
        .post(format!(
            "{}/api/nodes/{}/freeze/commit",
            base_url(addr),
            node_id
        ))
        .json(&json!({ "session_id": session_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(commit_resp.status(), 200);

    let node_resp = client
        .get(format!("{}/api/nodes/{}", base_url(addr), node_id))
        .send()
        .await
        .unwrap();
    let node: Value = node_resp.json().await.unwrap();
    assert_eq!(node["phase"], "active");
    let acceptance: Value =
        serde_json::from_str(node["acceptance_json"].as_str().unwrap()).unwrap();
    assert_eq!(acceptance["type"], "structured");
    assert_eq!(acceptance["assertions"][0]["text"], "committed assertion");

    let repeat_resp = client
        .post(format!(
            "{}/api/nodes/{}/freeze/commit",
            base_url(addr),
            node_id
        ))
        .json(&json!({ "session_id": session_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(repeat_resp.status(), 409);
}
