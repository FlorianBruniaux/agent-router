//! Advisory routing of one normalized task (ADR 0051). It recommends a model,
//! effort and skills per phase, or explains why none is recommended. Nothing is
//! executed, no host or provider is contacted and no authority is granted.
use crate::{
    CliError, Outcome, catalogs, human::escaped, inspect, phases::read_json, product, render,
};
use clap::Args;
use router_adapter_api::{ALL_CAPABILITIES, MAX_SNAPSHOT_MODELS, ValidatedHostObservation};
use router_catalog::Catalog;
use router_policy::Router;
use router_types::{
    Capability, CapabilitySupport, Clarification, DecisionEnvelope, HostSnapshot, ModelId,
    NormalizedTask, RouteDecision, RouteRequest, SkillCandidate, SkillId, SkillTrust, StageKind,
    TaskContext, TaskInput, TaskRiskContext,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::ExitCode,
};

const MAX_TASK: usize = 1024 * 1024;
const MAX_CONTEXT: usize = 64 * 1024;
const MAX_HOST: usize = 256 * 1024;
const MAX_SKILLS: usize = 64;
/// Declared candidates carry no retrieval relevance; they only allow attachment.
const SKILL_SOURCE: &str = "operator-declared";
const NOT_ENFORCED: &str = "not_enforced_by_advisory_routing";

#[derive(Args)]
pub(crate) struct AdviseArgs {
    /// Complete strict TaskInput JSON, such as `input.json` from `task-guide`.
    #[arg(long)]
    task: PathBuf,
    /// Explicit TaskRiskContext JSON, such as `risk-context.json` from `task-guide`.
    #[arg(long)]
    risk_context: Option<PathBuf>,
    /// Catalog JSON (schema 1, or a generated schema 2 catalog).
    #[arg(long)]
    catalog: PathBuf,
    /// Host observation from `inspect`; its models and efforts are discovered.
    #[arg(long, conflicts_with = "host")]
    host_snapshot: Option<PathBuf>,
    /// Operator-declared HostSnapshot JSON; its values are reported as declared.
    #[arg(long)]
    host: Option<PathBuf>,
    /// Declare one host capability as supported. Reported as declared, never verified.
    #[arg(long = "declare-capability", value_name = "CAPABILITY")]
    declare_capability: Vec<String>,
    /// Candidate skill identifier. Only compatible catalog skills are suggested.
    #[arg(long = "skill", value_name = "ID")]
    skills: Vec<String>,
    /// Maximum accepted host snapshot age, in seconds.
    #[arg(long, default_value_t = 86400)]
    snapshot_max_age_seconds: u64,
    /// Current time override, in Unix seconds (for fixtures and replays).
    #[arg(long)]
    now: Option<u64>,
    /// Export inputs and results into a new private directory; never replace a path.
    #[arg(long)]
    output_directory: Option<PathBuf>,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    mode: &'static str,
    executed: bool,
    status: &'static str,
    questions: Vec<Clarification>,
    mapping: Vec<Mapping>,
    facts: Vec<Fact>,
    skills: Vec<SkillRow>,
    decision: Option<DecisionEnvelope>,
    next_manual_action: NextAction,
    output_directory: Option<PathBuf>,
}

#[derive(Serialize)]
struct Mapping {
    field: &'static str,
    handling: &'static str,
    value: Value,
    note: &'static str,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Provenance {
    /// Stated by the operator, a host file or a catalog prior; not checked.
    Declared,
    /// Read from a host observation whose integrity was checked; not authenticated.
    Discovered,
    /// Checked by this command or by authenticated evidence.
    Verified,
    Unknown,
}

#[derive(Serialize)]
struct Fact {
    subject: String,
    value: Value,
    provenance: Provenance,
    source: String,
}

#[derive(Serialize)]
struct SkillRow {
    id: SkillId,
    status: &'static str,
    stages: Vec<Value>,
}

#[derive(Serialize)]
struct Phase {
    stage: Value,
    model: String,
    effort: Value,
    skills: Vec<SkillId>,
    /// Run this phase in a fresh session, independent of the earlier phases.
    fresh_instance: bool,
}

