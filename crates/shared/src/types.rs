use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ---------------------------------------------------------------------------
// Phase
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Phase {
    Draft,
    Active,
    InReview,
    Complete,
    Archived,
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Phase::Draft => write!(f, "draft"),
            Phase::Active => write!(f, "active"),
            Phase::InReview => write!(f, "in_review"),
            Phase::Complete => write!(f, "complete"),
            Phase::Archived => write!(f, "archived"),
        }
    }
}

impl std::str::FromStr for Phase {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(Phase::Draft),
            "active" => Ok(Phase::Active),
            "in_review" => Ok(Phase::InReview),
            "complete" => Ok(Phase::Complete),
            "archived" => Ok(Phase::Archived),
            other => Err(format!("invalid phase: {other}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AssertionStatus {
    Pass,
    Fail,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Assertion {
    pub id: String,
    pub text: String,
    pub check_fn: Option<String>,
    pub status: AssertionStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Metric {
    pub id: String,
    pub name: String,
    pub baseline: Option<f64>,
    pub current: Option<f64>,
    pub target: f64,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RubricDimension {
    pub id: String,
    pub dimension: String,
    pub score: Option<f64>,
    pub scale: f64,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StructuredAcceptance {
    #[serde(default)]
    pub assertions: Vec<Assertion>,
    #[serde(default)]
    pub metrics: Vec<Metric>,
    #[serde(default)]
    pub rubric: Vec<RubricDimension>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Acceptance {
    Prose { text: String },
    Structured(StructuredAcceptance),
}

// ---------------------------------------------------------------------------
// Policy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Policy {
    pub tokens_max: Option<u64>,
    pub iterations_max: Option<u64>,
    pub wallclock_max_s: Option<u64>,
    pub allowed_tools: Option<Vec<String>>,
    pub review_required: Option<bool>,
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Node {
    pub id: String,
    pub intent: String,
    pub parent_id: Option<String>,
    pub phase: Phase,
    pub acceptance_json: String,
    pub local_policy_json: Option<String>,
    pub canonical_artifact_text: Option<String>,
    pub canonical_updated_by_run_id: Option<String>,
    pub next_step_cache: Option<String>,
    pub next_step_cache_for_run_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// NodeState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NodeState {
    pub state: Phase,
    pub progress: f64,
    pub confidence: f64,
    pub next_step: String,
    pub effective_policy: Policy,
    pub rollup_blockers: Vec<String>,
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Run {
    pub id: String,
    pub node_id: String,
    #[serde(rename = "type")]
    pub run_type: String,
    pub status: String,
    pub runtime: String,
    pub input_snapshot_json: Option<String>,
    pub output_json: Option<String>,
    pub scratchpad_path: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// AncestorSummary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AncestorSummary {
    pub id: String,
    pub intent: String,
    pub phase: Phase,
    pub acceptance_summary: String,
    pub canonical_summary: Option<String>,
    pub progress: u8,
}

// ---------------------------------------------------------------------------
// RunInput / RunOutput
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunInput {
    pub intent: String,
    pub acceptance: Acceptance,
    pub effective_policy: Policy,
    pub parent_context: Vec<AncestorSummary>,
    pub node_docs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunOutput {
    pub findings: String,
    pub concerns: Vec<String>,
    pub confidence: f64,
    pub needs_human_review: bool,
    pub assertion_results: Vec<Assertion>,
    pub metric_values: Vec<Metric>,
    pub rubric_scores: Vec<RubricDimension>,
}

// ---------------------------------------------------------------------------
// RunEvent / RunTerminal / RunDispatch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunEvent {
    pub run_id: String,
    pub seq: i64,
    pub event_type: String,
    pub data_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunTerminal {
    pub run_id: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunDispatch {
    pub run_id: String,
    pub input: RunInput,
    pub runtime: String,
}

// ---------------------------------------------------------------------------
// WebSocket messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WsFrontendMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum WsDaemonMessage {
    Event(RunEvent),
    Terminal(RunTerminal),
}

// ---------------------------------------------------------------------------
// FreezeSession / FreezeProposal
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FreezeSession {
    pub id: String,
    pub node_id: String,
    pub approved_items_json: String,
    pub current_layer: String,
    pub status: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FreezeProposal {
    pub event_type: String,
    pub layer: String,
    pub item_json: String,
    pub reasoning: String,
    pub source_quote: String,
}
