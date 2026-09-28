use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

#[derive(Clone, Copy, ValueEnum)]
pub enum HostChoice {
    Auto,
    Codex,
    Claude,
}

#[derive(Args)]
pub struct PhaseArgs {
    #[arg(long)]
    pub context: PathBuf,
    #[arg(long)]
    pub host_snapshot: PathBuf,
    #[arg(long)]
    pub catalog: PathBuf,
    #[arg(long)]
    pub policy: Option<PathBuf>,
    /// Use the explicitly selected store's active offline policy unless --policy overrides it.
    #[arg(long)]
    pub policy_store: Option<PathBuf>,
    /// Independently pinned model/skill contracts and fresh native availability.
    #[arg(long)]
    pub asset_authority: Option<PathBuf>,
    #[arg(long)]
    pub now: Option<u64>,
    #[arg(long, default_value_t = 86400)]
    pub snapshot_max_age_seconds: u64,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Explicit standard-sidecar capability and single-event export commands.
    Sidecar {
        #[arg(long, global = true)]
        json: bool,
        #[command(subcommand)]
        command: crate::sidecar::Command,
    },
    /// Review accounting bounds and propose durable attempt-based escalation.
    Escalation {
        #[arg(long, global = true)]
        json: bool,
        #[command(subcommand)]
        command: crate::escalation::Command,
    },
    /// Generate a bounded global plan from eligible metadata only.
    GeneratePlan {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    /// Manage signed, explicitly approved semantic backend activation.
    BackendActivation {
        #[arg(long, global = true)]
        json: bool,
        #[command(subcommand)]
        command: crate::backend_activation::Command,
    },
    Coordinate {
        #[arg(long, global = true)]
        json: bool,
        #[command(subcommand)]
        command: CoordinateCommand,
    },
    Assets {
        #[command(subcommand)]
        command: AssetCommand,
    },
    Prepare {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        output_directory: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Review or execute one immutable workflow node.
    Run {
        #[arg(long)]
        definition: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        sandbox: PathBuf,
        #[arg(long)]
        approve: Option<String>,
        #[arg(long, conflicts_with = "approve")]
        retry: bool,
        #[arg(long)]
        json: bool,
    },
    Status {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        json: bool,
    },
    Cancel {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        json: bool,
    },
    Events {
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        json: bool,
    },
    SourceState {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Workflow {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        execution_contract: Option<PathBuf>,
        #[arg(long, requires = "execution_contract")]
        seal_contract: bool,
        #[arg(long)]
        json: bool,
    },
    Verify {
        #[arg(long)]
        definition: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        evidence: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Artifacts {
        #[arg(long)]
        definition: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long, global = true)]
        json: bool,
        #[command(subcommand)]
        command: ArtifactCommand,
    },
    Feedback {
        #[arg(long)]
        store: PathBuf,
        #[arg(long, global = true)]
        now: Option<u64>,
        #[arg(long, global = true)]
        json: bool,
        #[command(subcommand)]
        command: FeedbackCommand,
    },
    Policy {
        #[arg(long)]
        store: PathBuf,
        #[arg(long, global = true)]
        now: Option<u64>,
        #[arg(long, global = true)]
        json: bool,
        #[command(subcommand)]
        command: PolicyCommand,
    },
    Doctor {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long)]
        deep: bool,
        #[arg(long)]
        json: bool,
    },
    Audit {
        #[command(subcommand)]
        command: AuditCommand,
    },
    Config {
        #[arg(long)]
        layers: PathBuf,
        #[arg(long)]
        json: bool,
    },
    RoutePhase {
        #[command(flatten)]
        inputs: PhaseArgs,
        #[arg(long)]
        json: bool,
    },
    Backend {
        #[command(flatten)]
        inputs: PhaseArgs,
        #[arg(long, default_value = "deterministic", value_parser = ["deterministic", "laya", "jev", "standard-v1"])]
        backend: String,
        #[arg(long)]
        expected_backend_artifact: Option<String>,
        #[arg(long)]
        endpoint: Option<std::net::SocketAddr>,
        #[arg(long)]
        bearer_env: Option<String>,
        #[arg(long)]
        allow_redacted_metadata: bool,
        #[arg(long, default_value = "fallback", value_parser = ["fallback", "abstain"])]
        on_error: String,
        #[arg(long, default_value_t = 500)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 800000)]
        minimum_confidence_micros: u32,
        #[arg(long)]
        json: bool,
    },
    Replay {
        #[arg(long)]
        record: PathBuf,
        #[arg(long)]
        policy: String,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long, default_value_t = 86400)]
        snapshot_max_age_seconds: u64,
        #[arg(long)]
        json: bool,
    },
    Eval {
        #[command(subcommand)]
        command: EvalCommand,
    },
    Catalog {
        #[command(subcommand)]
        command: CatalogCommand,
    },
    Route {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        host_snapshot: Option<PathBuf>,
        #[arg(long)]
        retrieval: Option<PathBuf>,
        #[arg(long, default_value_t = 86400)]
        snapshot_max_age_seconds: u64,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    Inspect {
        #[arg(long, value_enum, default_value = "auto")]
        host: HostChoice,
        #[arg(long)]
        model_cache: Option<PathBuf>,
        #[arg(long)]
        native_stream: Option<PathBuf>,
        #[arg(long)]
        observed_at: Option<u64>,
        #[arg(long)]
        observed_version: Option<String>,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    Search {
        #[arg(long)]
        metadata: PathBuf,
        #[arg(long)]
        catalog: Option<PathBuf>,
        #[arg(long)]
        now: Option<u64>,
        #[arg(
            long,
            required_unless_present = "context_query",
            conflicts_with = "context_query"
        )]
        query: Option<String>,
        #[arg(long, conflicts_with_all = ["query", "kind", "top_k"], requires = "catalog")]
        context_query: Option<PathBuf>,
        #[arg(long, requires = "context_query")]
        semantic_ranking: Option<PathBuf>,
        #[arg(long, value_parser = ["agent", "skill", "prompt", "tool", "model"])]
        kind: Option<String>,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        #[arg(long)]
        json: bool,
    },
    Index {
        #[arg(long)]
        metadata: PathBuf,
        #[arg(long)]
        signals: Option<PathBuf>,
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    /// Apply a checked metadata diff; optional output is a new file, never an overwrite.
    Refresh {
        #[arg(long)]
        metadata: PathBuf,
        #[arg(long)]
        successor: PathBuf,
        #[arg(long)]
        expected_source_hash: String,
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Collect explicit task facts and clarify unknowns without executing anything.
    TaskGuide {
        #[command(flatten)]
        args: crate::task_guide::GuideArgs,
        #[arg(long)]
        json: bool,
    },
    Normalize {
        #[arg(long)]
        task: PathBuf,
        /// Explicit parent facts about environment, reversibility and operations.
        #[arg(long)]
        risk_context: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    Plan {
        #[arg(long)]
        task: PathBuf,
        #[arg(long)]
        risk_context: Option<PathBuf>,
        #[arg(long, default_value = "plan-execute-verify", value_parser = ["direct-safe", "plan-execute-verify", "explore-then-build", "fanout-review", "incident", "migration", "security-readonly", "bulk-mechanical"])]
        preset: String,
        #[arg(long, default_value_t = 8)]
        max_width: usize,
        #[arg(long, default_value_t = 16)]
        max_depth: usize,
        #[arg(long, default_value_t = 0)]
        reserve_cost_microusd: u64,
        #[arg(long)]
        json: bool,
    },
    Explain {
        #[arg(long)]
        decision: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum AssetCommand {
    Review {
        #[arg(long)]
        manifest: PathBuf,
        #[command(flatten)]
        inputs: PhaseArgs,
        #[arg(long = "skill", required = true)]
        skills: Vec<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum CoordinateCommand {
    Seal {
        #[arg(long)]
        definition: PathBuf,
    },
    Status {
        #[arg(long)]
        definition: PathBuf,
        #[arg(long)]
        store: PathBuf,
    },
    Cancel {
        #[arg(long)]
        definition: PathBuf,
        #[arg(long)]
        store: PathBuf,
    },
    Review {
        #[arg(long)]
        definition: PathBuf,
        #[arg(long)]
        source_definition: PathBuf,
        #[arg(long)]
        sandbox: PathBuf,
        #[arg(long)]
        node_id: String,
        #[arg(long, default_value_t = 0)]
        generation: u32,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        output_directory: PathBuf,
    },
    RunWave {
        #[arg(long)]
        definition: PathBuf,
        #[arg(long)]
        wave: PathBuf,
        #[arg(long)]
        store: PathBuf,
    },
    Verify {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[arg(long)]
        evidence: PathBuf,
        #[arg(long)]
        approval: String,
    },
    Service {
        #[arg(long)]
        definition: PathBuf,
        #[arg(long = "bundle", required = true)]
        bundles: Vec<PathBuf>,
        #[arg(long)]
        store: PathBuf,
    },
    Artifacts {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        store: PathBuf,
        #[command(subcommand)]
        command: ArtifactCommand,
    },
}

#[derive(Subcommand)]
pub enum ArtifactCommand {
    Preview,
    Outputs,
    Declared,
    PurgeDeclared {
        #[arg(long)]
        id: String,
    },
    PurgeOutputs {
        #[arg(long)]
        attempt: u32,
    },
    Apply {
        #[arg(long)]
        approve: String,
    },
    Discard {
        #[arg(long)]
        approve: String,
    },
    Keep,
}

#[derive(Subcommand)]
pub enum FeedbackCommand {
    Consent,
    Configure {
        #[arg(long)]
        preferences: PathBuf,
    },
    Record {
        #[arg(long)]
        input: PathBuf,
    },
    Export,
    Statistics,
    Purge {
        #[arg(long)]
        expired_only: bool,
    },
    Drift {
        #[arg(long)]
        rules: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum PolicyCommand {
    Baseline {
        #[arg(long)]
        policy: PathBuf,
    },
    Propose {
        #[arg(long)]
        policy: PathBuf,
        #[arg(long)]
        rules: PathBuf,
    },
    List,
    Active,
    Show {
        id: String,
    },
    Digest {
        id: String,
    },
    Evaluate {
        id: String,
        #[arg(long, required = true, num_args = 1..)]
        records: Vec<PathBuf>,
        #[arg(long)]
        baseline: String,
        #[arg(long)]
        candidate: String,
        #[arg(long)]
        criteria: PathBuf,
        #[arg(long, default_value_t = 86400)]
        snapshot_max_age_seconds: u64,
    },
    Trial {
        id: String,
        #[arg(long)]
        evidence: PathBuf,
    },
    SigningPayload {
        #[arg(long)]
        approval: PathBuf,
    },
    Apply {
        #[arg(long)]
        approval: PathBuf,
        #[arg(long)]
        trusted_keys: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum AuditCommand {
    Export {
        #[arg(long)]
        inputs: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Verify {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        expected_root: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum EvalCommand {
    Seal {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Compare {
        #[arg(long, required = true, num_args = 1..)]
        records: Vec<PathBuf>,
        #[arg(long)]
        baseline: String,
        #[arg(long)]
        candidate: String,
        #[arg(long)]
        criteria: Option<PathBuf>,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long, default_value_t = 86400)]
        snapshot_max_age_seconds: u64,
        #[arg(long)]
        json: bool,
    },
    Corpus {
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum CatalogCommand {
    Validate {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    Show {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "model", value_parser = ["model", "agent", "skill", "prompt", "tool"])]
        kind: String,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    Refresh {
        #[arg(long)]
        host_snapshot: PathBuf,
        #[arg(long)]
        prior: PathBuf,
        #[arg(long)]
        registry: Option<PathBuf>,
        #[arg(long)]
        now: Option<u64>,
        #[arg(long)]
        json: bool,
    },
}

impl Commands {
    /// Ask the command for its canonical JSON result, then render that same
    /// result for humans. Existing machine flags and exit statuses are preserved.
    pub(crate) fn structured_output(&mut self) -> Option<&'static str> {
        let (json, title) = match self {
            Self::GeneratePlan { json, .. } => (json, "Generated plan"),
            Self::Escalation { json, .. } => (json, "Escalation"),
            Self::Sidecar { json, .. } => (json, "Sidecar exchange"),
            Self::BackendActivation { json, .. } => (json, "Backend activation"),
            Self::Coordinate { json, .. } => (json, "Workflow coordination"),
            Self::Prepare { json, .. } => (json, "Prepared execution"),
            Self::Run { json, .. } => (json, "Run"),
            Self::Status { json, .. } => (json, "Run status"),
            Self::Cancel { json, .. } => (json, "Cancellation"),
            Self::Events { json, .. } => (json, "Run events"),
            Self::SourceState { json, .. } => (json, "Workspace source state"),
            Self::Workflow { json, .. } => (json, "Workflow"),
            Self::Verify { json, .. } => (json, "Verification"),
            Self::Artifacts { json, .. } => (json, "Artifacts"),
            Self::Feedback { json, .. } => (json, "Feedback"),
            Self::Policy { json, .. } => (json, "Policy"),
            Self::Doctor { json, .. } => (json, "Diagnostics"),
            Self::Config { json, .. } => (json, "Effective configuration"),
            Self::RoutePhase { json, .. } => (json, "Routing decision"),
            Self::Backend { json, .. } => (json, "Backend decision"),
            Self::Replay { json, .. } => (json, "Replay"),
            Self::Inspect { json, .. } => (json, "Host observations"),
            Self::Search { json, .. } => (json, "Search results"),
            Self::Index { json, .. } => (json, "Metadata index"),
            Self::Refresh { json, .. } => (json, "Index update"),
            Self::TaskGuide { json, .. } => (json, "Task guide"),
            Self::Normalize { json, .. } => (json, "Task normalization"),
            Self::Plan { json, .. } => (json, "Workflow plan"),
            Self::Assets {
                command: AssetCommand::Review { json, .. },
            } => (json, "Asset review"),
            Self::Audit { command } => match command {
                AuditCommand::Export { json, .. } | AuditCommand::Verify { json, .. } => {
                    (json, "Audit")
                }
            },
            Self::Eval { command } => match command {
                EvalCommand::Seal { json, .. }
                | EvalCommand::Compare { json, .. }
                | EvalCommand::Corpus { json, .. } => (json, "Evaluation"),
            },
            Self::Catalog { command } => match command {
                CatalogCommand::Validate { json, .. }
                | CatalogCommand::List { json, .. }
                | CatalogCommand::Show { json, .. }
                | CatalogCommand::Refresh { json, .. } => (json, "Catalog"),
            },
            Self::Route { .. } | Self::Explain { .. } => return None,
        };
        let human = !*json;
        *json = true;
        human.then_some(title)
    }
}
