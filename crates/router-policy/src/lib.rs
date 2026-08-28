#![forbid(unsafe_code)]

mod constraint;
mod retrieval;

pub use constraint::{
    CapabilitiesSatisfied, CapabilityKnowledge, Constraint, ConstraintEngine, ConstraintVerdict,
    EffortSupported, EligibilityContext, ModelAvailable, StageSupported, Support, is_eligible,
};
pub use retrieval::{
    RetrievalError, SkillRetriever, StaticSkillRetriever, parse_candidate_document,
};
