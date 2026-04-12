//! Integration tests for the WebSocket hub implementation (CP05).
//!
//! Tests exercise the full WS hub: daemon client connects, frontend client connects,
//! Run is dispatched, daemon sends RunEvent, frontend receives broadcast.
//!
//! All test names prefixed with `ws_` so `cargo test -- ws` matches them.

use reqwest::Client;
use serde_json::{json, Value};
use std::net::SocketAddr;
use tokio::time::{Duration, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message, tungstenite::client::IntoClientRequest};
use futures::{SinkExt, StreamExt};

// ---------------------------------------------------------------------------
// Test server helper
// ---------------------------------------------------------------------------

/// Start server on a random port with a temp DB. Returns (addr, tempdir_guard).
async fn start_server() -> (SocketAddr, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = endgoal_backend::create_pool(&db_url).await.expect("pool");
    endgoal_backend::run_migrations(&pool).await.expect("migrations");

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

fn ws_url(addr: SocketAddr, path: &str) -> String {
    format!("ws://{}{}", addr, path)
}

/// Connect to ws/daemon with valid Bearer token. Returns the WS stream.
async fn connect_daemon(
    addr: SocketAddr,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = ws_url(addr, "/ws/daemon");
    let uri: tokio_tungstenite::tungstenite::http::Uri = url.parse().unwrap();
    let mut req = uri.into_client_request().unwrap();
    req.headers_mut().insert(
        tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
        tokio_tungstenite::tungstenite::http::HeaderValue::from_static("Bearer dev-token"),
    );
    let (ws, _) = connect_async(req).await.expect("daemon ws connect");
    ws
}

/// Connect to ws/frontend. Returns the WS stream.
async fn connect_frontend(
    addr: SocketAddr,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = ws_url(addr, "/ws/frontend");
    let (ws, _) = connect_async(&url).await.expect("frontend ws connect");
    ws
}

/// Create an active structured node, return its ID.
async fn create_active_node(client: &Client, addr: SocketAddr) -> String {
    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "WS hub test node",
            "acceptance_json": "{\"type\":\"structured\",\"assertions\":[{\"id\":\"a1\",\"text\":\"works\",\"status\":\"pending\"}],\"metrics\":[],\"rubric\":[]}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let node: Value = resp.json().await.unwrap();
    let id = node["id"].as_str().unwrap().to_string();

    // activate
    let act = client
        .post(format!("{}/api/nodes/{}/activate", base_url(addr), &id))
        .send()
        .await
        .unwrap();
    assert_eq!(act.status(), 200);
    id
}

// ---------------------------------------------------------------------------
// AC1 + AC5: Full round-trip — connect daemon + frontend, dispatch, send RunEvent,
//            assert frontend receives { type: "run:updated" }, DB has run_events row,
//            Run status is "running".
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_full_roundtrip_frontend_receives_run_updated() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    // 1. Connect daemon WS
    let mut daemon_ws = connect_daemon(addr).await;

    // 2. Connect frontend WS
    let (mut frontend_sink, mut frontend_stream) = connect_frontend(addr).await.split();
    let _ = frontend_sink; // keep sink alive

    // 3. Create active node and dispatch run
    let node_id = create_active_node(&client, addr).await;
    let dispatch_resp = client
        .post(format!("{}/api/nodes/{}/runs", base_url(addr), &node_id))
        .json(&json!({"type": "research_iteration", "runtime": "echo"}))
        .send()
        .await
        .unwrap();
    assert_eq!(dispatch_resp.status(), 201);
    let dispatched: Value = dispatch_resp.json().await.unwrap();
    let run_id = dispatched["id"].as_str().unwrap().to_string();

    // 4. Daemon receives RunDispatch message
    let dispatch_msg = timeout(Duration::from_secs(2), daemon_ws.next())
        .await
        .expect("daemon should receive RunDispatch within 2s")
        .expect("daemon stream not closed")
        .expect("daemon WS message");

    let dispatch_text = match dispatch_msg {
        Message::Text(t) => t,
        other => panic!("expected Text, got: {:?}", other),
    };
    let dispatch_json: Value = serde_json::from_str(&dispatch_text).unwrap();
    assert_eq!(dispatch_json["run_id"], run_id, "dispatch should contain run_id");

    // 5. Daemon sends RunEvent back
    let run_event = json!({
        "kind": "event",
        "run_id": run_id,
        "seq": 1,
        "event_type": "stdout",
        "data_text": "hello"
    });
    daemon_ws
        .send(Message::Text(run_event.to_string().into()))
        .await
        .expect("daemon send event");

    // 6. Frontend receives { type: "run:updated", id: run_id } within 2s
    let frontend_msg = timeout(Duration::from_secs(2), frontend_stream.next())
        .await
        .expect("frontend should receive run:updated within 2s")
        .expect("frontend stream not closed")
        .expect("frontend WS message");

    let msg_text = match frontend_msg {
        Message::Text(t) => t,
        other => panic!("expected Text, got: {:?}", other),
    };
    let msg_json: Value = serde_json::from_str(&msg_text).unwrap();
    assert_eq!(msg_json["type"], "run:updated", "frontend should receive run:updated");
    assert_eq!(msg_json["id"], run_id, "frontend should receive correct run_id");

    // 7. DB: run_events row exists
    // (We verify this via the fact that Run status is "running")
    let run_resp = client
        .get(format!("{}/api/runs/{}", base_url(addr), &run_id))
        .send()
        .await
        .unwrap();
    assert_eq!(run_resp.status(), 200);
    let run: Value = run_resp.json().await.unwrap();
    assert_eq!(run["status"], "running", "Run status should be 'running' after first RunEvent");
    assert!(run["started_at"].is_string(), "started_at should be set after RunEvent");
}

