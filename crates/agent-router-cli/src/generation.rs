//! Metadata-only global planning. Serialized proposals carry no execution authority.
use crate::{
    CliError, Outcome, command::PhaseArgs, execution, inspect, phases, product, read_bounded,
};
use router_policy::{
    GenerationError, GenerationNode, PhaseRouter, PlanGenerationInput, ValidatedPhaseContext,
    generate_plans,
};
use router_types::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::ExitCode,
};

const MAX_MANIFEST: usize = 1024 * 1024;
const MAX_DOCUMENT: usize = 8 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    node_id: String,
    context: PathBuf,
    host_snapshot: PathBuf,
    catalog: PathBuf,
    policy: Option<PathBuf>,
    asset_authority: Option<PathBuf>,
    dimensions: Option<GenerationDimensions>,
    #[serde(default)]
    evidence: BTreeMap<String, GenerationCandidateEvidence>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    plan: PathBuf,
    workflow_contract: PathBuf,
    policy: PlanGenerationPolicy,
    nodes: Vec<Node>,
    compatibility: Vec<GenerationEdgeCompatibility>,
    snapshot_max_age_seconds: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Document {
    pub schema_version: u32,
    pub input: PathBuf,
    pub source_sha256: String,
    pub planned_at_unix_seconds: u64,
    pub report: PlanGenerationReport,
    #[serde(default)]
    pub selected_proposal_hash: Option<String>,
}

pub(crate) fn error(e: GenerationError) -> CliError {
    CliError::Product(match e {
        GenerationError::Invalid(_) => "generation.invalid",
        GenerationError::Limit(_) | GenerationError::WorkLimit { .. } => "generation.limit",
        GenerationError::Binding => "generation.binding",
        GenerationError::Expired => "generation.evidence.expired",
        GenerationError::Phase(_) => "generation.phase",
        GenerationError::Workflow(_) => "generation.workflow",
    })
}
fn resolve(base: &Path, path: &Path) -> Result<PathBuf, CliError> {
    if path.as_os_str().is_empty() || path.as_os_str().len() > 4096 {
        return Err(CliError::Product("generation.path"));
    }
    std::fs::canonicalize(if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    })
    .map_err(|_| CliError::Product("generation.source.unavailable"))
}
#[derive(Default)]
struct Sources {
    bytes: usize,
    hashes: BTreeMap<PathBuf, (usize, String)>,
}
impl Sources {
    fn capture(&mut self, path: &Path, limit: usize) -> Result<Vec<u8>, CliError> {
        let remaining = MAX_SOURCE_BYTES.saturating_sub(self.bytes);
        let bytes = read_bounded(
            path,
            limit.min(remaining),
            CliError::RequestRead,
            CliError::Product("generation.source.limit"),
        )?;
        self.bytes += bytes.len();
        let hash = router_adapter_api::content_digest(&bytes);
        if self
            .hashes
            .insert(path.to_owned(), (bytes.len(), hash.clone()))
            .is_some_and(|old| old != (bytes.len(), hash))
        {
            return Err(CliError::Product("generation.source.changed"));
        }
        Ok(bytes)
    }
    fn assets(&mut self, path: &Path) -> Result<(), CliError> {
        let bytes = self.capture(path, 256 * 1024)?;
        let value =
            router_adapter_api::strict_json(&bytes).map_err(|_| CliError::RequestJsonInvalid)?;
        let base = path.parent().ok_or(CliError::RequestRead)?;
        for field in ["contracts", "availability", "policy", "requests"] {
            let raw = if matches!(field, "contracts" | "availability") {
                &value[field]["path"]
            } else {
                &value[field]
            };
            let path = raw.as_str().ok_or(CliError::RequestJsonInvalid)?;
            self.capture(&resolve(base, Path::new(path))?, 4 * 1024 * 1024)?;
        }
        // metadata and body_root are deliberately unopened before selection.
        Ok(())
    }
    fn verify(&self) -> Result<(), CliError> {
        for (path, (length, expected)) in &self.hashes {
            let bytes = read_bounded(
                path,
                *length,
                CliError::RequestRead,
                CliError::RequestTooLarge,
            )?;
            if router_adapter_api::content_digest(&bytes) != *expected {
                return Err(CliError::Product("generation.source.changed"));
            }
        }
        Ok(())
    }
    fn hash(&self) -> Result<String, CliError> {
        serde_json::to_vec(&self.hashes)
            .map(|bytes| router_adapter_api::content_digest(&bytes))
            .map_err(|_| CliError::Internal)
    }
}