#[derive(Serialize)]
struct Change {
    code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    capability: Option<Capability>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    models: Vec<ModelId>,
    change: String,
}

#[derive(Serialize)]
struct NextAction {
    kind: &'static str,
    summary: String,
    phases: Vec<Phase>,
    would_change: Vec<Change>,
}

fn error(code: &'static str) -> CliError {
    CliError::Product(code)
}

fn label(value: &impl Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(text)) => text,
        _ => String::new(),
    }
}

fn to_value(value: &impl Serialize) -> Result<Value, CliError> {
    serde_json::to_value(value).map_err(|_| CliError::Internal)
}

/// Same identifier rule as host observations.
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:/".contains(&byte))
}

/// Where the host facts came from, before any declaration overlay.
enum HostSource {
    Declared,
    Observed(ValidatedHostObservation),
}

fn declarations(values: &[String]) -> Result<BTreeSet<Capability>, CliError> {
    let mut declared = BTreeSet::new();
    for value in values {
        let capability: Capability = serde_json::from_value(Value::String(value.clone()))
            .map_err(|_| error("advisory.declaration.invalid"))?;
        // No egress boundary exists to make a network declaration meaningful.
        if capability == Capability::Network {
            return Err(error("advisory.declaration.network"));
        }
        declared.insert(capability);
    }
    Ok(declared)
}

fn skill_ids(values: &[String]) -> Result<BTreeSet<SkillId>, CliError> {
    if values.len() > MAX_SKILLS {
        return Err(error("advisory.skills.too_many"));
    }
    values
        .iter()
        .map(|value| {
            if !identifier(value) {
                return Err(error("advisory.skill.invalid"));
            }
            SkillId::parse(value.clone()).map_err(|_| error("advisory.skill.invalid"))
        })
        .collect()
}

fn declared_host(path: &Path) -> Result<HostSnapshot, CliError> {
    let host: HostSnapshot = read_json(path, MAX_HOST)?;
    if !identifier(&host.id)
        || !identifier(host.harness.as_str())
        || host.supported_models.len() > MAX_SNAPSHOT_MODELS
        || host
            .supported_efforts
            .keys()
            .any(|model| !host.supported_models.contains(model))
    {
        return Err(error("advisory.host.invalid"));
    }
    Ok(host)
}

fn mapping(input: &TaskInput, task: &NormalizedTask) -> Result<Vec<Mapping>, CliError> {
    let routed = |field, value, note| Mapping {
        field,
        handling: "routed",
        value,
        note,
    };
    let kept = |field, value, note| Mapping {
        field,
        handling: NOT_ENFORCED,
        value,
        note,
    };
    Ok(vec![
        routed(
            "objective",
            to_value(&task.objective)?,
            "Recorded in the route request; it does not change eligibility.",
        ),
        routed(
            "kind",
            to_value(&task.kind)?,
            "Selects compatible skills and scores plans.",
        ),
        routed(
            "risk",
            json!({"declared": input.risk, "effective": task.risk}),
            "The effective risk after the normalizer's floor is routed.",
        ),
        routed(
            "required_capabilities",
            to_value(&task.required_capabilities)?,
            "Each capability must be supported by the host and the model.",
        ),
        routed(
            "acceptance",
            to_value(&task.acceptance)?,
            "Recorded in the route request; it does not verify the result.",
        ),
        kept(
            "scope",
            to_value(&task.scope)?,
            "Paths are not restricted by a recommendation; restrict the host session yourself.",
        ),
        kept(
            "budget",
            to_value(&task.budget)?,
            "No cost, token or time admission is applied; fit is not checked.",
        ),
        kept(
            "allowed_mutations",
            to_value(&task.allowed_mutations)?,
            "No permission is granted or enforced; your host keeps its own permission settings.",
        ),
        kept(
            "sensitivity",
            to_value(&task.sensitivity)?,
            "Data handling is not checked; your host and provider decide where data goes.",
        ),
        kept(
            "deliverables",
            to_value(&task.deliverables)?,
            "Kept for the operator; not used to select a model.",
        ),
        kept(
            "constraints",
            to_value(&task.constraints)?,
            "Kept for the operator; not used to select a model.",
        ),
        kept(
            "dependencies",
            to_value(&task.dependencies)?,
            "Kept for the operator; not used to select a model.",
        ),
    ])
}