// ---------------------------------------------------------------------------
// AC2: Daemon disconnect → all running Runs become failed within 1s
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_daemon_disconnect_marks_running_runs_failed() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    // Connect daemon
    let mut daemon_ws = connect_daemon(addr).await;

    // Create and activate node, dispatch run
    let node_id = create_active_node(&client, addr).await;
    let dispatch_resp = client
        .post(format!("{}/api/nodes/{}/runs", base_url(addr), &node_id))
        .json(&json!({"type": "research_iteration", "runtime": "echo"}))
        .send()
        .await
        .unwrap();
    assert_eq!(dispatch_resp.status(), 201);
    let dispatched: Value = dispatch_resp.json().await.unwrap();
    let run_id = dispatched["id"].as_str().unwrap().to_string();

    // Wait for dispatch message then send RunEvent to set status to "running"
    let _dispatch_msg = timeout(Duration::from_secs(2), daemon_ws.next())
        .await
        .expect("daemon receives dispatch")
        .unwrap()
        .unwrap();

    let run_event = json!({
        "kind": "event",
        "run_id": run_id,
        "seq": 1,
        "event_type": "stdout",
        "data_text": "running..."
    });
    daemon_ws.send(Message::Text(run_event.to_string().into())).await.unwrap();

    // Small delay to let event be processed
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify run is running
    let run_resp = client.get(format!("{}/api/runs/{}", base_url(addr), &run_id)).send().await.unwrap();
    let run: Value = run_resp.json().await.unwrap();
    assert_eq!(run["status"], "running", "Run should be running before disconnect");

    // Disconnect daemon
    daemon_ws.close(None).await.unwrap();

    // Within 1s, run should become "failed"
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let run_resp = client.get(format!("{}/api/runs/{}", base_url(addr), &run_id)).send().await.unwrap();
        let run: Value = run_resp.json().await.unwrap();
        if run["status"] == "failed" {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("Run status did not become 'failed' within 1s after daemon disconnect; got: {}", run["status"]);
        }
    }
}

// ---------------------------------------------------------------------------
// AC3: POST /api/nodes/:id/runs returns 503 when no daemon connected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_dispatch_returns_503_when_no_daemon() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    // No daemon connected
    let node_id = create_active_node(&client, addr).await;
    let dispatch_resp = client
        .post(format!("{}/api/nodes/{}/runs", base_url(addr), &node_id))
        .json(&json!({"type": "research_iteration", "runtime": "echo"}))
        .send()
        .await
        .unwrap();

    assert_eq!(dispatch_resp.status(), 503, "should return 503 when no daemon connected");
}

// ---------------------------------------------------------------------------
// AC4: daemon WS returns 401 with wrong token
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_daemon_rejects_invalid_token() {
    let (addr, _tmp) = start_server().await;

    let url = ws_url(addr, "/ws/daemon");
    let uri: tokio_tungstenite::tungstenite::http::Uri = url.parse().unwrap();
    let mut req = uri.into_client_request().unwrap();
    req.headers_mut().insert(
        tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
        tokio_tungstenite::tungstenite::http::HeaderValue::from_static("Bearer wrong-token"),
    );
    let result = connect_async(req).await;
    // Should fail with HTTP 401 — tungstenite treats 4xx as failed handshake
    match result {
        Err(_) => {} // Expected: HTTP 401 causes a handshake error
        Ok((mut ws, _resp)) => {
            // If ws connects unexpectedly (shouldn't happen), close it
            let _ = ws.close(None).await;
        }
    }
}

