#![forbid(unsafe_code)]

mod constraint;
mod phases;
mod preset;
mod retrieval;
mod route;
mod score;

pub use constraint::{
    CapabilitiesSatisfied, CapabilityKnowledge, Constraint, ConstraintEngine, ConstraintVerdict,
    EffortSupported, EligibilityContext, ModelAvailable, StageSupported, Support, is_eligible,
};
pub use phases::{
    MAX_PHASE_ASSETS, MAX_PHASE_CANDIDATES, MAX_PHASE_REQUEST_BYTES, PhaseRouteError, PhaseRouter,
    ValidatedPhaseContext,
};
pub use retrieval::{
    RetrievalError, SkillRetriever, StaticSkillRetriever, parse_candidate_document,
};
pub use route::{PolicyConfig, RouteError, Router};