struct Facts<'a> {
    host: &'a HostSnapshot,
    source: &'a HostSource,
    knowledge: &'a BTreeMap<Capability, CapabilitySupport>,
    declared: &'a BTreeSet<Capability>,
    catalog: &'a Catalog,
    now: u64,
    max_age: u64,
}

impl Facts<'_> {
    fn build(&self) -> Result<Vec<Fact>, CliError> {
        let (provenance, source) = match self.source {
            HostSource::Declared => (Provenance::Declared, "operator host file".to_owned()),
            HostSource::Observed(snapshot) => (
                Provenance::Discovered,
                format!("host observation {}", snapshot.hash()),
            ),
        };
        let mut facts = vec![
            Fact {
                subject: "task".into(),
                value: Value::String("explicit task facts".into()),
                provenance: Provenance::Declared,
                source: "operator answers; risk floor from the normalizer rules".into(),
            },
            Fact {
                subject: "host.id".into(),
                value: json!({"id": self.host.id, "harness": self.host.harness}),
                provenance,
                source: source.clone(),
            },
        ];
        if let HostSource::Observed(snapshot) = self.source {
            facts.push(Fact {
                subject: "host.snapshot".into(),
                value: Value::String(snapshot.hash().into()),
                provenance: Provenance::Verified,
                source: format!(
                    "content digest recomputed; observed at most {} seconds before {}",
                    self.max_age, self.now
                ),
            });
        }
        facts.push(Fact {
            subject: "host.models".into(),
            value: to_value(&self.host.supported_models)?,
            provenance,
            source: source.clone(),
        });
        facts.push(Fact {
            subject: "host.efforts".into(),
            value: to_value(&self.host.supported_efforts)?,
            provenance,
            source: source.clone(),
        });
        for capability in ALL_CAPABILITIES {
            let support = self
                .knowledge
                .get(&capability)
                .copied()
                .unwrap_or(CapabilitySupport::Unknown);
            let (provenance, source) = if self.declared.contains(&capability) {
                (Provenance::Declared, "--declare-capability".to_owned())
            } else if support == CapabilitySupport::Unknown {
                (Provenance::Unknown, "no evidence supplied".to_owned())
            } else {
                match self.source {
                    HostSource::Declared => (Provenance::Declared, "operator host file".into()),
                    HostSource::Observed(_) => (
                        Provenance::Verified,
                        "authenticated capability proof".into(),
                    ),
                }
            };
            facts.push(Fact {
                subject: format!("host.capability.{}", label(&capability)),
                value: to_value(&support)?,
                provenance,
                source,
            });
        }
        let ratings: BTreeMap<_, _> = self
            .host
            .supported_models
            .iter()
            .filter_map(|id| self.catalog.model(id))
            .map(|model| (model.id.clone(), model.source.clone()))
            .collect();
        facts.push(Fact {
            subject: "catalog.models".into(),
            value: json!({"catalog_version": self.catalog.version(), "rating_sources": ratings}),
            provenance: Provenance::Declared,
            source: "catalog policy prior; ratings are not measurements".into(),
        });
        Ok(facts)
    }
}

fn skill_rows(
    requested: &BTreeSet<SkillId>,
    catalog: &Catalog,
    request: &RouteRequest,
    decision: &DecisionEnvelope,
) -> Vec<SkillRow> {
    let supported = |capability: &Capability| {
        request.capability_knowledge.get(capability) == Some(&CapabilitySupport::Supported)
    };
    requested
        .iter()
        .map(|id| {
            let stages: Vec<Value> = decision
                .selected_plan()
                .map(|plan| {
                    plan.stages
                        .iter()
                        .filter(|stage| stage.skills.contains(id))
                        .map(|stage| Value::String(label(&stage.kind)))
                        .collect()
                })
                .unwrap_or_default();
            let status = match catalog.skill(id) {
                None => "absent_from_catalog",
                Some(skill)
                    if !matches!(
                        skill.trust,
                        SkillTrust::Builtin
                            | SkillTrust::TrustedLocal
                            | SkillTrust::VerifiedThirdParty
                    ) =>
                {
                    "untrusted"
                }
                Some(skill) if !skill.task_kinds.contains(&request.task.kind) => {
                    "incompatible_task_kind"
                }
                Some(skill) if !skill.required_capabilities.iter().all(supported) => {
                    "capability_not_supported"
                }
                Some(_) if !stages.is_empty() => "suggested",
                Some(_) => "not_attached",
            };
            SkillRow {
                id: id.clone(),
                status,
                stages,
            }
        })
        .collect()
}

