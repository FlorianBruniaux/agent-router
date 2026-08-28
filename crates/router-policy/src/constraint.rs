use std::collections::BTreeMap;

use router_types::{
    Capability, EffortLevel, HostSnapshot, ModelSpec, RiskLevel, StageKind, TaskContext,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Support {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilityKnowledge(BTreeMap<Capability, Support>);

impl CapabilityKnowledge {
    pub fn support_for(&self, capability: Capability) -> Support {
        self.0.get(&capability).copied().unwrap_or(Support::Unknown)
    }
}

impl From<BTreeMap<Capability, Support>> for CapabilityKnowledge {
    fn from(value: BTreeMap<Capability, Support>) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibilityContext {
    pub task: TaskContext,
    pub host: HostSnapshot,
    pub model: ModelSpec,
    pub stage: StageKind,
    pub effort: EffortLevel,
    pub capabilities: CapabilityKnowledge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConstraintVerdict {
    Eligible {
        code: &'static str,
    },
    Excluded {
        code: &'static str,
        evidence: String,
    },
    Unknown {
        code: &'static str,
        evidence: String,
    },
}

impl ConstraintVerdict {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Eligible { code } | Self::Excluded { code, .. } | Self::Unknown { code, .. } => {
                code
            }
        }
    }
}

pub trait Constraint: Send + Sync {
    fn evaluate(&self, context: &EligibilityContext) -> ConstraintVerdict;
}

pub struct ConstraintEngine {
    constraints: Vec<Box<dyn Constraint>>,
}

impl ConstraintEngine {
    pub fn evaluate(&self, context: &EligibilityContext) -> Vec<ConstraintVerdict> {
        self.constraints
            .iter()
            .map(|constraint| constraint.evaluate(context))
            .collect()
    }
}

impl Default for ConstraintEngine {
    fn default() -> Self {
        Self {
            constraints: vec![
                Box::new(ModelAvailable),
                Box::new(EffortSupported),
                Box::new(StageSupported),
                Box::new(CapabilitiesSatisfied),
            ],
        }
    }
}

pub struct ModelAvailable;

impl Constraint for ModelAvailable {
    fn evaluate(&self, context: &EligibilityContext) -> ConstraintVerdict {
        if context.host.supported_models.contains(&context.model.id) {
            eligible("host.model.available")
        } else {
            excluded(
                "host.model.unavailable",
                format!(
                    "host '{}' does not support model '{}'",
                    context.host.id, context.model.id
                ),
            )
        }
    }
}

pub struct EffortSupported;

impl Constraint for EffortSupported {
    fn evaluate(&self, context: &EligibilityContext) -> ConstraintVerdict {
        let supported = context
            .host
            .supported_efforts
            .get(&context.model.id)
            .is_some_and(|efforts| efforts.contains(&context.effort));

        if supported {
            eligible("host.effort.supported")
        } else {
            excluded(
                "host.effort.unsupported",
                format!(
                    "host '{}' does not support effort '{:?}' for model '{}'",
                    context.host.id, context.effort, context.model.id
                ),
            )
        }
    }
}

pub struct StageSupported;

impl Constraint for StageSupported {
    fn evaluate(&self, context: &EligibilityContext) -> ConstraintVerdict {
        if context.model.stages.contains(&context.stage) {
            eligible("model.stage.supported")
        } else {
            excluded(
                "model.stage.unsupported",
                format!(
                    "model '{}' cannot serve stage '{:?}'",
                    context.model.id, context.stage
                ),
            )
        }
    }
}

pub struct CapabilitiesSatisfied;

impl Constraint for CapabilitiesSatisfied {
    fn evaluate(&self, context: &EligibilityContext) -> ConstraintVerdict {
        for capability in &context.task.required_capabilities {
            match context.capabilities.support_for(*capability) {
                Support::Unsupported => {
                    return excluded(
                        "host.capability.missing",
                        format!(
                            "host '{}' is known not to support capability '{:?}'",
                            context.host.id, capability
                        ),
                    );
                }
                Support::Unknown => {
                    return unknown_capability(context, *capability);
                }
                Support::Supported if !context.host.capabilities.contains(capability) => {
                    return excluded(
                        "host.capability.missing",
                        format!(
                            "host '{}' has no supported capability '{:?}'",
                            context.host.id, capability
                        ),
                    );
                }
                Support::Supported if !context.model.capabilities.contains(capability) => {
                    return excluded(
                        "model.capability.unsupported",
                        format!(
                            "model '{}' cannot provide capability '{:?}'",
                            context.model.id, capability
                        ),
                    );
                }
                Support::Supported => {}
            }
        }

        eligible("host.capability.satisfied")
    }
}

pub fn is_eligible(verdicts: &[ConstraintVerdict]) -> bool {
    verdicts
        .iter()
        .all(|verdict| matches!(verdict, ConstraintVerdict::Eligible { .. }))
}

fn eligible(code: &'static str) -> ConstraintVerdict {
    ConstraintVerdict::Eligible { code }
}

fn excluded(code: &'static str, evidence: String) -> ConstraintVerdict {
    ConstraintVerdict::Excluded { code, evidence }
}

fn unknown_capability(context: &EligibilityContext, capability: Capability) -> ConstraintVerdict {
    let code = match context.task.risk {
        RiskLevel::High | RiskLevel::Critical => "host.capability.unknown_high_risk",
        RiskLevel::Low | RiskLevel::Medium => "host.capability.unknown",
    };
    ConstraintVerdict::Unknown {
        code,
        evidence: format!(
            "host '{}' has unknown support for capability '{:?}'",
            context.host.id, capability
        ),
    }
}
