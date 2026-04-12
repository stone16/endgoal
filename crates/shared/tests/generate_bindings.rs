#![cfg(feature = "generate-bindings")]

use ts_rs::TS;

/// When run with --features generate-bindings, this test exports all TS type
/// definitions to the directory specified by TS_RS_EXPORT_DIR.
#[test]
fn export_all_bindings() {
    use endgoal_shared::*;

    // Export each type. ts-rs writes to TS_RS_EXPORT_DIR.
    Phase::export_all().expect("Failed to export Phase");
    AssertionStatus::export_all().expect("Failed to export AssertionStatus");
    Assertion::export_all().expect("Failed to export Assertion");
    Metric::export_all().expect("Failed to export Metric");
    RubricDimension::export_all().expect("Failed to export RubricDimension");
    StructuredAcceptance::export_all().expect("Failed to export StructuredAcceptance");
    Acceptance::export_all().expect("Failed to export Acceptance");
    Policy::export_all().expect("Failed to export Policy");
    Node::export_all().expect("Failed to export Node");
    NodeState::export_all().expect("Failed to export NodeState");
    Run::export_all().expect("Failed to export Run");
    AncestorSummary::export_all().expect("Failed to export AncestorSummary");
    RunInput::export_all().expect("Failed to export RunInput");
    RunOutput::export_all().expect("Failed to export RunOutput");
    RunEvent::export_all().expect("Failed to export RunEvent");
    RunTerminal::export_all().expect("Failed to export RunTerminal");
    RunDispatch::export_all().expect("Failed to export RunDispatch");
    WsFrontendMessage::export_all().expect("Failed to export WsFrontendMessage");
    WsDaemonMessage::export_all().expect("Failed to export WsDaemonMessage");
    FreezeSession::export_all().expect("Failed to export FreezeSession");
    FreezeProposal::export_all().expect("Failed to export FreezeProposal");
    FreezeLayerCompleteEvent::export_all()
        .expect("Failed to export FreezeLayerCompleteEvent");
}