fn capability_change(code: &str, capability: Capability, support: CapabilitySupport) -> Change {
    let name = label(&capability);
    let change = match support {
        CapabilitySupport::Unknown if capability == Capability::Network => {
            "Network support cannot be declared in advisory mode; it needs a provider egress boundary (ADR 0022).".to_owned()
        }
        CapabilitySupport::Unknown => format!(
            "Supply authenticated host evidence for '{name}', or declare it with --declare-capability {name}; a declaration is reported as declared, not verified."
        ),
        CapabilitySupport::Unsupported => format!(
            "Use a host that supports '{name}', or remove it from required_capabilities if the task does not need it."
        ),
        CapabilitySupport::Supported => format!(
            "No offered catalog model declares '{name}'; this needs a reviewed catalog revision or another model."
        ),
    };
    Change {
        code: code.to_owned(),
        capability: Some(capability),
        models: vec![],
        change,
    }
}

fn generic_change(code: &str, models: Vec<ModelId>) -> Change {
    let change = match code {
        "host.model.unavailable" => {
            "The host does not offer these catalog models; offering one would make it a candidate."
        }
        _ => "The host offers none of the efforts the catalog accepts for these models.",
    };
    Change {
        code: code.to_owned(),
        capability: None,
        models,
        change: change.to_owned(),
    }
}

/// Facts an operator could supply to change a no-route decision. Each excluded
/// (stage, model) pair counts once, under the reason that blocks it whatever
/// the effort: an unoffered model, then a stage the catalog does not declare
/// (a catalog fact, not an operator one), then a capability, then efforts.
fn would_change(request: &RouteRequest, decision: &DecisionEnvelope) -> Vec<Change> {
    let mut pairs = BTreeMap::<(StageKind, &ModelId), BTreeSet<&str>>::new();
    for exclusion in decision.exclusions() {
        pairs
            .entry((exclusion.stage, &exclusion.model))
            .or_default()
            .insert(exclusion.code.as_str());
    }
    let mut capability_codes = BTreeSet::new();
    let mut generic = BTreeMap::<&str, BTreeSet<ModelId>>::new();
    for ((_, model), codes) in pairs {
        if codes.contains("host.model.unavailable") {
            generic
                .entry("host.model.unavailable")
                .or_default()
                .insert(model.clone());
        } else if codes.contains("model.stage.unsupported") {
            continue;
        } else if let Some(code) = codes.iter().copied().find(|code| {
            matches!(
                *code,
                "host.capability.unknown"
                    | "host.capability.unknown_high_risk"
                    | "host.capability.missing"
                    | "model.capability.unsupported"
            )
        }) {
            capability_codes.insert(code);
        } else {
            generic
                .entry("host.effort.unsupported")
                .or_default()
                .insert(model.clone());
        }
    }
    let mut changes = Vec::new();
    for capability in &request.task.required_capabilities {
        let support = request
            .capability_knowledge
            .get(capability)
            .copied()
            .unwrap_or(CapabilitySupport::Unknown);
        let code = match support {
            CapabilitySupport::Unknown => [
                "host.capability.unknown",
                "host.capability.unknown_high_risk",
            ]
            .into_iter()
            .find(|code| capability_codes.contains(code)),
            CapabilitySupport::Unsupported => {
                Some("host.capability.missing").filter(|code| capability_codes.contains(code))
            }
            CapabilitySupport::Supported => {
                ["host.capability.missing", "model.capability.unsupported"]
                    .into_iter()
                    .find(|code| capability_codes.contains(code))
            }
        };
        if let Some(code) = code {
            changes.push(capability_change(code, *capability, support));
        }
    }
    for (code, models) in generic {
        changes.push(generic_change(code, models.into_iter().collect()));
    }
    if changes.is_empty() {
        changes.push(Change {
            code: "route.no_operator_fact".into(),
            capability: None,
            models: vec![],
            change: "No operator-supplied fact changes this result; the catalog declares no model for a required phase, or no plan passed the policy. See the decision exclusions.".into(),
        });
    }
    changes
}

