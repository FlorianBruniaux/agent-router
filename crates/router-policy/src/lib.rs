#![forbid(unsafe_code)]

mod asset_phases;
mod constraint;
mod phases;
mod preset;
mod retrieval;
mod route;
mod score;

pub use asset_phases::{PhaseAssetInputs, PhaseExecutionScope, asset_authority_scope_digest};
pub use constraint::{
    CapabilitiesSatisfied, CapabilityKnowledge, Constraint, ConstraintEngine, ConstraintVerdict,
    EffortSupported, EligibilityContext, ModelAvailable, StageSupported, Support, is_eligible,
};
pub use phases::{
    MAX_PHASE_ASSETS, MAX_PHASE_CANDIDATES, MAX_PHASE_REQUEST_BYTES, PhaseRouteError, PhaseRouter,
    ValidatedPhaseContext, ValidatedPhaseReceipt,
};
pub use retrieval::{
    RetrievalError, SkillRetriever, StaticSkillRetriever, parse_candidate_document,
};
pub use route::{PolicyConfig, RouteError, Router};
