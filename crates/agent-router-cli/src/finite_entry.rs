// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Fresh, one-task local-scripted entry. No provider transport or resumable handle.
mod input;
mod output;
#[cfg(feature = "qualification-fresh-t02")]
mod qualification;
pub(crate) mod resume;
mod routing;
mod session;
mod transport;
type EntryResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
// Static stage codes preserve diagnostic context without exposing private errors.
#[derive(Debug)]
struct EntryStage(&'static str);
impl std::fmt::Display for EntryStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for EntryStage {}
use crate::{CliError, Outcome};
use input::Input;
use router_types::{
    Capability, DataSensitivity, EffortLevel, FiniteClientFile, ModelId, Mutation, NormalizedTask,
    RiskLevel, TaskInput, TaskRiskContext,
};
use serde::Deserialize;
use std::time::{Duration, Instant};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::Read,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u32,
    #[cfg(feature = "qualification-fresh-t02")]
    #[serde(default)]
    qualification: Option<qualification::Mode>,
    enabled: bool,
    transport: String,
    root: PathBuf,
    programs: BTreeMap<String, FiniteClientFile>,
    allowed_models: BTreeMap<ModelId, BTreeSet<EffortLevel>>,
}
fn config_bytes(path: &Path) -> Result<Vec<u8>, CliError> {
    let fail = || CliError::Product("finite_entry.config");
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| fail())?;
    if !f.metadata().map_err(|_| fail())?.is_file() {
        return Err(fail());
    }
    let mut bytes = Vec::new();
    f.take(65537).read_to_end(&mut bytes).map_err(|_| fail())?;
    if bytes.len() > 65536 {
        return Err(fail());
    }
    Ok(bytes)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicFile {
    content: String,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submission {
    schema_version: u32,
    kind: String,
    task: TaskInput,
    risk_context: TaskRiskContext,
    model: ModelId,
    effort: EffortLevel,
    output: String,
    public_files: BTreeMap<String, PublicFile>,
}
fn flat(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn validate_submission(s: &Submission, c: &Config) -> Result<NormalizedTask, CliError> {
    let fail = || CliError::Product("finite_entry.task");
    if s.schema_version != 1 || s.kind != "submit" {
        return Err(fail());
    }
    if !c
        .allowed_models
        .get(&s.model)
        .is_some_and(|efforts| efforts.contains(&s.effort))
    {
        return Err(CliError::Product("finite_entry.selection"));
    }
    let normal = router_workflow::normalize_with_risk(s.task.clone(), Some(&s.risk_context))
        .map_err(|_| fail())?;
    let task = normal.normalization.task.ok_or_else(fail)?;
    if !normal.normalization.questions.is_empty()
        || task.sensitivity != DataSensitivity::Public
        || task.risk > RiskLevel::Medium
        || task.scope != [s.output.clone()]
        || !flat(&s.output)
        || ["AGENTS.md", "statement.md", "HARNESS.md"].contains(&s.output.as_str())
        || task.allowed_mutations != BTreeSet::from([Mutation::Network, Mutation::WorkspaceWrite])
        || task.required_capabilities
            != BTreeSet::from([
                Capability::Network,
                Capability::WorkspaceWrite,
                Capability::Tools,
            ])
        || task.budget.max_elapsed_ms == 0
        || task.budget.max_elapsed_ms > 60000
        || task.budget.max_tokens == 0
        || task.budget.max_tokens > 6000
        || task.budget.max_cost_microusd > 200000000
        || s.public_files.len() > 16
        || ["AGENTS.md", "statement.md", s.output.as_str()]
            .iter()
            .any(|n| !s.public_files.contains_key(*n))
        || s.public_files.iter().any(|(n, f)| {
            !flat(n)
                || f.content.len() > 32768
                || router_adapter_api::content_digest(f.content.as_bytes()) != f.sha256
        })
        || s.public_files
            .values()
            .map(|f| f.content.len())
            .sum::<usize>()
            > 131072
    {
        return Err(fail());
    }
    #[cfg(feature = "qualification-fresh-t02")]
    if c.qualification.is_some() {
        qualification::check_submission(s)
            .map_err(|_| CliError::Product("finite_entry.qualification_scope"))?;
    }
    Ok(task)
}
fn validate_config(c: &Config) -> Result<(), CliError> {
    let fail = || CliError::Product("finite_entry.config");
    if c.schema_version != 1 || c.transport != "local_scripted" {
        return Err(fail());
    }
    if !c.enabled {
        return Err(CliError::Product("finite_entry.disabled"));
    }
    if !c.root.is_absolute()
        || c.root.file_name().is_none()
        || c.programs
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != BTreeSet::from([
                "client",
                "native",
                "helper",
                "guard",
                "namespace",
                "profile",
            ])
        || c.programs
            .values()
            .any(|v| !Path::new(&v.path).is_absolute() || !router_adapter_api::is_digest(&v.sha256))
        || c.allowed_models.is_empty()
        || c.allowed_models.len() > 16
        || c.allowed_models.values().any(BTreeSet::is_empty)
    {
        return Err(fail());
    }
    if c.root.symlink_metadata().is_ok() {
        return Err(CliError::Product("finite_entry.fresh_required"));
    }
    Ok(())
}
struct Entry {
    config: Config,
    submission: Submission,
    task: NormalizedTask,
    raw: Vec<u8>,
    config_path: PathBuf,
    config_raw: Vec<u8>,
    input: Input,
    output: output::Output,
    dispatched: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
pub(crate) fn run(path: &Path, json: bool) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let bytes = config_bytes(path)?;
    let value = router_adapter_api::strict_json_limited(&bytes, 65536)
        .map_err(|_| CliError::Product("finite_entry.config"))?;
    let config: Config =
        serde_json::from_value(value).map_err(|_| CliError::Product("finite_entry.config"))?;
    validate_config(&config)?;
    let mut input = Input::stdin()?;
    let raw = input.line(262144, Instant::now() + Duration::from_secs(15))?;
    let v = router_adapter_api::strict_json_limited(&raw, 262144)
        .map_err(|_| CliError::Product("finite_entry.task"))?;
    let submission: Submission =
        serde_json::from_value(v).map_err(|_| CliError::Product("finite_entry.task"))?;
    let task = validate_submission(&submission, &config)?;
    let mut entropy = [0; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut entropy))
        .map_err(|_| CliError::Internal)?;
    let output = output::Output::new(
        router_adapter_api::content_digest(&entropy),
        router_adapter_api::content_digest(&raw),
    )
    .map_err(|_| CliError::Internal)?;
    #[cfg(feature = "qualification-fresh-t02")]
    let output = {
        let mut output = output;
        if config.qualification.is_some() {
            output.qualification = Some(qualification::identity().map_err(|_| CliError::Internal)?);
            if matches!(
                config.qualification,
                Some(qualification::Mode::FreshT02ApplyV1)
            ) {
                output.qualification.as_mut().ok_or(CliError::Internal)?["controller_application"] =
                    serde_json::json!("explicit_accept_apply_v1");
            }
        }
        output
    };
    output.emit("entry.accepted",serde_json::json!({"task":task,"model":submission.model,"effort":submission.effort,"worker_attempts":0})).map_err(|_|CliError::Internal)?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| CliError::Internal)?;
    let dispatched = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let entry = Entry {
        config,
        submission,
        task,
        raw,
        config_path: path.to_path_buf(),
        config_raw: bytes,
        input,
        output,
        dispatched: dispatched.clone(),
    };
    rt.block_on(session::run(entry)).map_err(|error| {
        CliError::Product(if dispatched.load(std::sync::atomic::Ordering::SeqCst) {
            "finite_entry.outcome_unknown"
        } else {
            error
                .downcast_ref::<EntryStage>()
                .map_or("finite_entry.refused", |stage| stage.0)
        })
    })
}