fn next_action(
    host: &HostSnapshot,
    catalog: &Catalog,
    declared: &BTreeSet<Capability>,
    request: &RouteRequest,
    decision: &DecisionEnvelope,
) -> Result<NextAction, CliError> {
    let harness = host.harness.as_str();
    let mut caveat = String::new();
    if request.task.required_capabilities.is_empty() {
        caveat.push_str(" The task requires no capability, so no host capability was checked.");
    }
    if !declared.is_empty() {
        let names: Vec<_> = declared.iter().map(label).collect();
        caveat.push_str(&format!(
            " Support for {} is declared by you, not verified.",
            names.join(", ")
        ));
    }
    match &decision.decision {
        RouteDecision::Selected { plan, .. } => {
            let mut phases = Vec::new();
            let mut steps = Vec::new();
            for stage in &plan.stages {
                let skills = if stage.skills.is_empty() {
                    "no skill".to_owned()
                } else {
                    let ids: Vec<_> = stage.skills.iter().map(SkillId::as_str).collect();
                    format!("skills {}", ids.join(", "))
                };
                let fresh = if stage.verification_independence.is_some() {
                    ", in a fresh session"
                } else {
                    ""
                };
                steps.push(format!(
                    "{}: model {} at effort {} with {}{fresh}",
                    label(&stage.kind),
                    stage.model,
                    label(&stage.effort),
                    skills
                ));
                phases.push(Phase {
                    stage: to_value(&stage.kind)?,
                    model: stage.model.to_string(),
                    effort: to_value(&stage.effort)?,
                    skills: stage.skills.clone(),
                    fresh_instance: stage.verification_independence.is_some(),
                });
            }
            Ok(NextAction {
                kind: "select_per_phase",
                summary: format!(
                    "Nothing was executed. In your {harness} host, select per phase {}. Ratings come from catalog {}, a declared policy prior, not a measurement.{caveat}",
                    steps.join("; "),
                    catalog.version()
                ),
                phases,
                would_change: vec![],
            })
        }
        RouteDecision::NoRoute { .. } => Ok(NextAction {
            kind: "no_recommendation",
            summary: format!(
                "Nothing was executed and no model is recommended for your {harness} host. Keep your current host choice, or supply one of the facts listed in would_change and run advise again.{caveat}"
            ),
            phases: vec![],
            would_change: would_change(request, decision),
        }),
    }
}

fn clarification(questions: Vec<Clarification>) -> Report {
    let would_change = questions
        .iter()
        .map(|question| Change {
            code: question.code.clone(),
            capability: None,
            models: vec![],
            change: question.question.clone(),
        })
        .collect();
    Report {
        schema_version: 1,
        mode: "advisory",
        executed: false,
        status: "needs_clarification",
        questions,
        mapping: vec![],
        facts: vec![],
        skills: vec![],
        decision: None,
        next_manual_action: NextAction {
            kind: "answer_questions",
            summary: "Nothing was executed. Answer the open questions with task-guide --task <this task> --interactive --output-directory <new directory> --json, then run advise on the exported input.json.".into(),
            phases: vec![],
            would_change,
        },
        output_directory: None,
    }
}

