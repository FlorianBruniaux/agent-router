use serde::{Deserialize, Serialize};

use crate::{EffortLevel, ModelId, SkillId, StageKind};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationIndependence {
    FreshInstance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanStage {
    pub kind: StageKind,
    pub model: ModelId,
    pub effort: EffortLevel,
    pub skills: Vec<SkillId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_independence: Option<VerificationIndependence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExecutionPlan {
    pub preset: String,
    pub stages: Vec<PlanStage>,
    pub utility_micros: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Exclusion {
    pub stage: StageKind,
    pub model: ModelId,
    pub code: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteDecision {
    Selected {
        plan: ExecutionPlan,
        alternatives: Vec<ExecutionPlan>,
        exclusions: Vec<Exclusion>,
        missing_evidence: Vec<String>,
    },
    NoRoute {
        exclusions: Vec<Exclusion>,
        missing_evidence: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DecisionEnvelope {
    pub schema_version: String,
    pub decision_id: String,
    pub catalog_version: String,
    pub policy_version: String,
    pub decision: RouteDecision,
}
