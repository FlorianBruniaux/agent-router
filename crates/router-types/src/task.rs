use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{Capability, HostSnapshot, SkillCandidate};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Explore,
    Implement,
    Review,
    Architecture,
    Security,
    Migration,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskContext {
    pub objective: String,
    pub kind: TaskKind,
    pub risk: RiskLevel,
    pub required_capabilities: BTreeSet<Capability>,
    pub acceptance: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RouteRequest {
    pub task: TaskContext,
    pub host: HostSnapshot,
    pub skill_candidates: Vec<SkillCandidate>,
}