pub(crate) fn run(args: AdviseArgs, json: bool) -> Result<Outcome, CliError> {
    let declared = declarations(&args.declare_capability)?;
    let requested = skill_ids(&args.skills)?;
    let input: TaskInput = read_json(&args.task, MAX_TASK)?;
    let risk_context: Option<TaskRiskContext> = args
        .risk_context
        .as_deref()
        .map(|path| read_json(path, MAX_CONTEXT))
        .transpose()?;
    let normalized = router_workflow::normalize_with_risk(input.clone(), risk_context.as_ref())
        .map_err(product::workflow_error)?;
    let Some(task) = normalized.normalization.task.clone() else {
        return output(
            &clarification(normalized.normalization.questions),
            json,
            ExitCode::from(3),
        );
    };
    router_workflow::validate_task(&task).map_err(product::workflow_error)?;

    let now = inspect::current_time(args.now)?;
    let max_age = args.snapshot_max_age_seconds;
    let (catalog, embedded) = catalogs::load(&args.catalog, Some(now), max_age)?;
    let explicit = args
        .host_snapshot
        .as_deref()
        .map(|path| inspect::read_snapshot(path, Some(now), max_age))
        .transpose()?;
    let declared_file = args.host.as_deref().map(declared_host).transpose()?;
    let (source, mut host, mut knowledge) = match (explicit, embedded, declared_file) {
        (Some(explicit), Some(embedded), _) if explicit.hash() != embedded.hash() => {
            return Err(error("catalog.snapshot.mismatch"));
        }
        (Some(snapshot), _, None) | (None, Some(snapshot), None) => {
            let host = snapshot.host().clone();
            let knowledge = snapshot.capabilities();
            (HostSource::Observed(snapshot), host, knowledge)
        }
        (None, None, Some(host)) => {
            // An unlisted capability is not declared absent; it stays unknown.
            let knowledge = ALL_CAPABILITIES
                .into_iter()
                .map(|capability| {
                    let support = if host.capabilities.contains(&capability) {
                        CapabilitySupport::Supported
                    } else {
                        CapabilitySupport::Unknown
                    };
                    (capability, support)
                })
                .collect();
            (HostSource::Declared, host, knowledge)
        }
        (_, Some(_), Some(_)) | (Some(_), None, Some(_)) => {
            return Err(error("advisory.host.conflict"));
        }
        (None, None, None) => return Err(error("advisory.host.required")),
    };
    // A model the catalog cannot rate is refused, never guessed or skipped.
    if host
        .supported_models
        .iter()
        .any(|model| catalog.model(model).is_none())
    {
        return Err(error("advisory.model.unknown"));
    }
    for capability in &declared {
        knowledge.insert(*capability, CapabilitySupport::Supported);
        host.capabilities.insert(*capability);
    }

    let request = RouteRequest {
        task: TaskContext {
            objective: task.objective.clone(),
            kind: task.kind,
            risk: task.risk,
            required_capabilities: task.required_capabilities.clone(),
            acceptance: task.acceptance.clone(),
        },
        host: host.clone(),
        skill_candidates: requested
            .iter()
            .map(|skill| SkillCandidate {
                skill: skill.clone(),
                source: SKILL_SOURCE.into(),
                score_micros: 0,
            })
            .collect(),
        capability_knowledge: knowledge.clone(),
    };
    let router = Router::stable_v1();
    // A declaration is not snapshot evidence: only an unmodified observation
    // binds its hash into the decision identity, exactly as `route` does.
    let decision = match &source {
        HostSource::Observed(snapshot) if declared.is_empty() => {
            router.route_with_snapshot_hash(&request, &catalog, snapshot.hash())
        }
        _ => router.route(&request, &catalog),
    }
    .map_err(|error| {
        if error.is_invalid_request() {
            CliError::RequestInvalid
        } else {
            CliError::Internal
        }
    })?;

    let (status, exit) = match decision.decision {
        RouteDecision::Selected { .. } => ("recommended", ExitCode::SUCCESS),
        RouteDecision::NoRoute { .. } => ("no_recommendation", ExitCode::from(3)),
    };
    let facts = Facts {
        host: &host,
        source: &source,
        knowledge: &knowledge,
        declared: &declared,
        catalog: &catalog,
        now,
        max_age,
    }
    .build()?;
    let mut report = Report {
        schema_version: 1,
        mode: "advisory",
        executed: false,
        status,
        questions: vec![],
        mapping: mapping(&input, &task)?,
        facts,
        skills: skill_rows(&requested, &catalog, &request, &decision),
        next_manual_action: next_action(&host, &catalog, &declared, &request, &decision)?,
        decision: Some(decision),
        output_directory: None,
    };
    if let Some(path) = &args.output_directory {
        report.output_directory = Some(publish(
            path,
            &input,
            risk_context.as_ref(),
            &request,
            &report,
        )?);
    }
    output(&report, json, exit)
}

