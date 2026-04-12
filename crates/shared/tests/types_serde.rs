use endgoal_shared::*;

// ---------------------------------------------------------------------------
// Phase round-trip
// ---------------------------------------------------------------------------

#[test]
fn phase_serde_round_trip() {
    let phases = vec![
        Phase::Draft,
        Phase::Active,
        Phase::InReview,
        Phase::Complete,
        Phase::Archived,
    ];
    for phase in &phases {
        let json = serde_json::to_string(phase).unwrap();
        let back: Phase = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, phase, "Phase round-trip failed for {json}");
    }
}

#[test]
fn phase_display_from_str_round_trip() {
    let phases = vec![
        Phase::Draft,
        Phase::Active,
        Phase::InReview,
        Phase::Complete,
        Phase::Archived,
    ];
    for phase in &phases {
        let s = phase.to_string();
        let back: Phase = s.parse().unwrap();
        assert_eq!(&back, phase, "Phase Display/FromStr failed for {s}");
    }
}

#[test]
fn phase_from_str_invalid() {
    let result: Result<Phase, _> = "bogus".parse();
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Acceptance round-trip
// ---------------------------------------------------------------------------

#[test]
fn acceptance_prose_round_trip() {
    let a = Acceptance::Prose {
        text: "Ship it".into(),
    };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains(r#""type":"prose""#));
    let back: Acceptance = serde_json::from_str(&json).unwrap();
    assert_eq!(back, a);
}

#[test]
fn acceptance_structured_round_trip() {
    let a = Acceptance::Structured(StructuredAcceptance {
        assertions: vec![Assertion {
            id: "a1".into(),
            text: "Tests pass".into(),
            check_fn: Some("run_tests".into()),
            status: AssertionStatus::Pending,
        }],
        metrics: vec![Metric {
            id: "m1".into(),
            name: "coverage".into(),
            baseline: Some(0.8),
            current: None,
            target: 0.95,
            unit: Some("%".into()),
        }],
        rubric: vec![RubricDimension {
            id: "r1".into(),
            dimension: "code quality".into(),
            score: None,
            scale: 10.0,
            description: Some("How clean is the code".into()),
        }],
    });
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains(r#""type":"structured""#));
    let back: Acceptance = serde_json::from_str(&json).unwrap();
    assert_eq!(back, a);
}

#[test]
fn structured_acceptance_empty_vecs() {
    let sa = StructuredAcceptance {
        assertions: vec![],
        metrics: vec![],
        rubric: vec![],
    };
    let json = serde_json::to_string(&sa).unwrap();
    let back: StructuredAcceptance = serde_json::from_str(&json).unwrap();
    assert_eq!(back, sa);
}

#[test]
fn structured_acceptance_missing_vecs_default() {
    // Per spec: all Vec fields optional/may be empty. serde(default) handles missing.
    let json = r#"{}"#;
    let sa: StructuredAcceptance = serde_json::from_str(json).unwrap();
    assert!(sa.assertions.is_empty());
    assert!(sa.metrics.is_empty());
    assert!(sa.rubric.is_empty());
}

// ---------------------------------------------------------------------------
// Policy round-trip
// ---------------------------------------------------------------------------

#[test]
fn policy_all_none_round_trip() {
    let p = Policy {
        tokens_max: None,
        iterations_max: None,
        wallclock_max_s: None,
        allowed_tools: None,
        review_required: None,
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: Policy = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);
}

#[test]
fn policy_all_set_round_trip() {
    let p = Policy {
        tokens_max: Some(100_000),
        iterations_max: Some(5),
        wallclock_max_s: Some(3600),
        allowed_tools: Some(vec!["search".into(), "code".into()]),
        review_required: Some(true),
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: Policy = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);
}

// ---------------------------------------------------------------------------
// Node round-trip
// ---------------------------------------------------------------------------

#[test]
fn node_round_trip() {
    let n = Node {
        id: "n1".into(),
        intent: "Build the thing".into(),
        parent_id: None,
        phase: Phase::Draft,
        acceptance_json: r#"{"type":"prose","text":"done"}"#.into(),
        local_policy_json: None,
        canonical_artifact_text: None,
        canonical_updated_by_run_id: None,
        next_step_cache: None,
        next_step_cache_for_run_id: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    };
    let json = serde_json::to_string(&n).unwrap();
    let back: Node = serde_json::from_str(&json).unwrap();
    assert_eq!(back, n);
}

#[test]
fn node_with_all_fields_round_trip() {
    let n = Node {
        id: "n2".into(),
        intent: "Sub-goal".into(),
        parent_id: Some("n1".into()),
        phase: Phase::Active,
        acceptance_json: r#"{"type":"structured","assertions":[],"metrics":[],"rubric":[]}"#.into(),
        local_policy_json: Some(r#"{"tokens_max":50000}"#.into()),
        canonical_artifact_text: Some("artifact text here".into()),
        canonical_updated_by_run_id: Some("run-1".into()),
        next_step_cache: Some("Do the next thing".into()),
        next_step_cache_for_run_id: Some("run-1".into()),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-02T00:00:00Z".into(),
    };
    let json = serde_json::to_string(&n).unwrap();
    let back: Node = serde_json::from_str(&json).unwrap();
    assert_eq!(back, n);
}

// ---------------------------------------------------------------------------
// NodeState round-trip
// ---------------------------------------------------------------------------

#[test]
fn node_state_round_trip() {
    let ns = NodeState {
        state: Phase::InReview,
        progress: 0.75,
        confidence: 0.9,
        next_step: "Review by human".into(),
        effective_policy: Policy {
            tokens_max: Some(50_000),
            iterations_max: None,
            wallclock_max_s: None,
            allowed_tools: None,
            review_required: Some(true),
        },
        rollup_blockers: vec!["child-3 is stalled".into()],
    };
    let json = serde_json::to_string(&ns).unwrap();
    let back: NodeState = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ns);
}

// ---------------------------------------------------------------------------
// Run round-trip
// ---------------------------------------------------------------------------

#[test]
fn run_round_trip() {
    let r = Run {
        id: "run-1".into(),
        node_id: "n1".into(),
        run_type: "exploration".into(),
        status: "running".into(),
        runtime: "claude-code".into(),
        input_snapshot_json: Some("{}".into()),
        output_json: None,
        scratchpad_path: Some("/tmp/scratch".into()),
        started_at: Some("2026-01-01T00:00:00Z".into()),
        ended_at: None,
        created_at: "2026-01-01T00:00:00Z".into(),
    };
    let json = serde_json::to_string(&r).unwrap();
    // Verify serde renames run_type to "type"
    assert!(json.contains(r#""type":"exploration""#));
    let back: Run = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
}

// ---------------------------------------------------------------------------
// AncestorSummary round-trip
// ---------------------------------------------------------------------------

#[test]
fn ancestor_summary_round_trip() {
    let a = AncestorSummary {
        id: "n1".into(),
        intent: "Root goal".into(),
        phase: Phase::Active,
        acceptance_summary: "structured: 3 assertions".into(),
        canonical_summary: Some("Summary text".into()),
        progress: 50,
    };
    let json = serde_json::to_string(&a).unwrap();
    let back: AncestorSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(back, a);
}

// ---------------------------------------------------------------------------
// RunInput / RunOutput round-trip
// ---------------------------------------------------------------------------

#[test]
fn run_input_round_trip() {
    let ri = RunInput {
        intent: "Build it".into(),
        acceptance: Acceptance::Prose {
            text: "Done when built".into(),
        },
        effective_policy: Policy {
            tokens_max: None,
            iterations_max: None,
            wallclock_max_s: None,
            allowed_tools: None,
            review_required: None,
        },
        parent_context: vec![],
        node_docs: vec!["doc content".into()],
    };
    let json = serde_json::to_string(&ri).unwrap();
    let back: RunInput = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ri);
}

#[test]
fn run_output_round_trip() {
    let ro = RunOutput {
        findings: "All good".into(),
        concerns: vec!["Edge case X".into()],
        confidence: 0.85,
        needs_human_review: false,
        assertion_results: vec![],
        metric_values: vec![],
        rubric_scores: vec![],
    };
    let json = serde_json::to_string(&ro).unwrap();
    let back: RunOutput = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ro);
}

// ---------------------------------------------------------------------------
// RunEvent / RunTerminal / RunDispatch round-trip
// ---------------------------------------------------------------------------

#[test]
fn run_event_round_trip() {
    let re = RunEvent {
        run_id: "run-1".into(),
        seq: 42,
        event_type: "stdout".into(),
        data_text: Some("hello world".into()),
    };
    let json = serde_json::to_string(&re).unwrap();
    let back: RunEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back, re);
}

#[test]
fn run_terminal_round_trip() {
    let rt = RunTerminal {
        run_id: "run-1".into(),
        status: "completed".into(),
        error: None,
    };
    let json = serde_json::to_string(&rt).unwrap();
    let back: RunTerminal = serde_json::from_str(&json).unwrap();
    assert_eq!(back, rt);
}

#[test]
fn run_dispatch_round_trip() {
    let rd = RunDispatch {
        run_id: "run-1".into(),
        input: RunInput {
            intent: "Build".into(),
            acceptance: Acceptance::Prose {
                text: "Done".into(),
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
        runtime: "claude-code".into(),
    };
    let json = serde_json::to_string(&rd).unwrap();
    let back: RunDispatch = serde_json::from_str(&json).unwrap();
    assert_eq!(back, rd);
}

// ---------------------------------------------------------------------------
// WsFrontendMessage / WsDaemonMessage round-trip
// ---------------------------------------------------------------------------

#[test]
fn ws_frontend_message_round_trip() {
    let m = WsFrontendMessage {
        msg_type: "subscribe".into(),
        id: "n1".into(),
    };
    let json = serde_json::to_string(&m).unwrap();
    // Verify serde renames msg_type to "type"
    assert!(json.contains(r#""type":"subscribe""#));
    let back: WsFrontendMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, m);
}

#[test]
fn ws_daemon_message_event_round_trip() {
    let m = WsDaemonMessage::Event(RunEvent {
        run_id: "run-1".into(),
        seq: 1,
        event_type: "stdout".into(),
        data_text: Some("output".into()),
    });
    let json = serde_json::to_string(&m).unwrap();
    assert!(json.contains(r#""kind":"event""#));
    let back: WsDaemonMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, m);
}

#[test]
fn ws_daemon_message_terminal_round_trip() {
    let m = WsDaemonMessage::Terminal(RunTerminal {
        run_id: "run-1".into(),
        status: "completed".into(),
        error: None,
    });
    let json = serde_json::to_string(&m).unwrap();
    assert!(json.contains(r#""kind":"terminal""#));
    let back: WsDaemonMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back, m);
}

// ---------------------------------------------------------------------------
// FreezeSession / FreezeProposal round-trip
// ---------------------------------------------------------------------------

#[test]
fn freeze_session_round_trip() {
    let fs = FreezeSession {
        id: "fs-1".into(),
        node_id: "n1".into(),
        approved_items_json: "[]".into(),
        current_layer: "assertions".into(),
        status: "active".into(),
        created_at: Some("2026-01-01T00:00:00Z".into()),
        updated_at: Some("2026-01-01T00:00:00Z".into()),
    };
    let json = serde_json::to_string(&fs).unwrap();
    let back: FreezeSession = serde_json::from_str(&json).unwrap();
    assert_eq!(back, fs);
}

#[test]
fn freeze_proposal_round_trip() {
    let fp = FreezeProposal {
        event_type: "propose_assertion".into(),
        layer: "assertions".into(),
        item_json: r#"{"id":"a1","text":"Test passes"}"#.into(),
        reasoning: "This is important".into(),
        source_quote: "From the spec".into(),
    };
    let json = serde_json::to_string(&fp).unwrap();
    let back: FreezeProposal = serde_json::from_str(&json).unwrap();
    assert_eq!(back, fp);
}

// ---------------------------------------------------------------------------
// AssertionStatus round-trip
// ---------------------------------------------------------------------------

#[test]
fn assertion_status_round_trip() {
    for status in [
        AssertionStatus::Pass,
        AssertionStatus::Fail,
        AssertionStatus::Pending,
    ] {
        let json = serde_json::to_string(&status).unwrap();
        let back: AssertionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, status);
    }
}
