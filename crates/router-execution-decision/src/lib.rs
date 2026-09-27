//! Separates the immutable deterministic baseline from an approved effective
//! selection. Backend authority must be reloaded under a dispatch-time lock.
#![forbid(unsafe_code)]

use router_backend_policy::{ActivatedBackendDecision, BackendActivationState, TrustedAuthorities};
use router_policy::{PhaseRouter, ValidatedPhaseContext, ValidatedPhaseReceipt};
use router_types::{
    BackendDecisionBinding, ExecutionDecisionBinding, PhaseReceipt, PhaseSelection,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum DecisionError {
    #[error("execution_decision.authority_unavailable")]
    AuthorityUnavailable,
    #[error("execution_decision.authority_required")]
    AuthorityRequired,
    #[error("execution_decision.context")]
    Context,
    #[error("execution_decision.abstained")]
    Abstained,
    #[error("execution_decision.current_authority_changed_or_expired")]
    Stale,
    #[error("execution_decision.serialization")]
    Serialization,
}
pub type Result<T> = std::result::Result<T, DecisionError>;

/// Trusted application boundary: acquire a bounded/try shared lock, then load
/// current state and roots while holding it. Activation and rollback must take
/// the corresponding exclusive lock. Never implement this with a stale clone.
pub trait ActivationAuthority: Send + Sync {
    fn acquire(&self) -> Result<Box<dyn ActivationGuard + '_>>;
}
/// The lock must remain held until Drop. JSON evidence is not this capability.
pub trait ActivationGuard: Send {
    fn state(&self) -> &BackendActivationState;
    fn authorities(&self) -> &TrustedAuthorities;
}

#[derive(Clone, Debug)]
enum Source {
    Deterministic(ValidatedPhaseReceipt),
    Activated(ActivatedBackendDecision),
}
/// Opaque and deliberately non-deserializable. No constructor accepts an
/// arbitrary candidate, a fabricated fallback, or a serialized audit binding.
#[derive(Clone, Debug)]
pub struct ExecutionDecision {
    source: Source,
    binding: Option<ExecutionDecisionBinding>,
    selection: PhaseSelection,
}
impl ExecutionDecision {
    pub fn deterministic(
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        proof: &ValidatedPhaseReceipt,
    ) -> Result<Self> {
        let proof = router
            .validate_receipt(context, proof.receipt())
            .map_err(|_| DecisionError::Context)?;
        Ok(Self {
            selection: proof.selection().clone(),
            source: Source::Deterministic(proof),
            binding: None,
        })
    }
    pub fn activated(
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
        proof: &ActivatedBackendDecision,
        current: &dyn ActivationGuard,
        now: u64,
    ) -> Result<Self> {
        proof
            .validate_current(current.state(), router, context, current.authorities(), now)
            .map_err(|_| DecisionError::Stale)?;
        let selected = proof.selection().ok_or(DecisionError::Abstained)?;
        router
            .validate_selection(context, selected)
            .map_err(|_| DecisionError::Context)?;
        let evidence = proof.evidence();
        let mut binding = ExecutionDecisionBinding {
            schema_version: 1,
            decision_id: String::new(),
            baseline_receipt_id: proof.baseline().receipt_id.clone(),
            context_hash: context.context_hash().into(),
            selected_candidate_id: selected.candidate.id.clone(),
            selection_sha256: hash("agent-router:effective-selection:v1", selected)?,
            backend: BackendDecisionBinding {
                activation_state_hash: evidence.state_hash.clone(),
                authority_roots_sha256: evidence.authority_roots_sha256.clone(),
                backend_artifact_hash: evidence.backend_artifact_hash.clone(),
                promotion_hash: evidence.promotion_hash.clone(),
                proposal_sha256: evidence.proposal_sha256.clone(),
                proposal_expires_at_unix_seconds: evidence.proposal_expires_at_unix_seconds,
                backend_decision_hash: evidence.decision_hash.clone(),
            },
        };
        binding.decision_id = hash("agent-router:execution-decision:v1", &binding)?;
        Ok(Self {
            selection: selected.clone(),
            source: Source::Activated(proof.clone()),
            binding: Some(binding),
        })
    }
    pub fn baseline(&self) -> &PhaseReceipt {
        match &self.source {
            Source::Deterministic(p) => p.receipt(),
            Source::Activated(p) => p.baseline(),
        }
    }
    pub fn selection(&self) -> &PhaseSelection {
        &self.selection
    }
    pub fn binding(&self) -> Option<&ExecutionDecisionBinding> {
        self.binding.as_ref()
    }
    pub fn authorization_id(&self) -> &str {
        self.binding
            .as_ref()
            .map_or(self.baseline().receipt_id.as_str(), |b| {
                b.decision_id.as_str()
            })
    }
    /// Identity-only validation is also usable when reopening an old run. It
    /// never substitutes for current authority validation before execution.
    pub fn validate_identity(
        &self,
        router: &PhaseRouter,
        context: &ValidatedPhaseContext,
    ) -> Result<()> {
        match &self.source {
            Source::Deterministic(p) => {
                router
                    .validate_receipt(context, p.receipt())
                    .map_err(|_| DecisionError::Context)?;
            }
            Source::Activated(p) => {
                if router.route(context).map_err(|_| DecisionError::Context)? != *p.baseline()
                    || p.evidence().context_hash != context.context_hash()
                {
                    return Err(DecisionError::Context);
                }
            }
        }
        router
            .validate_selection(context, self.selection())
            .map_err(|_| DecisionError::Context)
    }
    pub fn acquire_dispatch<'a>(
        &'a self,
        router: &'a PhaseRouter,
        context: &'a ValidatedPhaseContext,
        authority: Option<&'a dyn ActivationAuthority>,
        now: u64,
    ) -> Result<Option<DispatchGuard<'a>>> {
        self.validate_identity(router, context)?;
        if let Source::Activated(proof) = &self.source {
            let guard = authority
                .ok_or(DecisionError::AuthorityRequired)?
                .acquire()?;
            let lease = DispatchGuard {
                guard,
                proof,
                router,
                context,
            };
            lease.validate_at(now)?;
            Ok(Some(lease))
        } else {
            Ok(None)
        }
    }
}
/// Holds current application authority only through the final validation/spawn
/// critical section, never for the lifetime of the worker process.
pub struct DispatchGuard<'a> {
    guard: Box<dyn ActivationGuard + 'a>,
    proof: &'a ActivatedBackendDecision,
    router: &'a PhaseRouter,
    context: &'a ValidatedPhaseContext,
}
impl DispatchGuard<'_> {
    pub fn validate_at(&self, now: u64) -> Result<()> {
        self.proof
            .validate_current(
                self.guard.state(),
                self.router,
                self.context,
                self.guard.authorities(),
                now,
            )
            .map_err(|_| DecisionError::Stale)
    }
}
fn hash<T: Serialize>(domain: &str, value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| DecisionError::Serialization)?;
    let mut h = Sha256::new();
    h.update(domain);
    h.update([0]);
    h.update(bytes);
    Ok(hex::encode(h.finalize()))
}
