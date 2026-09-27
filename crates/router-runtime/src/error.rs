#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("runtime.decision: {0}")]
    Decision(#[from] router_execution_decision::DecisionError),
    #[error("runtime.invalid: {0}")]
    Invalid(&'static str),
    #[error("runtime.io: {0}")]
    Io(#[from] std::io::Error),
    #[error("runtime.storage")]
    Storage(#[from] rusqlite::Error),
    #[error("runtime.json")]
    Json(#[from] serde_json::Error),
    #[error("runtime.integrity: {0}")]
    Integrity(&'static str),
    #[error("runtime.locked")]
    Locked,
    #[error("runtime.transition.invalid")]
    Transition,
    #[error("runtime.not_found")]
    NotFound,
    #[error("runtime.approval.required_or_changed")]
    Approval,
    #[error("runtime.source.changed_or_dirty")]
    SourceChanged,
    #[error("runtime.scope.denied")]
    Scope,
    #[error("runtime.sandbox.unknown_or_unsafe")]
    Sandbox,
    #[error("runtime.resource.unsupported: {0}")]
    ResourceUnsupported(&'static str),
    #[error("runtime.route.invalid")]
    Route,
    #[error("runtime.adapter: {0}")]
    Adapter(#[from] router_adapter_api::AdapterError),
    #[error("runtime.process.failed")]
    Process,
    #[error("runtime.process.cleanup_unknown")]
    CleanupUnknown,
    #[error("runtime.limit")]
    Limit,
    #[error("runtime.cancelled")]
    Cancelled,
    #[error("runtime.dependencies.pending")]
    DependenciesPending,
    #[error("runtime.verification.required")]
    Verification,
}
pub type Result<T> = std::result::Result<T, RuntimeError>;
