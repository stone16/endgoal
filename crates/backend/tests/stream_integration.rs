//! Integration tests for run event SSE streaming (CP12).
//!
//! Test names are prefixed with `stream_` so `cargo test -- stream` matches
//! this checkpoint suite.

use reqwest::Client;
use serde_json::json;
use std::{net::SocketAddr, time::Duration};

async fn start_server_with_completed_run(run_id: &str) -> (SocketAddr, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = endgoal_backend::create_pool(&db_url).await.expect("pool");
    endgoal_backend::run_migrations(&pool)
        .await
        .expect("migrations");

    let now = "2026-04-12T00:00:00Z";
    sqlx::query(
        "INSERT INTO nodes (id, intent, phase, acceptance_json, created_at, updated_at)
         VALUES ('node-stream', 'Stream completed run', 'active',
                 '{\"type\":\"structured\",\"assertions\":[],\"metrics\":[],\"rubric\":[]}', ?, ?)",
    )
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .expect("insert node");

    let input_snapshot = json!({
        "intent": "Stream completed run",
        "acceptance": { "type": "structured", "assertions": [], "metrics": [], "rubric": [] },
        "effective_policy": {
            "tokens_max": null,
            "iterations_max": null,
            "wallclock_max_s": null,
            "allowed_tools": null,
            "review_required": null
        },
        "parent_context": [],
        "node_docs": []
    })
    .to_string();

    sqlx::query(
        "INSERT INTO runs (
             id, node_id, type, status, runtime, input_snapshot_json, started_at, ended_at, created_at
         ) VALUES (?, 'node-stream', 'research_iteration', 'completed', 'echo', ?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind(&input_snapshot)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .expect("insert run");

    for (seq, event_type, data_text) in [
        (1_i64, "stdout", Some("first line")),
        (2_i64, "stderr", Some("second line")),
        (3_i64, "system", Some("done")),
    ] {
        sqlx::query(
            "INSERT INTO run_events (id, run_id, seq, event_type, data_text, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("event-{seq}"))
        .bind(run_id)
        .bind(seq)
        .bind(event_type)
        .bind(data_text)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert run event");
    }

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
async fn stream_completed_run_replays_all_events_and_closes() {
    let run_id = "run-stream-completed";
    let (addr, _tmp) = start_server_with_completed_run(run_id).await;
    let client = Client::new();

    let response = client
        .get(format!("{}/api/runs/{}/stream", base_url(addr), run_id))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let stream_text = tokio::time::timeout(Duration::from_secs(2), response.text())
        .await
        .expect("completed run stream should close")
        .unwrap();

    assert!(stream_text.contains("event: stdout"), "{stream_text}");
    assert!(stream_text.contains("event: stderr"), "{stream_text}");
    assert!(stream_text.contains("event: system"), "{stream_text}");
    assert!(
        stream_text.contains(r#""run_id":"run-stream-completed""#),
        "{stream_text}"
    );
    assert!(stream_text.contains(r#""seq":1"#), "{stream_text}");
    assert!(
        stream_text.contains(r#""data_text":"first line""#),
        "{stream_text}"
    );
    assert!(
        stream_text.contains(r#""data_text":"second line""#),
        "{stream_text}"
    );
    assert!(
        stream_text.contains(r#""data_text":"done""#),
        "{stream_text}"
    );

    let first = stream_text.find("first line").expect("first event");
    let second = stream_text.find("second line").expect("second event");
    let third = stream_text.find("done").expect("third event");
    assert!(first < second && second < third, "{stream_text}");
}
