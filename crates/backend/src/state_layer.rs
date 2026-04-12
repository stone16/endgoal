//! State Layer — `state_at()` implementation for CP06.
//!
//! Computes `NodeState` for a given node by:
//! 1. Fetching the node row and its latest completed Run's `output_json`
//! 2. Computing `progress` via the locked formula:
//!    `(assertions_pass_rate × 0.40 + metric_achievement × 0.40 + rubric_normalized × 0.20) × 100`
//! 3. Computing `confidence` = `avg(score / scale)` across rubric dimensions
//! 4. Generating / caching `next_step` via `LlmClient::complete()`
//! 5. Assembling `parent_context` via recursive CTE (ancestors, root-first)
//! 6. Computing `rollup_blockers` for children up to `rollup_depth`
//! 7. Including `effective_policy`

use sqlx::sqlite::SqlitePool;

use crate::errors::AppError;
use crate::llm::LlmClient;
use crate::shared::types::{
    Acceptance, AncestorSummary, Assertion, AssertionStatus, Metric, NodeState, Phase, Policy,
    RubricDimension, RunOutput,
};

// ---------------------------------------------------------------------------
// Row types for DB queries
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct NodeStateRow {
    #[allow(dead_code)]
    id: String,
    intent: String,
    phase: String,
    acceptance_json: String,
    canonical_artifact_text: Option<String>,
    canonical_updated_by_run_id: Option<String>,
    next_step_cache: Option<String>,
    next_step_cache_for_run_id: Option<String>,
    #[allow(dead_code)]
    local_policy_json: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct RunOutputRow {
    output_json: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct AncestorRow {
    id: String,
    intent: String,
    phase: String,
    acceptance_json: String,
    canonical_artifact_text: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ChildRow {
    id: String,
    phase: String,
}

#[derive(Debug, sqlx::FromRow)]
struct PolicyRow {
    local_policy_json: Option<String>,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Compute the full `NodeState` for a node.
///
/// - `pool`: SQLite connection pool
/// - `node_id`: the node to compute state for
/// - `rollup_depth`: how many levels of children to inspect for blockers
/// - `llm`: injectable LLM client (use `StubLlmClient` in tests)
pub async fn state_at(
    pool: &SqlitePool,
    node_id: &str,
    rollup_depth: u8,
    llm: &dyn LlmClient,
) -> Result<NodeState, AppError> {
    // 1. Fetch the node row
    let node = sqlx::query_as::<_, NodeStateRow>(
        "SELECT id, intent, phase, acceptance_json,
                canonical_artifact_text, canonical_updated_by_run_id,
                next_step_cache, next_step_cache_for_run_id,
                local_policy_json
         FROM nodes WHERE id = ?",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("node {node_id} not found")))?;

    // 2. Fetch latest completed Run's output_json
    let run_output_row = sqlx::query_as::<_, RunOutputRow>(
        "SELECT output_json FROM runs
         WHERE node_id = ? AND status = 'completed'
         ORDER BY ended_at DESC
         LIMIT 1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await?;

    // 3. Parse RunOutput if present
    let run_output: Option<RunOutput> = run_output_row
        .and_then(|r| r.output_json)
        .and_then(|json_str| parse_run_output(&node.acceptance_json, &json_str));

    // 4. Compute progress and confidence
    let (progress, confidence) =
        compute_progress_and_confidence(&node.acceptance_json, &run_output);

    // 5. Compute/cache next_step
    let next_step = compute_next_step(pool, node_id, &node, llm).await?;

    // 6. Compute effective_policy
    let effective_policy = compute_effective_policy(pool, node_id).await?;

    // 7. Compute rollup_blockers
    let rollup_blockers = if rollup_depth > 0 {
        compute_rollup_blockers(pool, node_id, rollup_depth).await?
    } else {
        vec![]
    };

    // 8. Parse phase
    let phase: Phase = node
        .phase
        .parse()
        .map_err(|e: String| AppError::Internal(e))?;

    Ok(NodeState {
        state: phase,
        progress,
        confidence,
        next_step,
        effective_policy,
        rollup_blockers,
    })
}

fn parse_run_output(acceptance_json: &str, output_json: &str) -> Option<RunOutput> {
    if let Ok(output) = serde_json::from_str::<RunOutput>(output_json) {
        return Some(output);
    }

    let value: serde_json::Value = serde_json::from_str(output_json).ok()?;
    let structured = match serde_json::from_str::<Acceptance>(acceptance_json).ok()? {
        Acceptance::Structured(structured) => structured,
        Acceptance::Prose { .. } => return None,
    };

    let assertion_results = value
        .get("assertion_results")
        .and_then(serde_json::Value::as_object)
        .map(|results| {
            results
                .iter()
                .map(|(id, status)| {
                    let declared = structured
                        .assertions
                        .iter()
                        .find(|assertion| assertion.id == *id);
                    Assertion {
                        id: id.clone(),
                        text: declared
                            .map(|assertion| assertion.text.clone())
                            .unwrap_or_else(|| id.clone()),
                        check_fn: declared.and_then(|assertion| assertion.check_fn.clone()),
                        status: assertion_status_from_value(status),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let metric_values = value
        .get("metric_values")
        .and_then(serde_json::Value::as_object)
        .map(|results| {
            results
                .iter()
                .map(|(id, current)| {
                    let declared = structured.metrics.iter().find(|metric| metric.id == *id);
                    Metric {
                        id: id.clone(),
                        name: declared
                            .map(|metric| metric.name.clone())
                            .unwrap_or_else(|| id.clone()),
                        baseline: declared.and_then(|metric| metric.baseline),
                        current: current.as_f64(),
                        target: declared.map(|metric| metric.target).unwrap_or(100.0),
                        unit: declared.and_then(|metric| metric.unit.clone()),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let rubric_scores = value
        .get("rubric_scores")
        .and_then(serde_json::Value::as_object)
        .map(|results| {
            results
                .iter()
                .map(|(id, score)| {
                    let declared = structured.rubric.iter().find(|rubric| rubric.id == *id);
                    RubricDimension {
                        id: id.clone(),
                        dimension: declared
                            .map(|rubric| rubric.dimension.clone())
                            .unwrap_or_else(|| id.clone()),
                        score: score.as_f64(),
                        scale: declared.map(|rubric| rubric.scale).unwrap_or(10.0),
                        description: declared.and_then(|rubric| rubric.description.clone()),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Some(RunOutput {
        findings: value
            .get("findings")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        concerns: value
            .get("concerns")
            .and_then(serde_json::Value::as_array)
            .map(|concerns| {
                concerns
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        confidence: value
            .get("confidence")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
        needs_human_review: value
            .get("needs_human_review")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        assertion_results,
        metric_values,
        rubric_scores,
    })
}

fn assertion_status_from_value(value: &serde_json::Value) -> AssertionStatus {
    match value.as_str() {
        Some("pass") => AssertionStatus::Pass,
        Some("fail") => AssertionStatus::Fail,
        _ => AssertionStatus::Pending,
    }
}

// ---------------------------------------------------------------------------
// Progress and confidence computation
// ---------------------------------------------------------------------------

/// Compute `(progress, confidence)` from acceptance_json and optional RunOutput.
///
/// Uses the locked formula (pseudo-code):
/// ```text
/// progress = (
///   assertions_pass_rate * 0.40 +
///   metric_achievement   * 0.40 +
///   rubric_normalized    * 0.20
/// ) * 100
/// ```
/// with optional-layer weight redistribution when layers are empty.
fn compute_progress_and_confidence(
    acceptance_json: &str,
    run_output: &Option<RunOutput>,
) -> (f64, f64) {
    let Some(run_output) = run_output else {
        return (0.0, 0.0);
    };

    // Parse the structured acceptance to understand the spec's layers.
    // RunOutput contains the actual results.
    let assertion_results = &run_output.assertion_results;
    let metric_values = &run_output.metric_values;
    let rubric_scores = &run_output.rubric_scores;

    // Parse acceptance to understand the "declared" layers for weight computation.
    // We use run_output's actual data for the scoring, which is what the daemon reports.
    let acceptance: Option<crate::shared::types::Acceptance> =
        serde_json::from_str(acceptance_json).ok();
    let declared_structured = match acceptance {
        Some(crate::shared::types::Acceptance::Structured(s)) => Some(s),
        _ => None,
    };

    // Determine which layers are "present" (have items).
    // Use the declared acceptance as the ground truth for presence, but score from run_output.
    // If no declared structured acceptance, fall back to run_output counts.
    let has_assertions = if let Some(ref s) = declared_structured {
        !s.assertions.is_empty()
    } else {
        !assertion_results.is_empty()
    };
    let has_metrics = if let Some(ref s) = declared_structured {
        !s.metrics.is_empty()
    } else {
        !metric_values.is_empty()
    };
    let has_rubric = if let Some(ref s) = declared_structured {
        !s.rubric.is_empty()
    } else {
        !rubric_scores.is_empty()
    };

    // Compute raw scores for each present layer
    let assertions_pass_rate = if has_assertions && !assertion_results.is_empty() {
        let pass_count = assertion_results
            .iter()
            .filter(|a| a.status == AssertionStatus::Pass)
            .count();
        pass_count as f64 / assertion_results.len() as f64
    } else if has_assertions {
        // Declared but no results yet
        0.0
    } else {
        0.0
    };

    let metric_achievement = if has_metrics && !metric_values.is_empty() {
        let sum: f64 = metric_values
            .iter()
            .map(|m| {
                let current = m.current.unwrap_or(0.0);
                if m.target > 0.0 {
                    (current / m.target).min(1.0)
                } else {
                    0.0
                }
            })
            .sum();
        sum / metric_values.len() as f64
    } else {
        0.0
    };

    let rubric_normalized = if has_rubric && !rubric_scores.is_empty() {
        let sum: f64 = rubric_scores
            .iter()
            .map(|r| {
                let score = r.score.unwrap_or(0.0);
                if r.scale > 0.0 {
                    (score / r.scale).min(1.0)
                } else {
                    0.0
                }
            })
            .sum();
        sum / rubric_scores.len() as f64
    } else {
        0.0
    };

    // Weight redistribution for optional layers
    let progress = match (has_assertions, has_metrics, has_rubric) {
        (false, false, false) => 0.0,
        (true, false, false) => assertions_pass_rate * 100.0,
        (false, true, false) => metric_achievement * 100.0,
        (false, false, true) => rubric_normalized * 100.0,
        (true, true, false) => (assertions_pass_rate * 0.5 + metric_achievement * 0.5) * 100.0,
        (true, false, true) => (assertions_pass_rate * 0.667 + rubric_normalized * 0.333) * 100.0,
        (false, true, true) => (metric_achievement * 0.667 + rubric_normalized * 0.333) * 100.0,
        (true, true, true) => {
            (assertions_pass_rate * 0.40 + metric_achievement * 0.40 + rubric_normalized * 0.20)
                * 100.0
        }
    };

    // Confidence = avg(score / scale) across rubric dimensions
    let confidence = if !rubric_scores.is_empty() {
        let sum: f64 = rubric_scores
            .iter()
            .map(|r| {
                let score = r.score.unwrap_or(0.0);
                if r.scale > 0.0 { score / r.scale } else { 0.0 }
            })
            .sum();
        sum / rubric_scores.len() as f64
    } else {
        0.0
    };

    (progress, confidence)
}

// ---------------------------------------------------------------------------
// next_step caching
// ---------------------------------------------------------------------------

async fn compute_next_step(
    pool: &SqlitePool,
    node_id: &str,
    node: &NodeStateRow,
    llm: &dyn LlmClient,
) -> Result<String, AppError> {
    // Check if cache is still valid:
    // Cache is valid when next_step_cache is Some AND
    // next_step_cache_for_run_id == canonical_updated_by_run_id
    let cache_valid = match (
        &node.next_step_cache,
        &node.next_step_cache_for_run_id,
        &node.canonical_updated_by_run_id,
    ) {
        (Some(_cached), Some(cache_run_id), Some(canon_run_id)) => cache_run_id == canon_run_id,
        (Some(cached), None, None) => {
            // Cache exists and canonical hasn't been set by any run yet — stale, regenerate
            // But if canonical_artifact_text is null, return early message instead
            let _ = cached;
            false
        }
        _ => false,
    };

    if cache_valid {
        return Ok(node.next_step_cache.clone().unwrap());
    }

    // Generate new next_step
    let next_step = if node.canonical_artifact_text.is_none() {
        // No completed runs yet — return static message without LLM call
        "No runs completed yet — dispatch a research_iteration Run to begin.".to_string()
    } else {
        // Build prompt from node intent + canonical summary
        let canon = node.canonical_artifact_text.as_deref().unwrap_or("");
        let prompt = format!(
            "You are helping plan the next step for a research node.\n\
             Node intent: {}\n\
             Canonical summary: {}\n\
             What should be the immediate next step?",
            node.intent, canon
        );
        llm.complete(&prompt).await?
    };

    // Write back to cache
    let cache_run_id = node.canonical_updated_by_run_id.clone();
    sqlx::query(
        "UPDATE nodes SET next_step_cache = ?, next_step_cache_for_run_id = ?, updated_at = ? WHERE id = ?"
    )
    .bind(&next_step)
    .bind(&cache_run_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(node_id)
    .execute(pool)
    .await?;

    Ok(next_step)
}

// ---------------------------------------------------------------------------
// Effective policy computation
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Parent context assembly
// ---------------------------------------------------------------------------

/// Assemble the ancestor chain for a node (root-first, excluding self).
/// Returns `Vec<AncestorSummary>` with truncated acceptance_summary and canonical_summary.
pub async fn assemble_parent_context(
    pool: &SqlitePool,
    node_id: &str,
) -> Result<Vec<AncestorSummary>, AppError> {
    let rows: Vec<AncestorRow> = sqlx::query_as::<_, AncestorRow>(
        "WITH RECURSIVE ancestors(id, intent, phase, acceptance_json, canonical_artifact_text, parent_id, depth) AS (
            -- Start from the node's parent (exclude self)
            SELECT n.id, n.intent, n.phase, n.acceptance_json, n.canonical_artifact_text, n.parent_id, 1
            FROM nodes n
            INNER JOIN nodes child ON child.parent_id = n.id
            WHERE child.id = ?
            UNION ALL
            SELECT p.id, p.intent, p.phase, p.acceptance_json, p.canonical_artifact_text, p.parent_id, a.depth + 1
            FROM nodes p
            INNER JOIN ancestors a ON a.parent_id = p.id
         )
         SELECT id, intent, phase, acceptance_json, canonical_artifact_text
         FROM ancestors
         ORDER BY depth DESC",
    )
    .bind(node_id)
    .fetch_all(pool)
    .await?;

    let summaries = rows
        .into_iter()
        .map(|r| {
            let phase = r.phase.parse::<Phase>().unwrap_or(Phase::Draft);
            // Truncate acceptance_summary to first 300 chars
            let acceptance_summary = r.acceptance_json.chars().take(300).collect::<String>();
            // Truncate canonical_summary to first 500 chars
            let canonical_summary = r
                .canonical_artifact_text
                .map(|t| t.chars().take(500).collect::<String>());
            AncestorSummary {
                id: r.id,
                intent: r.intent,
                phase,
                acceptance_summary,
                canonical_summary,
                progress: 0, // MVP: avoid infinite recursion
            }
        })
        .collect();

    Ok(summaries)
}

// ---------------------------------------------------------------------------
// Rollup blockers
// ---------------------------------------------------------------------------

/// Recursively check children up to `depth` levels for blocked children.
/// A child is blocked when: phase==Active AND zero completed Runs AND progress==0.
async fn compute_rollup_blockers(
    pool: &SqlitePool,
    node_id: &str,
    depth: u8,
) -> Result<Vec<String>, AppError> {
    let mut blockers = vec![];
    collect_blockers(pool, node_id, depth, &mut blockers).await?;
    Ok(blockers)
}

fn collect_blockers<'a>(
    pool: &'a SqlitePool,
    node_id: &'a str,
    depth: u8,
    blockers: &'a mut Vec<String>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AppError>> + Send + 'a>> {
    Box::pin(async move {
        if depth == 0 {
            return Ok(());
        }

        // Fetch direct children
        let children: Vec<ChildRow> =
            sqlx::query_as::<_, ChildRow>("SELECT id, phase FROM nodes WHERE parent_id = ?")
                .bind(node_id)
                .fetch_all(pool)
                .await?;

        for child in children {
            let phase = child.phase.parse::<Phase>().unwrap_or(Phase::Draft);

            if phase == Phase::Active {
                // Check for completed runs
                let completed_count: i64 = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM runs WHERE node_id = ? AND status = 'completed'",
                )
                .bind(&child.id)
                .fetch_one(pool)
                .await?;

                if completed_count == 0 {
                    // Also check progress — since no completed runs, progress must be 0
                    // (we can't easily compute state_at here without infinite recursion,
                    // but with 0 completed runs, progress is always 0 by the formula)
                    blockers.push(child.id.clone());
                }
            }

            // Recurse into children
            collect_blockers(pool, &child.id, depth - 1, blockers).await?;
        }

        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::types::{Assertion, AssertionStatus, Metric, RubricDimension};

    fn make_output(
        assertions: Vec<(AssertionStatus,)>,
        metrics: Vec<(f64, f64)>, // (current, target)
        rubric: Vec<(f64, f64)>,  // (score, scale)
    ) -> RunOutput {
        RunOutput {
            findings: "test".to_string(),
            concerns: vec![],
            confidence: 0.0,
            needs_human_review: false,
            assertion_results: assertions
                .into_iter()
                .enumerate()
                .map(|(i, (status,))| Assertion {
                    id: format!("a{i}"),
                    text: "test".to_string(),
                    check_fn: None,
                    status,
                })
                .collect(),
            metric_values: metrics
                .into_iter()
                .enumerate()
                .map(|(i, (current, target))| Metric {
                    id: format!("m{i}"),
                    name: "test".to_string(),
                    baseline: None,
                    current: Some(current),
                    target,
                    unit: None,
                })
                .collect(),
            rubric_scores: rubric
                .into_iter()
                .enumerate()
                .map(|(i, (score, scale))| RubricDimension {
                    id: format!("r{i}"),
                    dimension: "test".to_string(),
                    score: Some(score),
                    scale,
                    description: None,
                })
                .collect(),
        }
    }

    // AC1: 2 pass / 1 fail assertions + metric at 60% + rubric 7/10 → progress in [63, 67]
    #[test]
    fn test_progress_formula_ac1() {
        let acceptance_json = r#"{"type":"structured","assertions":[
            {"id":"a1","text":"p1","status":"pending"},
            {"id":"a2","text":"p2","status":"pending"},
            {"id":"a3","text":"f1","status":"pending"}
        ],"metrics":[
            {"id":"m1","name":"cov","target":100.0,"unit":"%"}
        ],"rubric":[
            {"id":"r1","dimension":"q","scale":10.0}
        ]}"#;

        let output = make_output(
            vec![
                (AssertionStatus::Pass,),
                (AssertionStatus::Pass,),
                (AssertionStatus::Fail,),
            ],
            vec![(60.0, 100.0)],
            vec![(7.0, 10.0)],
        );

        let (progress, confidence) =
            compute_progress_and_confidence(acceptance_json, &Some(output));

        // Expected: (0.667 * 0.40 + 0.60 * 0.40 + 0.70 * 0.20) * 100 = 64.67
        assert!(
            (63.0..=67.0).contains(&progress),
            "progress {progress} should be in [63, 67]"
        );
        assert!(
            (confidence - 0.7).abs() < 0.01,
            "confidence {confidence} should be ~0.7"
        );
    }

    // AC2: All-passing fixture → progress == 100
    #[test]
    fn test_progress_all_passing_100() {
        let acceptance_json = r#"{"type":"structured","assertions":[
            {"id":"a1","text":"p1","status":"pending"}
        ],"metrics":[
            {"id":"m1","name":"cov","target":100.0,"unit":"%"}
        ],"rubric":[
            {"id":"r1","dimension":"q","scale":10.0}
        ]}"#;

        let output = make_output(
            vec![(AssertionStatus::Pass,)],
            vec![(100.0, 100.0)],
            vec![(10.0, 10.0)],
        );

        let (progress, confidence) =
            compute_progress_and_confidence(acceptance_json, &Some(output));

        assert_eq!(progress, 100.0, "all-passing should yield progress=100");
        assert!(
            (confidence - 1.0).abs() < 0.01,
            "all-passing rubric should yield confidence=1.0"
        );
    }

    // Assertions only → weight 1.0
    #[test]
    fn test_progress_assertions_only() {
        let acceptance_json = r#"{"type":"structured","assertions":[
            {"id":"a1","text":"p","status":"pending"},
            {"id":"a2","text":"f","status":"pending"}
        ],"metrics":[],"rubric":[]}"#;

        let output = make_output(
            vec![(AssertionStatus::Pass,), (AssertionStatus::Fail,)],
            vec![],
            vec![],
        );

        let (progress, confidence) =
            compute_progress_and_confidence(acceptance_json, &Some(output));

        assert!(
            (progress - 50.0).abs() < 0.01,
            "50% pass rate with assertions-only should be 50.0, got {progress}"
        );
        assert_eq!(confidence, 0.0, "no rubric means confidence=0");
    }

    #[test]
    fn test_parse_run_output_accepts_smoke_result_maps() {
        let acceptance_json = r#"{"type":"structured","assertions":[
            {"id":"a1","text":"p","status":"pending"}
        ],"metrics":[
            {"id":"m1","name":"coverage","target":100.0,"unit":"%"}
        ],"rubric":[
            {"id":"r1","dimension":"quality","scale":10.0}
        ]}"#;
        let output_json = r#"{
            "assertion_results": {"a1": "pass"},
            "metric_values": {"m1": 80},
            "rubric_scores": {"r1": 8},
            "confidence": 0.8,
            "findings": "smoke test pass",
            "concerns": [],
            "needs_human_review": false
        }"#;

        let output = parse_run_output(acceptance_json, output_json).expect("run output");
        assert_eq!(output.assertion_results[0].status, AssertionStatus::Pass);
        assert_eq!(output.metric_values[0].current, Some(80.0));
        assert_eq!(output.rubric_scores[0].score, Some(8.0));

        let (progress, confidence) =
            compute_progress_and_confidence(acceptance_json, &Some(output));

        assert!(
            (progress - 88.0).abs() < 0.01,
            "smoke maps should compute 88 progress, got {progress}"
        );
        assert!((confidence - 0.8).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_assemble_parent_context_depth3() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let db_path = tmp.path().join("test.db");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = crate::create_pool(&db_url).await.expect("pool");
        crate::run_migrations(&pool).await.expect("migrations");
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO nodes (id, intent, phase, acceptance_json, created_at, updated_at)
             VALUES ('root', 'Root', 'active', '{\"type\":\"structured\",\"assertions\":[],\"metrics\":[],\"rubric\":[]}', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert root");
        sqlx::query(
            "INSERT INTO nodes (id, intent, parent_id, phase, acceptance_json, created_at, updated_at)
             VALUES ('mid', 'Mid', 'root', 'active', '{\"type\":\"structured\",\"assertions\":[],\"metrics\":[],\"rubric\":[]}', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert mid");
        sqlx::query(
            "INSERT INTO nodes (id, intent, parent_id, phase, acceptance_json, created_at, updated_at)
             VALUES ('leaf', 'Leaf', 'mid', 'active', '{\"type\":\"structured\",\"assertions\":[],\"metrics\":[],\"rubric\":[]}', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert leaf");

        let context = assemble_parent_context(&pool, "leaf")
            .await
            .expect("parent context");

        assert_eq!(context.len(), 2);
        assert_eq!(context[0].id, "root");
        assert_eq!(context[1].id, "mid");
    }

    // No completed run → progress=0, confidence=0
    #[test]
    fn test_progress_no_run() {
        let acceptance_json = r#"{"type":"structured","assertions":[
            {"id":"a1","text":"p","status":"pending"}
        ],"metrics":[],"rubric":[]}"#;

        let (progress, confidence) = compute_progress_and_confidence(acceptance_json, &None);

        assert_eq!(progress, 0.0);
        assert_eq!(confidence, 0.0);
    }

    // Prose acceptance → progress=0 (no run), confidence=0
    #[test]
    fn test_progress_prose_acceptance_no_run() {
        let acceptance_json = r#"{"type":"prose","text":"vague goal"}"#;
        let (progress, confidence) = compute_progress_and_confidence(acceptance_json, &None);
        assert_eq!(progress, 0.0);
        assert_eq!(confidence, 0.0);
    }
}
