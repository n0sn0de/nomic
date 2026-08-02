use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScenarioStatus {
    Blocked,
    Enabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionResult {
    Pass,
    Fail,
    MissingCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum ScenarioOutcome {
    Pass,
    Fail,
    Blocked,
}

impl ScenarioOutcome {
    #[must_use]
    pub const fn from_result(status: ScenarioStatus, result: ExecutionResult) -> Self {
        match status {
            ScenarioStatus::Blocked => Self::Blocked,
            ScenarioStatus::Enabled => match result {
                ExecutionResult::Pass => Self::Pass,
                ExecutionResult::Fail | ExecutionResult::MissingCapability => Self::Fail,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioContract {
    pub id: String,
    pub case_id: String,
    pub spec_version: String,
    pub status: ScenarioStatus,
    pub release_requirement: String,
    pub phase: String,
    pub topology: String,
    pub backend: Backend,
    pub fidelity: Fidelity,
    pub semantic_seed_domain: String,
    pub capabilities: Vec<String>,
    pub fixtures: Vec<String>,
    pub steps: Vec<String>,
    pub barriers: Vec<String>,
    pub terminal_predicates: Vec<String>,
    pub unchanged_projections: Vec<String>,
    pub oracles: Vec<String>,
    pub invariants: Vec<String>,
    pub evidence: Vec<String>,
    pub operation_deadline_ms: u64,
    pub overall_deadline_ms: u64,
    pub memory_budget_bytes: u64,
    pub artifact_budget_bytes: u64,
    pub ci_lane: String,
    pub executable: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backend {
    Process,
    Container,
    ProcessAndContainer,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fidelity {
    Synthetic,
    Integrated,
}