fn output(report: &Report, json: bool, exit_code: ExitCode) -> Result<Outcome, CliError> {
    if json {
        return product::json_output(report, exit_code);
    }
    Ok(Outcome {
        output: human(report),
        exit_code,
    })
}

/// Short human view. The next manual action is always the last section.
fn human(report: &Report) -> String {
    let mut lines = vec![
        "Advisory route: nothing was executed".to_owned(),
        format!("Status: {}", report.status),
    ];
    for question in &report.questions {
        lines.push(format!(
            "Question {}: {}",
            escaped(&question.code),
            escaped(&question.question)
        ));
    }
    if !report.facts.is_empty() {
        lines.push("Facts:".into());
        for fact in &report.facts {
            let value = match &fact.value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            lines.push(format!(
                "  {} [{}]: {}",
                escaped(&fact.subject),
                label(&fact.provenance),
                escaped(&value)
            ));
        }
    }
    let ignored: Vec<_> = report
        .mapping
        .iter()
        .filter(|row| row.handling == NOT_ENFORCED)
        .map(|row| row.field)
        .collect();
    if !ignored.is_empty() {
        lines.push(format!(
            "Not enforced by advisory routing: {}",
            ignored.join(", ")
        ));
    }
    for skill in &report.skills {
        lines.push(format!(
            "Skill {}: {}",
            escaped(skill.id.as_str()),
            skill.status
        ));
    }
    if let Some(decision) = &report.decision {
        lines.extend(
            render::explain(decision)
                .lines()
                .take_while(|line| !line.starts_with("Stage: "))
                .map(str::to_owned),
        );
        if decision.selected_plan().is_none() {
            lines.push(format!(
                "Excluded candidates: {}",
                decision.exclusions().len()
            ));
        }
    }
    if let Some(path) = &report.output_directory {
        lines.push(format!(
            "Saved to: {}",
            escaped(&path.display().to_string())
        ));
    }
    let action = &report.next_manual_action;
    lines.push("Next manual action:".into());
    lines.push(format!("  {}", escaped(&action.summary)));
    for change in &action.would_change {
        let subject = change
            .capability
            .map(|capability| format!(" {}", label(&capability)))
            .unwrap_or_default();
        lines.push(format!(
            "  - {}{}: {}",
            escaped(&change.code),
            subject,
            escaped(&change.change)
        ));
        if !change.models.is_empty() {
            let models: Vec<_> = change
                .models
                .iter()
                .map(|model| escaped(model.as_str()))
                .collect();
            lines.push(format!("    models: {}", models.join(", ")));
        }
    }
    format!("{}\n", lines.join("\n"))
}

/// `advice.json` is published last as the completion marker.
#[cfg(unix)]
fn publish(
    path: &Path,
    input: &TaskInput,
    risk_context: Option<&TaskRiskContext>,
    request: &RouteRequest,
    report: &Report,
) -> Result<PathBuf, CliError> {
    let fail = || error("advisory.output");
    let decision = report.decision.as_ref().ok_or_else(fail)?;
    let mut documents = vec![(
        "task.json",
        serde_json::to_vec_pretty(input).map_err(|_| fail())?,
    )];
    if let Some(context) = risk_context {
        documents.push((
            "risk-context.json",
            serde_json::to_vec_pretty(context).map_err(|_| fail())?,
        ));
    }
    documents.push((
        "route-request.json",
        serde_json::to_vec_pretty(request).map_err(|_| fail())?,
    ));
    let mut decision = render::decision_json(decision).map_err(|_| fail())?;
    decision.pop();
    documents.push(("decision.json", decision.into_bytes()));
    documents.push((
        "advice.json",
        serde_json::to_vec_pretty(report).map_err(|_| fail())?,
    ));
    crate::private_output::publish(path, documents, "advisory.output")
}

#[cfg(not(unix))]
fn publish(
    _: &Path,
    _: &TaskInput,
    _: Option<&TaskRiskContext>,
    _: &RouteRequest,
    _: &Report,
) -> Result<PathBuf, CliError> {
    Err(error("advisory.output.unsupported"))
}
