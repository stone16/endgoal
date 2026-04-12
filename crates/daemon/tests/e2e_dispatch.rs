//! E2E integration test:
//! 1. Starts a backend WS stub (axum server with /ws/daemon endpoint)
//! 2. Connects the daemon WS client logic
//! 3. Sends a RunDispatch from the backend to the daemon
//! 4. Asserts RunEvent + RunTerminal received by the backend
//! 5. Verifies scratchpad directory was created

use std::sync::Arc;
use tokio::sync::Mutex;

use axum::{
    Router,
    extract::WebSocketUpgrade,
    response::IntoResponse,
    routing::any,
    extract::ws,
};
use endgoal_daemon::shared::types::*;
use futures::SinkExt;

/// Shared state to collect messages received by the backend stub.
#[derive(Clone, Default)]
struct BackendState {
    received: Arc<Mutex<Vec<WsDaemonMessage>>>,
    dispatch_to_send: Arc<Mutex<Option<RunDispatch>>>,
}

/// Backend WS handler that sends a RunDispatch and collects responses.
async fn ws_handler(
    ws: WebSocketUpgrade,
    state: axum::extract::State<BackendState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state.0))
}

async fn handle_ws(mut socket: ws::WebSocket, state: BackendState) {
    // Send the RunDispatch to the daemon
    if let Some(dispatch) = state.dispatch_to_send.lock().await.take() {
        let json = serde_json::to_string(&dispatch).unwrap();
        if let Err(e) = socket.send(ws::Message::Text(json.into())).await {
            eprintln!("[test-backend] Failed to send dispatch: {e}");
            return;
        }
    }

    // Collect all responses from the daemon
    while let Some(msg_result) = socket.recv().await {
        match msg_result {
            Ok(ws::Message::Text(text)) => {
                match serde_json::from_str::<WsDaemonMessage>(&text) {
                    Ok(daemon_msg) => {
                        let is_terminal = matches!(&daemon_msg, WsDaemonMessage::Terminal(_));
                        state.received.lock().await.push(daemon_msg);
                        if is_terminal {
                            let _ = socket.close().await;
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("[test-backend] Parse error: {e}, text: {text}");
                    }
                }
            }
            Ok(ws::Message::Close(_)) => break,
            Ok(_) => {} // ignore ping/pong/binary
            Err(e) => {
                eprintln!("[test-backend] recv error: {e}");
                break;
            }
        }
    }
}

fn make_dispatch(run_id: &str, runtime: &str, intent: &str) -> RunDispatch {
    RunDispatch {
        run_id: run_id.to_string(),
        runtime: runtime.to_string(),
        input: RunInput {
            intent: intent.to_string(),
            acceptance: Acceptance::Prose {
                text: "e2e test".to_string(),
            },
            effective_policy: Policy {
                tokens_max: None,
                iterations_max: None,
                wallclock_max_s: None,
                allowed_tools: None,
                review_required: None,
            },
            parent_context: vec![],
            node_docs: vec![],
        },
    }
}

/// Helper: start test backend, connect daemon, wait for terminal, return received messages.
async fn run_e2e_scenario(
    dispatch: RunDispatch,
    scratchpad_root: std::path::PathBuf,
) -> Vec<WsDaemonMessage> {
    let state = BackendState {
        received: Arc::new(Mutex::new(Vec::new())),
        dispatch_to_send: Arc::new(Mutex::new(Some(dispatch))),
    };

    // Start the backend stub
    let app = Router::new()
        .route("/ws/daemon", any(ws_handler))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Connect the daemon client, but instead of using run_daemon_client (which
    // includes its own WS connect logic), we'll use handle_dispatch directly
    // for the unit-level E2E, or we can use the full WS flow.
    let ws_url = format!("ws://127.0.0.1:{}/ws/daemon", addr.port());
    let client_result = Arc::new(Mutex::new(None::<String>));
    let client_result_clone = client_result.clone();

    let client_handle = tokio::spawn({
        let scratchpad_root = scratchpad_root.clone();
        async move {
            let result = endgoal_daemon::ws_client::run_daemon_client(
                &ws_url,
                "dev-token",
                Some(scratchpad_root),
            )
            .await;
            if let Err(e) = &result {
                *client_result_clone.lock().await = Some(format!("{e}"));
            }
            result
        }
    });

    // Wait for the terminal message to appear (with timeout)
    let timeout = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            // Check if client errored
            if let Some(err) = client_result.lock().await.as_ref() {
                panic!("Daemon client failed: {err}");
            }
            let received = state.received.lock().await;
            let has_terminal = received.iter().any(|m| matches!(m, WsDaemonMessage::Terminal(_)));
            if has_terminal {
                break;
            }
            drop(received);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });

    timeout.await.expect("timed out waiting for terminal message");

    let result = state.received.lock().await.clone();

    // Cleanup
    client_handle.abort();
    server_handle.abort();

    result
}