struct Loaded {
    path: PathBuf,
    source_sha256: String,
    manifest: Manifest,
    plan: WorkflowPlan,
    contract: WorkflowExecutionContract,
    contexts: Vec<(PhaseRouter, ValidatedPhaseContext)>,
}
impl Loaded {
    fn nodes(&self) -> Vec<GenerationNode<'_>> {
        self.manifest
            .nodes
            .iter()
            .zip(&self.contexts)
            .map(|(input, (router, context))| {
                let mut node = GenerationNode::from_context(&input.node_id, router, context);
                if let Some(dimensions) = &input.dimensions {
                    node.dimensions = dimensions.clone();
                }
                node.evidence = input.evidence.clone();
                node
            })
            .collect()
    }
    fn input<'a>(&'a self, nodes: &'a [GenerationNode<'a>], now: u64) -> PlanGenerationInput<'a> {
        PlanGenerationInput {
            plan: &self.plan,
            execution: &self.contract,
            nodes,
            compatibility: &self.manifest.compatibility,
            policy: &self.manifest.policy,
            now_unix_seconds: now,
        }
    }
}
fn load(path: &Path, now: u64) -> Result<Loaded, CliError> {
    let path = resolve(
        &std::env::current_dir().map_err(|_| CliError::RequestRead)?,
        path,
    )?;
    let base = path.parent().ok_or(CliError::RequestRead)?;
    let mut sources = Sources::default();
    let value = router_adapter_api::strict_json(&sources.capture(&path, MAX_MANIFEST)?)
        .map_err(|_| CliError::RequestJsonInvalid)?;
    let manifest: Manifest =
        serde_json::from_value(value).map_err(|_| CliError::RequestJsonInvalid)?;
    if manifest.schema_version != 1
        || manifest.nodes.is_empty()
        || manifest.nodes.len() > router_policy::MAX_GENERATION_NODES
        || manifest.snapshot_max_age_seconds == 0
        || manifest.snapshot_max_age_seconds > 300
    {
        return Err(CliError::Product("generation.manifest"));
    }
    let plan_path = resolve(base, &manifest.plan)?;
    let contract_path = resolve(base, &manifest.workflow_contract)?;
    sources.capture(&plan_path, MAX_DOCUMENT)?;
    sources.capture(&contract_path, MAX_DOCUMENT)?;
    let plan = execution::plan(&plan_path)?;
    let contract = phases::read_json(&contract_path, MAX_DOCUMENT)?;
    let mut args = vec![];
    for node in &manifest.nodes {
        let context = resolve(base, &node.context)?;
        let host_snapshot = resolve(base, &node.host_snapshot)?;
        let catalog = resolve(base, &node.catalog)?;
        sources.capture(&context, router_policy::MAX_PHASE_REQUEST_BYTES)?;
        sources.capture(&host_snapshot, router_adapter_api::MAX_SNAPSHOT_BYTES)?;
        sources.capture(&catalog, crate::MAX_CATALOG_BYTES)?;
        let policy = node.policy.as_ref().map(|p| resolve(base, p)).transpose()?;
        if let Some(p) = &policy {
            sources.capture(p, 64 * 1024)?;
        }
        let asset_authority = node
            .asset_authority
            .as_ref()
            .map(|p| resolve(base, p))
            .transpose()?;
        if let Some(p) = &asset_authority {
            sources.assets(p)?;
        }
        args.push(PhaseArgs {
            context,
            host_snapshot,
            catalog,
            policy,
            policy_store: None,
            asset_authority,
            now: Some(now),
            snapshot_max_age_seconds: manifest.snapshot_max_age_seconds,
        });
    }
    let contexts = args
        .iter()
        .map(phases::load)
        .collect::<Result<Vec<_>, _>>()?;
    sources.verify()?;
    Ok(Loaded {
        path,
        source_sha256: sources.hash()?,
        manifest,
        plan,
        contract,
        contexts,
    })
}
pub(crate) fn run(path: &Path, now: Option<u64>, json: bool) -> Result<Outcome, CliError> {
    if !json {
        return Err(CliError::JsonOutputRequired);
    }
    let now = inspect::current_time(now)?;
    let loaded = load(path, now)?;
    let report = generate_plans(&loaded.input(&loaded.nodes(), now)).map_err(error)?;
    let code = if report.selected.is_some() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    };
    product::json_output(
        &Document {
            schema_version: 1,
            input: loaded.path,
            source_sha256: loaded.source_sha256,
            planned_at_unix_seconds: now,
            report,
            selected_proposal_hash: None,
        },
        code,
    )
}

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum SourceBinding {
    Backend(crate::execution_backend::AuthorityBinding),
    Generated {
        document: PathBuf,
        document_sha256: String,
        input: PathBuf,
        source_sha256: String,
    },
}
pub(crate) struct LoadedDecision {
    pub decision: router_execution_decision::ExecutionDecision,
    pub authority: Option<router_backend_store::BackendStore>,
    pub binding: Option<SourceBinding>,
}
impl LoadedDecision {
    pub fn authority(&self) -> Option<&dyn router_execution_decision::ActivationAuthority> {
        self.authority
            .as_ref()
            .map(|value| value as &dyn router_execution_decision::ActivationAuthority)
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn load_decision(
    backend: Option<&Path>,
    generated: Option<&Path>,
    node_id: &str,
    router: &PhaseRouter,
    context: &ValidatedPhaseContext,
    baseline: &PhaseReceipt,
    scope_sha256: &str,
    historical: bool,
) -> Result<LoadedDecision, CliError> {
    let Some(path) = generated else {
        let loaded = crate::execution_backend::load(
            backend,
            router,
            context,
            baseline,
            scope_sha256,
            historical,
        )?;
        return Ok(LoadedDecision {
            decision: loaded.decision,
            authority: loaded.authority,
            binding: loaded.binding.map(SourceBinding::Backend),
        });
    };
    if backend.is_some() {
        return Err(CliError::Product("generation.composition.denied"));
    }
    let path = resolve(
        &std::env::current_dir().map_err(|_| CliError::RequestRead)?,
        path,
    )?;
    let raw = read_bounded(
        &path,
        MAX_DOCUMENT,
        CliError::RequestRead,
        CliError::RequestTooLarge,
    )?;
    let document: Document = serde_json::from_value(
        router_adapter_api::strict_json(&raw).map_err(|_| CliError::RequestJsonInvalid)?,
    )
    .map_err(|_| CliError::RequestJsonInvalid)?;
    let now = inspect::current_time(None)?;
    if document.schema_version != 1 || document.planned_at_unix_seconds > now {
        return Err(CliError::Product("generation.document"));
    }
    let input = resolve(path.parent().ok_or(CliError::RequestRead)?, &document.input)?;
    let loaded = load(
        &input,
        if historical {
            document.planned_at_unix_seconds
        } else {
            now
        },
    )?;
    if loaded.source_sha256 != document.source_sha256 {
        return Err(CliError::Product("generation.source.changed"));
    }
    let nodes = loaded.nodes();
    let input = loaded.input(&nodes, document.planned_at_unix_seconds);
    let report = generate_plans(&input).map_err(error)?;
    if report != document.report {
        return Err(CliError::Product("generation.report.changed"));
    }
    let proposal = match document.selected_proposal_hash.as_deref() {
        Some(hash) => report
            .selected
            .iter()
            .chain(&report.alternatives)
            .find(|plan| plan.proposal_hash == hash),
        None => report.selected.as_ref(),
    }
    .ok_or(CliError::Product("generation.abstained"))?;
    let proof = router_policy::validate_generated_plan(&input, proposal).map_err(error)?;
    if !historical {
        proof.validate_current(now).map_err(error)?;
    }
    let decision = router_execution_decision::ExecutionDecision::generated(
        router, context, baseline, &proof, node_id,
    )
    .map_err(|_| CliError::Product("generation.context.changed"))?;
    Ok(LoadedDecision {
        decision,
        authority: None,
        binding: Some(SourceBinding::Generated {
            document: path,
            document_sha256: router_adapter_api::content_digest(&raw),
            input: loaded.path,
            source_sha256: loaded.source_sha256,
        }),
    })
}
