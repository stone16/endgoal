use endgoal_shared::*;
use sqlx::Row;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;

/// Returns the absolute path to the migrations directory.
fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    PathBuf::from(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("db")
        .join("migrations")
}

/// E2E test: apply migrations to a temp SQLite DB, insert a Node row via sqlx,
/// read it back, and assert all fields round-trip correctly.
#[tokio::test]
async fn e2e_node_insert_and_round_trip() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let db_path = tmp_dir.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .unwrap();

    // Apply migrations manually by reading the SQL file
    let migration_sql =
        std::fs::read_to_string(migrations_dir().join("001_initial_schema.sql")).unwrap();
    sqlx::raw_sql(&migration_sql).execute(&pool).await.unwrap();

    // Construct a Node with all fields populated
    let node = Node {
        id: "node-001".into(),
        intent: "Build the EndGoal MVP".into(),
        parent_id: None,
        phase: Phase::Draft,
        acceptance_json: serde_json::to_string(&Acceptance::Prose {
            text: "Ship it".into(),
        })
        .unwrap(),
        local_policy_json: Some(
            serde_json::to_string(&Policy {
                tokens_max: Some(100_000),
                iterations_max: Some(5),
                wallclock_max_s: Some(3600),
                allowed_tools: Some(vec!["search".into(), "code".into()]),
                review_required: Some(true),
            })
            .unwrap(),
        ),
        canonical_artifact_text: Some("Initial artifact".into()),
        canonical_updated_by_run_id: Some("run-abc".into()),
        next_step_cache: Some("Start coding".into()),
        next_step_cache_for_run_id: Some("run-abc".into()),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T12:00:00Z".into(),
    };

    // INSERT
    sqlx::query(
        r#"
        INSERT INTO nodes (
            id, intent, parent_id, phase, acceptance_json, local_policy_json,
            canonical_artifact_text, canonical_updated_by_run_id,
            next_step_cache, next_step_cache_for_run_id,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        "#,
    )
    .bind(&node.id)
    .bind(&node.intent)
    .bind(&node.parent_id)
    .bind(node.phase.to_string())
    .bind(&node.acceptance_json)
    .bind(&node.local_policy_json)
    .bind(&node.canonical_artifact_text)
    .bind(&node.canonical_updated_by_run_id)
    .bind(&node.next_step_cache)
    .bind(&node.next_step_cache_for_run_id)
    .bind(&node.created_at)
    .bind(&node.updated_at)
    .execute(&pool)
    .await
    .unwrap();

    // SELECT and reconstruct
    let row = sqlx::query("SELECT * FROM nodes WHERE id = ?1")
        .bind(&node.id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let read_back = Node {
        id: row.get("id"),
        intent: row.get("intent"),
        parent_id: row.get("parent_id"),
        phase: row.get::<String, _>("phase").parse().unwrap(),
        acceptance_json: row.get("acceptance_json"),
        local_policy_json: row.get("local_policy_json"),
        canonical_artifact_text: row.get("canonical_artifact_text"),
        canonical_updated_by_run_id: row.get("canonical_updated_by_run_id"),
        next_step_cache: row.get("next_step_cache"),
        next_step_cache_for_run_id: row.get("next_step_cache_for_run_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    };

    assert_eq!(read_back, node, "Node round-trip through SQLite failed");

    // Also verify the acceptance_json can be deserialized back
    let acceptance: Acceptance = serde_json::from_str(&read_back.acceptance_json).unwrap();
    assert_eq!(
        acceptance,
        Acceptance::Prose {
            text: "Ship it".into()
        }
    );

    // And policy
    let policy: Policy =
        serde_json::from_str(read_back.local_policy_json.as_ref().unwrap()).unwrap();
    assert_eq!(policy.tokens_max, Some(100_000));
    assert_eq!(policy.review_required, Some(true));

    pool.close().await;
}

/// Test: insert a Node with a child (parent_id FK works)
#[tokio::test]
async fn e2e_node_parent_child_relationship() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let db_path = tmp_dir.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .unwrap();

    let migration_sql =
        std::fs::read_to_string(migrations_dir().join("001_initial_schema.sql")).unwrap();
    sqlx::raw_sql(&migration_sql).execute(&pool).await.unwrap();

    // Insert parent
    sqlx::query(
        "INSERT INTO nodes (id, intent, phase, acceptance_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
    )
    .bind("parent-1")
    .bind("Parent goal")
    .bind("draft")
    .bind(r#"{"type":"prose","text":""}"#)
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&pool)
    .await
    .unwrap();

    // Insert child referencing parent
    sqlx::query(
        "INSERT INTO nodes (id, intent, parent_id, phase, acceptance_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    )
    .bind("child-1")
    .bind("Child goal")
    .bind("parent-1")
    .bind("draft")
    .bind(r#"{"type":"prose","text":""}"#)
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&pool)
    .await
    .unwrap();

    // Verify FK works by reading child
    let row = sqlx::query("SELECT parent_id FROM nodes WHERE id = ?1")
        .bind("child-1")
        .fetch_one(&pool)
        .await
        .unwrap();

    let parent_id: Option<String> = row.get("parent_id");
    assert_eq!(parent_id, Some("parent-1".into()));

    pool.close().await;
}

/// Test: all tables exist after migration
#[tokio::test]
async fn e2e_all_tables_created() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let db_path = tmp_dir.path().join("test.db");
    let db_url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .unwrap();

    let migration_sql =
        std::fs::read_to_string(migrations_dir().join("001_initial_schema.sql")).unwrap();
    sqlx::raw_sql(&migration_sql).execute(&pool).await.unwrap();

    let expected_tables = [
        "nodes",
        "node_docs",
        "review_log",
        "runs",
        "run_events",
        "freeze_sessions",
    ];

    for table in &expected_tables {
        let result = sqlx::query(&format!(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
            table
        ))
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(
            result.is_some(),
            "Table '{table}' was not created by migration"
        );
    }

    // Verify the index exists
    let idx = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_run_events_run_seq'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(idx.is_some(), "Index idx_run_events_run_seq not created");

    pool.close().await;
}