#[tokio::test]
async fn test_e2e_echo_dispatch_produces_event_and_terminal() {
    let tmp = tempfile::tempdir().unwrap();
    let dispatch = make_dispatch("e2e-test-1", "echo", "hello");
    let received = run_e2e_scenario(dispatch, tmp.path().to_path_buf()).await;

    // Should have at least one Event and exactly one Terminal
    let events: Vec<_> = received
        .iter()
        .filter(|m| matches!(m, WsDaemonMessage::Event(_)))
        .collect();
    let terminals: Vec<_> = received
        .iter()
        .filter(|m| matches!(m, WsDaemonMessage::Terminal(_)))
        .collect();

    assert!(
        !events.is_empty(),
        "expected at least one RunEvent, got none"
    );
    assert_eq!(terminals.len(), 1, "expected exactly one RunTerminal");

    // Verify RunEvent content
    if let WsDaemonMessage::Event(event) = &events[0] {
        assert_eq!(event.run_id, "e2e-test-1");
        assert_eq!(event.event_type, "stdout");
        assert_eq!(event.data_text.as_deref(), Some("hello"));
    } else {
        panic!("expected Event variant");
    }

    // Verify RunTerminal content
    if let WsDaemonMessage::Terminal(terminal) = &terminals[0] {
        assert_eq!(terminal.run_id, "e2e-test-1");
        assert_eq!(terminal.status, "completed");
        assert!(terminal.error.is_none());
    } else {
        panic!("expected Terminal variant");
    }

    // Verify scratchpad directory was created
    let scratchpad_dir = tmp.path().join("run-e2e-test-1");
    assert!(
        scratchpad_dir.exists(),
        "scratchpad directory should exist at {:?}",
        scratchpad_dir
    );
    assert!(scratchpad_dir.is_dir());
}

#[tokio::test]
async fn test_e2e_nonexistent_runtime_returns_failed_terminal() {
    let tmp = tempfile::tempdir().unwrap();
    let dispatch = make_dispatch("e2e-fail-1", "nonexistent_runtime_xyz", "anything");
    let received = run_e2e_scenario(dispatch, tmp.path().to_path_buf()).await;

    let events: Vec<_> = received
        .iter()
        .filter(|m| matches!(m, WsDaemonMessage::Event(_)))
        .collect();
    let terminals: Vec<_> = received
        .iter()
        .filter(|m| matches!(m, WsDaemonMessage::Terminal(_)))
        .collect();

    assert_eq!(events.len(), 0, "expected no events for unknown runtime");
    assert_eq!(terminals.len(), 1, "expected one terminal message");

    if let WsDaemonMessage::Terminal(terminal) = &terminals[0] {
        assert_eq!(terminal.run_id, "e2e-fail-1");
        assert_eq!(terminal.status, "failed");
        assert!(
            terminal.error.as_ref().unwrap().contains("unknown runtime"),
            "expected unknown runtime error, got: {:?}",
            terminal.error
        );
    }
}

#[tokio::test]
async fn test_e2e_scratchpad_custom_root() {
    let tmp = tempfile::tempdir().unwrap();
    let custom_root = tmp.path().join("custom-scratchpads");
    std::fs::create_dir_all(&custom_root).unwrap();

    let dispatch = make_dispatch("e2e-custom-root", "echo", "hi");
    let received = run_e2e_scenario(dispatch, custom_root.clone()).await;

    // Verify scratchpad was created in the custom root
    let scratchpad_dir = custom_root.join("run-e2e-custom-root");
    assert!(
        scratchpad_dir.exists(),
        "scratchpad should be in custom root at {:?}",
        scratchpad_dir
    );

    // Verify completed
    let terminals: Vec<_> = received
        .iter()
        .filter(|m| matches!(m, WsDaemonMessage::Terminal(_)))
        .collect();
    assert_eq!(terminals.len(), 1);
    if let WsDaemonMessage::Terminal(terminal) = &terminals[0] {
        assert_eq!(terminal.status, "completed");
    }
}
