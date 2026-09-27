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
    #[arg(long)]
    pub now: Option<u64>,
    #[arg(long, default_value_t = 86400)]
    pub snapshot_max_age_seconds: u64,
}

#[derive(Subcommand)]
pub enum Commands {
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
        #[arg(long, default_value = "deterministic", value_parser = ["deterministic", "laya", "jev"])]
        backend: String,
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
        query: String,
        #[arg(long, value_parser = ["agent", "skill", "prompt", "tool"])]
        kind: Option<String>,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        #[arg(long)]
        json: bool,
    },
    Normalize {
        #[arg(long)]
        task: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Plan {
        #[arg(long)]
        task: PathBuf,
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