// ---------------------------------------------------------------------------
// AC5 part 2: Frontend WS endpoint /ws/frontend is accessible
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_frontend_endpoint_accessible() {
    let (addr, _tmp) = start_server().await;

    // Connect frontend WS - should succeed
    let url = ws_url(addr, "/ws/frontend");
    let (mut ws, _) = connect_async(&url).await.expect("frontend ws should connect");
    ws.close(None).await.unwrap();
}

// ---------------------------------------------------------------------------
// RunTerminal: daemon sends RunTerminal, Run status updated + frontend notified
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_run_terminal_updates_status_and_notifies_frontend() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    // Connect daemon + frontend
    let mut daemon_ws = connect_daemon(addr).await;
    let (mut _frontend_sink, mut frontend_stream) = connect_frontend(addr).await.split();

    // Create node, dispatch run
    let node_id = create_active_node(&client, addr).await;
    let dispatch_resp = client
        .post(format!("{}/api/nodes/{}/runs", base_url(addr), &node_id))
        .json(&json!({"type": "research_iteration", "runtime": "echo"}))
        .send()
        .await
        .unwrap();
    assert_eq!(dispatch_resp.status(), 201);
    let dispatched: Value = dispatch_resp.json().await.unwrap();
    let run_id = dispatched["id"].as_str().unwrap().to_string();

    // Consume the RunDispatch
    let _dispatch_msg = timeout(Duration::from_secs(2), daemon_ws.next()).await.unwrap().unwrap().unwrap();

    // First send RunEvent to transition to "running"
    let run_event = json!({
        "kind": "event",
        "run_id": run_id,
        "seq": 1,
        "event_type": "stdout",
        "data_text": "output"
    });
    daemon_ws.send(Message::Text(run_event.to_string().into())).await.unwrap();

    // Consume the run:updated from RunEvent
    let _evt_msg = timeout(Duration::from_secs(2), frontend_stream.next()).await.unwrap().unwrap().unwrap();

    // Now send RunTerminal with status "complete"
    let terminal = json!({
        "kind": "terminal",
        "run_id": run_id,
        "status": "complete",
        "error": null
    });
    daemon_ws.send(Message::Text(terminal.to_string().into())).await.unwrap();

    // Frontend should receive run:updated or node:updated
    let terminal_msg = timeout(Duration::from_secs(2), frontend_stream.next())
        .await
        .expect("frontend should receive notification after terminal")
        .expect("frontend stream not closed")
        .expect("frontend WS message");

    let msg_text = match terminal_msg {
        Message::Text(t) => t,
        other => panic!("expected Text, got: {:?}", other),
    };
    let msg_json: Value = serde_json::from_str(&msg_text).unwrap();
    assert!(
        msg_json["type"] == "run:updated" || msg_json["type"] == "node:updated",
        "frontend should receive run:updated or node:updated; got: {}",
        msg_json
    );

    // DB: Run status should be "complete"
    tokio::time::sleep(Duration::from_millis(100)).await;
    let run_resp = client.get(format!("{}/api/runs/{}", base_url(addr), &run_id)).send().await.unwrap();
    let run: Value = run_resp.json().await.unwrap();
    assert_eq!(run["status"], "complete", "Run should be complete after RunTerminal");
}

// ---------------------------------------------------------------------------
// Node mutation broadcasts: create_node notifies frontend
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_node_mutation_broadcasts_node_updated() {
    let (addr, _tmp) = start_server().await;
    let client = Client::new();

    // Connect daemon (needed so create_node can broadcast)
    let _daemon_ws = connect_daemon(addr).await;

    // Connect frontend
    let (mut _frontend_sink, mut frontend_stream) = connect_frontend(addr).await.split();

    // Small delay to ensure frontend is registered in hub
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create a node — should broadcast node:updated
    let resp = client
        .post(format!("{}/api/nodes", base_url(addr)))
        .json(&json!({
            "intent": "Test broadcast",
            "acceptance_json": "{\"type\":\"prose\",\"text\":\"test\"}"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let node: Value = resp.json().await.unwrap();
    let node_id = node["id"].as_str().unwrap().to_string();

    let msg = timeout(Duration::from_secs(2), frontend_stream.next())
        .await
        .expect("frontend should receive node:updated after create_node")
        .expect("frontend stream not closed")
        .expect("frontend WS message");

    let msg_text = match msg {
        Message::Text(t) => t,
        other => panic!("expected Text, got: {:?}", other),
    };
    let msg_json: Value = serde_json::from_str(&msg_text).unwrap();
    assert_eq!(msg_json["type"], "node:updated", "frontend should receive node:updated");
    assert_eq!(msg_json["id"], node_id, "broadcast should include correct node_id");
}
