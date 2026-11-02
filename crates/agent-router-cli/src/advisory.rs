// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Advisory routing of one normalized task (ADR 0051). It recommends a model,
//! effort and skills per phase, or explains why none is recommended. Nothing is
//! executed, no host or provider is contacted and no authority is granted.
use crate::{
    CliError, Outcome, catalogs, command::ScoringPolicy, human::escaped, inspect,
    phases::read_json, product, render,
};
use clap::Args;
use router_adapter_api::{ALL_CAPABILITIES, MAX_SNAPSHOT_MODELS, ValidatedHostObservation};
use router_catalog::Catalog;
use router_policy::{AdvisoryScoring, EffortProvenance};
use router_types::{
    Capability, CapabilitySupport, Clarification, DataSensitivity, DecisionEnvelope, HostSnapshot,
    ModelId, Mutation, NormalizedTask, RiskEnvironment, RiskLevel, RiskOperation,
    RiskReversibility, RouteDecision, RouteRequest, SkillCandidate, SkillId, SkillTrust, StageKind,
    TaskBudget, TaskContext, TaskInput, TaskKind, TaskRiskContext,
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
    #[arg(long, conflicts_with = "text", required_unless_present = "text")]
    task: Option<PathBuf>,
    /// Explicit TaskRiskContext JSON, such as `risk-context.json` from `task-guide`.
    #[arg(long, conflicts_with = "text")]
    risk_context: Option<PathBuf>,
    /// Inline task objective. Needs `--kind`; replaces `--task`. Stated facts are
    /// `--risk`, `--budget` and `--scope`: omit one and the command asks for it.
    /// Other omitted facts take the fixed defaults listed in docs/advisory-route.md
    /// and are reported with provenance `defaulted`.
    #[arg(long, requires = "kind")]
    text: Option<String>,
    /// Task kind: explore, implement, review, architecture, security or migration.
    #[arg(long, requires = "text")]
    kind: Option<String>,
    /// Task risk: low, medium, high or critical. Never defaulted.
    #[arg(long, requires = "text")]
    risk: Option<String>,
    /// Authorized limits as `MICROUSD:MILLISECONDS:TOKENS`, such as `0:600000:200000`.
    /// Never defaulted; advice enforces none of them.
    #[arg(long, requires = "text")]
    budget: Option<String>,
    /// Relative workspace path in scope; repeatable. Never defaulted.
    #[arg(long = "scope", value_name = "PATH", requires = "text")]
    scopes: Vec<String>,
    /// Data sensitivity: public, internal or secret. Default: internal.
    #[arg(long, requires = "text")]
    sensitivity: Option<String>,
    /// Target environment: local, test, staging, production or unknown. Default: local.
    #[arg(long, requires = "text")]
    environment: Option<String>,
    /// Effect reversibility: reversible, irreversible or unknown. Default: reversible.
    #[arg(long, requires = "text")]
    reversibility: Option<String>,
    /// Intended operation such as `workspace_change` or `deployment`; repeatable.
    /// Default: `workspace_change` for implement and migration, else `read_only`.
    #[arg(long = "operation", value_name = "OPERATION", requires = "text")]
    operations: Vec<String>,
    /// Authorized mutation (`workspace_write`, `network`, `execute_skills`,
    /// `integrate`, or `none`); repeatable. Default: `workspace_write` for
    /// implement and migration, else `none`.
    #[arg(long = "mutation", value_name = "MUTATION", requires = "text")]
    mutations: Vec<String>,
    /// Observable acceptance criterion; repeatable. Default: a fixed manual-review line.
    #[arg(long = "acceptance", value_name = "TEXT", requires = "text")]
    acceptances: Vec<String>,
    /// Host capability the task requires, such as `tools`; repeatable.
    #[arg(
        long = "require-capability",
        value_name = "CAPABILITY",
        requires = "text"
    )]
    required_capabilities: Vec<String>,
    /// Catalog JSON (schema 1, or a generated schema 2 catalog).
    #[arg(long)]
    catalog: PathBuf,
    /// Scoring policy: stable-v2 (default, risk- and kind-aware) or stable-v1 (frozen).
    #[arg(long, value_enum, default_value = "stable-v2")]
    policy: ScoringPolicy,
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
    /// Additive: how `stable-v2` read the catalog. Absent under `stable-v1`.
    #[serde(skip_serializing_if = "Option::is_none")]
    scoring: Option<AdvisoryScoring>,
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
    /// A fixed documented value of the inline entry, used because the operator
    /// stated nothing. It is not a statement by the operator.
    Defaulted,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    would_change: Vec<Change>,
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
    /// For a selection: whether catalog ratings or only the tie-break chose it.
    #[serde(skip_serializing_if = "Option::is_none")]
    selection_basis: Option<&'static str>,
    summary: String,
    phases: Vec<Phase>,
    would_change: Vec<Change>,
}

const DEFAULT_ACCEPTANCE: &str = "The operator judges the result by hand.";

fn parsed<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, CliError> {
    serde_json::from_value(Value::String(value.to_owned()))
        .map_err(|_| error("advisory.task.invalid"))
}

fn parsed_set<T: serde::de::DeserializeOwned + Ord>(
    values: &[String],
) -> Result<BTreeSet<T>, CliError> {
    values.iter().map(|value| parsed(value)).collect()
}

fn parse_budget(value: &str) -> Result<TaskBudget, CliError> {
    let invalid = || error("advisory.budget.invalid");
    let parts: Vec<_> = value.split(':').collect();
    let [cost, elapsed, tokens] = parts.as_slice() else {
        return Err(invalid());
    };
    let number = |part: &str| part.parse::<u64>().map_err(|_| invalid());
    Ok(TaskBudget {
        max_cost_microusd: number(cost)?,
        max_elapsed_ms: number(elapsed)?,
        max_tokens: number(tokens)?,
    })
}

/// The inline task and its explicit risk context. A fact the operator did not
/// state takes a fixed default only when the task contract has no UNKNOWN for
/// it; each such value is returned as a `defaulted` fact. Nothing is inferred
/// from the text. Risk, budget and scope are never defaulted: when absent the
/// normalizer asks for them, as for the environment and reversibility of a
/// migration. `critical` and `irreversible` exist only when stated.
fn quick_task(args: &AdviseArgs) -> Result<(TaskInput, TaskRiskContext, Vec<Fact>), CliError> {
    let objective = args
        .text
        .clone()
        .ok_or_else(|| error("advisory.task.invalid"))?;
    let kind: TaskKind = parsed(
        args.kind
            .as_deref()
            .ok_or_else(|| error("advisory.task.invalid"))?,
    )?;
    let effect_kind = matches!(kind, TaskKind::Implement | TaskKind::Migration);
    let mut defaulted = Vec::new();
    let mut default = |subject: &str, value: Value, flag: &str| {
        defaulted.push(Fact {
            subject: format!("task.{subject}"),
            value,
            provenance: Provenance::Defaulted,
            source: format!("fixed quick-entry default; state it with {flag}"),
        });
    };
    let sensitivity = match args.sensitivity.as_deref() {
        Some(value) => parsed(value)?,
        None => {
            default("sensitivity", json!("internal"), "--sensitivity");
            DataSensitivity::Internal
        }
    };
    // A migration is where an unstated environment or reversibility would hide
    // the risk: it stays UNKNOWN and the normalizer asks for it.
    let migration = kind == TaskKind::Migration;
    let environment = match args.environment.as_deref() {
        Some(value) => parsed(value)?,
        None if migration => RiskEnvironment::Unknown,
        None => {
            default("environment", json!("local"), "--environment");
            RiskEnvironment::Local
        }
    };
    let reversibility = match args.reversibility.as_deref() {
        Some(value) => parsed(value)?,
        None if migration => RiskReversibility::Unknown,
        None => {
            default("reversibility", json!("reversible"), "--reversibility");
            RiskReversibility::Reversible
        }
    };
    let operations: BTreeSet<RiskOperation> = if args.operations.is_empty() {
        let operation = if effect_kind {
            RiskOperation::WorkspaceChange
        } else {
            RiskOperation::ReadOnly
        };
        default("operations", to_value(&[operation])?, "--operation");
        BTreeSet::from([operation])
    } else {
        parsed_set(&args.operations)?
    };
    let allowed_mutations: BTreeSet<Mutation> = if args.mutations.is_empty() {
        let mutations: BTreeSet<Mutation> = if effect_kind {
            BTreeSet::from([Mutation::WorkspaceWrite])
        } else {
            BTreeSet::new()
        };
        default("allowed_mutations", to_value(&mutations)?, "--mutation");
        mutations
    } else if args.mutations.iter().any(|value| value == "none") {
        if args.mutations.len() != 1 {
            return Err(error("advisory.task.invalid"));
        }
        BTreeSet::new()
    } else {
        parsed_set(&args.mutations)?
    };
    let acceptance = if args.acceptances.is_empty() {
        default("acceptance", json!([DEFAULT_ACCEPTANCE]), "--acceptance");
        vec![DEFAULT_ACCEPTANCE.to_owned()]
    } else {
        args.acceptances.clone()
    };
    let input = TaskInput {
        objective,
        kind,
        deliverables: vec![],
        constraints: vec![],
        scope: args.scopes.clone(),
        risk: args.risk.as_deref().map(parsed::<RiskLevel>).transpose()?,
        sensitivity: Some(sensitivity),
        budget: args.budget.as_deref().map(parse_budget).transpose()?,
        allowed_mutations: Some(allowed_mutations),
        dependencies: vec![],
        acceptance,
        required_capabilities: parsed_set(&args.required_capabilities)?,
    };
    let context = TaskRiskContext {
        schema_version: 1,
        environment,
        reversibility,
        operations,
    };
    Ok((input, context, defaulted))
}

fn error(code: &'static str) -> CliError {
    CliError::Product(code)
}

/// Serialized name of a unit enum value; any other shape is an internal error.
fn label(value: &impl Serialize) -> Result<String, CliError> {
    match serde_json::to_value(value) {
        Ok(Value::String(text)) => Ok(text),
        _ => Err(CliError::Internal),
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
    // Same rule as --declare-capability: no egress boundary exists.
    if host.capabilities.contains(&Capability::Network) {
        return Err(error("advisory.declaration.network"));
    }
    Ok(host)
}

/// Overlays operator declarations. A declaration never overrides evidence
/// that the capability is unsupported.
fn apply_declarations(
    host: &mut HostSnapshot,
    knowledge: &mut BTreeMap<Capability, CapabilitySupport>,
    declared: &BTreeSet<Capability>,
) -> Result<(), CliError> {
    for capability in declared {
        if knowledge.get(capability) == Some(&CapabilitySupport::Unsupported) {
            return Err(error("advisory.declaration.contradicts_evidence"));
        }
        knowledge.insert(*capability, CapabilitySupport::Supported);
        host.capabilities.insert(*capability);
    }
    Ok(())
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
                subject: format!("host.capability.{}", label(&capability)?),
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
) -> Result<Vec<SkillRow>, CliError> {
    requested
        .iter()
        .map(|id| {
            let stages = decision
                .selected_plan()
                .map(|plan| {
                    plan.stages
                        .iter()
                        .filter(|stage| stage.skills.contains(id))
                        .map(|stage| label(&stage.kind).map(Value::String))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            let blocked = catalog
                .skill(id)
                .and_then(|skill| blocking_capability(&skill.required_capabilities, request));
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
                Some(_) if blocked.is_some() => {
                    blocked.map_or("not_attached", |(status, _)| status)
                }
                Some(_) if !stages.is_empty() => "suggested",
                Some(_) => "not_attached",
            };
            // Unknown is not a refusal: name the fact that would settle it.
            let would_change = match (status, blocked) {
                ("capability_unknown", Some((_, capability))) => vec![capability_change(
                    "host.capability.unknown",
                    capability,
                    CapabilitySupport::Unknown,
                )?],
                _ => vec![],
            };
            Ok(SkillRow {
                id: id.clone(),
                status,
                stages,
                would_change,
            })
        })
        .collect()
}

/// The status and capability that keep a skill out of the plan, if any. A
/// capability the host is known to lack, or that is absent from the host
/// knowledge, is `capability_not_supported` and outranks `capability_unknown`,
/// which names a capability the host has not been observed either way.
fn blocking_capability(
    required: &BTreeSet<Capability>,
    request: &RouteRequest,
) -> Option<(&'static str, Capability)> {
    let mut unknown = None;
    for capability in required {
        match request.capability_knowledge.get(capability) {
            Some(CapabilitySupport::Supported) => {}
            Some(CapabilitySupport::Unknown) => {
                unknown.get_or_insert(("capability_unknown", *capability));
            }
            Some(CapabilitySupport::Unsupported) | None => {
                return Some(("capability_not_supported", *capability));
            }
        }
    }
    unknown
}

fn capability_change(
    code: &str,
    capability: Capability,
    support: CapabilitySupport,
) -> Result<Change, CliError> {
    let name = label(&capability)?;
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
    Ok(Change {
        code: code.to_owned(),
        capability: Some(capability),
        models: vec![],
        change,
    })
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
fn would_change(
    request: &RouteRequest,
    decision: &DecisionEnvelope,
) -> Result<Vec<Change>, CliError> {
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
            changes.push(capability_change(code, *capability, support)?);
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
    Ok(changes)
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
        let names = declared.iter().map(label).collect::<Result<Vec<_>, _>>()?;
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
                    label(&stage.kind)?,
                    stage.model,
                    label(&stage.effort)?,
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
            // Quality 0 is the catalogs' unrated convention: the score cannot
            // separate such models on quality, so cost, latency and then the
            // identifier order decide.
            let unrated: BTreeSet<&ModelId> = plan
                .stages
                .iter()
                .filter(|stage| {
                    catalog
                        .model(&stage.model)
                        .is_none_or(|model| model.quality_micros == 0)
                })
                .map(|stage| &stage.model)
                .collect();
            let (selection_basis, basis) = if plan
                .stages
                .iter()
                .all(|stage| unrated.contains(&stage.model))
            {
                (
                    "unrated_tie_break",
                    "every selected model is unrated in the catalog, so this choice is a deterministic tie-break, not a quality recommendation.".to_owned(),
                )
            } else if unrated.is_empty() {
                (
                    "catalog_policy_prior",
                    "ratings come from the catalog, a declared policy prior, not a measurement."
                        .to_owned(),
                )
            } else {
                let ids: Vec<_> = unrated.iter().map(|id| id.as_str()).collect();
                (
                    "partly_unrated",
                    format!(
                        "ratings come from the catalog, a declared policy prior, not a measurement; {} unrated, so its selection is not a quality recommendation.",
                        if ids.len() == 1 {
                            format!("{} is", ids[0])
                        } else {
                            format!("{} are", ids.join(", "))
                        }
                    ),
                )
            };
            Ok(NextAction {
                kind: "select_per_phase",
                selection_basis: Some(selection_basis),
                summary: format!(
                    "Nothing was executed. In your {harness} host, select per phase {}. Catalog {}: {basis}{caveat}",
                    steps.join("; "),
                    catalog.version()
                ),
                phases,
                would_change: vec![],
            })
        }
        RouteDecision::NoRoute { .. } => Ok(NextAction {
            kind: "no_recommendation",
            selection_basis: None,
            summary: format!(
                "Nothing was executed and no model is recommended for your {harness} host. Keep your current host choice, or supply one of the facts listed in would_change and run advise again.{caveat}"
            ),
            phases: vec![],
            would_change: would_change(request, decision)?,
        }),
    }
}

fn clarification(questions: Vec<Clarification>, output_requested: bool) -> Report {
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
        scoring: None,
        next_manual_action: NextAction {
            kind: "answer_questions",
            selection_basis: None,
            summary: format!(
                "Nothing was executed. Answer the open questions with task-guide --task <this task> --interactive --output-directory <new directory> --json, then run advise on the exported input.json.{}",
                if output_requested {
                    " The requested output directory was not created."
                } else {
                    ""
                }
            ),
            phases: vec![],
            would_change,
        },
        output_directory: None,
    }
}

pub(crate) fn run(args: AdviseArgs, json: bool) -> Result<Outcome, CliError> {
    let declared = declarations(&args.declare_capability)?;
    let requested = skill_ids(&args.skills)?;
    let (input, risk_context, defaulted_facts): (TaskInput, Option<TaskRiskContext>, Vec<Fact>) =
        if args.text.is_some() {
            let (input, context, defaulted) = quick_task(&args)?;
            (input, Some(context), defaulted)
        } else {
            let path = args
                .task
                .as_deref()
                .ok_or_else(|| error("advisory.task.required"))?;
            let input = read_json(path, MAX_TASK).map_err(|failure| match failure {
                CliError::RequestTooLarge => error("advisory.task.too_large"),
                other => other,
            })?;
            let context = args
                .risk_context
                .as_deref()
                .map(|path| read_json(path, MAX_CONTEXT))
                .transpose()?;
            (input, context, vec![])
        };
    let normalized = router_workflow::normalize_with_risk(input.clone(), risk_context.as_ref())
        .map_err(product::workflow_error)?;
    let Some(task) = normalized.normalization.task.clone() else {
        return output(
            &clarification(
                normalized.normalization.questions,
                args.output_directory.is_some(),
            ),
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
    // `route` always rebinds a catalog's embedded observation, so a declared
    // overlay on it could never replay through `route`; refuse it instead.
    if embedded.is_some() && !declared.is_empty() {
        return Err(error("advisory.declaration.embedded_observation"));
    }
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
    apply_declarations(&mut host, &mut knowledge, &declared)?;

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
    let router = args.policy.router();
    // A declaration is not snapshot evidence: only an unmodified observation
    // binds its hash into the decision identity, exactly as `route` does.
    let scored = match &source {
        HostSource::Observed(snapshot) if declared.is_empty() => {
            router.route_scored_with_snapshot_hash(&request, &catalog, snapshot.hash())
        }
        _ => router.route_scored(&request, &catalog),
    }
    .map_err(|error| {
        if error.is_invalid_request() {
            CliError::RequestInvalid
        } else {
            CliError::Internal
        }
    })?;

    let (decision, scoring) = (scored.decision, scored.scoring);
    let (status, exit) = match decision.decision {
        RouteDecision::Selected { .. } => ("recommended", ExitCode::SUCCESS),
        RouteDecision::NoRoute { .. } => ("no_recommendation", ExitCode::from(3)),
    };
    let mut facts = Facts {
        host: &host,
        source: &source,
        knowledge: &knowledge,
        declared: &declared,
        catalog: &catalog,
        now,
        max_age,
    }
    .build()?;
    // Defaulted task facts follow the task row, before any host fact.
    facts.splice(1..1, defaulted_facts);
    let mut report = Report {
        schema_version: 1,
        mode: "advisory",
        executed: false,
        status,
        questions: vec![],
        mapping: mapping(&input, &task)?,
        facts,
        skills: skill_rows(&requested, &catalog, &request, &decision)?,
        next_manual_action: next_action(&host, &catalog, &declared, &request, &decision)?,
        decision: Some(decision),
        scoring,
        output_directory: None,
    };
    if let Some(path) = &args.output_directory {
        publish(path, &input, risk_context.as_ref(), &request, &mut report)?;
    }
    output(&report, json, exit)
}

fn output(report: &Report, json: bool, exit_code: ExitCode) -> Result<Outcome, CliError> {
    if json {
        return product::json_output(report, exit_code);
    }
    Ok(Outcome {
        output: human(report)?,
        exit_code,
    })
}

/// Plain text for a fact value: lists joined by commas, maps as `key: value`.
fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) if items.is_empty() => "none".into(),
        Value::Array(items) => items.iter().map(plain).collect::<Vec<_>>().join(", "),
        Value::Object(fields) => fields
            .iter()
            .map(|(key, value)| format!("{key}: {}", plain(value)))
            .collect::<Vec<_>>()
            .join("; "),
        other => other.to_string(),
    }
}

/// Short human view. The next manual action is always the last section.
fn human(report: &Report) -> Result<String, CliError> {
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
            let value = plain(&fact.value);
            lines.push(format!(
                "  {} [{}]: {}",
                escaped(&fact.subject),
                label(&fact.provenance)?,
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
        for change in &skill.would_change {
            lines.push(format!("  - {}", escaped(&change.change)));
        }
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
    if let Some(scoring) = &report.scoring {
        lines.push(format!("Scoring ({}):", escaped(&scoring.policy_version)));
        for stage in &scoring.stages {
            let codes = if stage.reason_codes.is_empty() {
                String::new()
            } else {
                format!(" [{}]", stage.reason_codes.join(", "))
            };
            let effort = match stage.effort_provenance {
                EffortProvenance::PolicyTable => format!("effort {}", label(&stage.effort)?),
                EffortProvenance::Fallback => format!(
                    "effort {} (fallback, table asked {})",
                    label(&stage.effort)?,
                    label(&stage.effort_target)?
                ),
            };
            lines.push(format!(
                "  {} target {} chosen {} shortfall {} {}{}",
                label(&stage.stage)?,
                stage.target_micros,
                stage.quality_micros,
                stage.shortfall_micros,
                effort,
                codes
            ));
        }
        lines.push(
            "  Margins and efforts are declared priors, not measurements. Effort cost is not modelled."
                .into(),
        );
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
        let subject = match change.capability {
            Some(capability) => format!(" {}", label(&capability)?),
            None => String::new(),
        };
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
    Ok(format!("{}\n", lines.join("\n")))
}

/// `advice.json` is published last as the completion marker. It records the
/// same `output_directory` as the printed report.
#[cfg(unix)]
fn publish(
    path: &Path,
    input: &TaskInput,
    risk_context: Option<&TaskRiskContext>,
    request: &RouteRequest,
    report: &mut Report,
) -> Result<(), CliError> {
    let fail = || error("advisory.output");
    let destination = crate::private_output::destination(path, "advisory.output")?;
    report.output_directory = Some(destination.clone());
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
        serde_json::to_vec_pretty(&*report).map_err(|_| fail())?,
    ));
    let published = crate::private_output::publish(path, documents, "advisory.output")?;
    if published != destination {
        return Err(fail());
    }
    Ok(())
}

#[cfg(not(unix))]
fn publish(
    _: &Path,
    _: &TaskInput,
    _: Option<&TaskRiskContext>,
    _: &RouteRequest,
    _: &mut Report,
) -> Result<(), CliError> {
    Err(error("advisory.output.unsupported"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> HostSnapshot {
        serde_json::from_value(json!({
            "id": "host", "harness": "codex", "supported_models": [],
            "supported_efforts": {}, "capabilities": []
        }))
        .unwrap()
    }

    #[test]
    fn a_declaration_never_overrides_unsupported_evidence() {
        let mut host = host();
        let mut knowledge = BTreeMap::from([(Capability::Tools, CapabilitySupport::Unsupported)]);
        let declared = BTreeSet::from([Capability::Tools]);
        let refused = apply_declarations(&mut host, &mut knowledge, &declared);
        assert!(matches!(
            refused,
            Err(CliError::Product(
                "advisory.declaration.contradicts_evidence"
            ))
        ));
        assert_eq!(
            knowledge[&Capability::Tools],
            CapabilitySupport::Unsupported
        );
        assert!(host.capabilities.is_empty());
        // Unknown support may be declared; the overlay is applied to both views.
        let mut knowledge = BTreeMap::from([(Capability::Tools, CapabilitySupport::Unknown)]);
        assert!(apply_declarations(&mut host, &mut knowledge, &declared).is_ok());
        assert_eq!(knowledge[&Capability::Tools], CapabilitySupport::Supported);
        assert!(host.capabilities.contains(&Capability::Tools));
    }

    #[test]
    fn capability_unsupported_still_reports_not_supported() {
        // The advisory CLI cannot reach explicit Unsupported without an
        // authenticated runtime proof, so the classification is tested directly.
        let request = |knowledge: &[(Capability, CapabilitySupport)]| RouteRequest {
            task: TaskContext {
                objective: "x".into(),
                kind: router_types::TaskKind::Implement,
                risk: router_types::RiskLevel::Low,
                required_capabilities: BTreeSet::new(),
                acceptance: vec![],
            },
            host: host(),
            skill_candidates: vec![],
            capability_knowledge: knowledge.iter().copied().collect(),
        };
        let required = BTreeSet::from([Capability::Tools]);
        let status = |support: Option<CapabilitySupport>| {
            let knowledge: Vec<_> = support
                .map(|s| (Capability::Tools, s))
                .into_iter()
                .collect();
            blocking_capability(&required, &request(&knowledge))
        };
        assert_eq!(
            status(Some(CapabilitySupport::Unsupported)),
            Some(("capability_not_supported", Capability::Tools))
        );
        assert_eq!(
            status(None),
            Some(("capability_not_supported", Capability::Tools))
        );
        assert_eq!(
            status(Some(CapabilitySupport::Unknown)),
            Some(("capability_unknown", Capability::Tools))
        );
        assert_eq!(status(Some(CapabilitySupport::Supported)), None);
        // Unsupported outranks Unknown when a skill needs both.
        let both = BTreeSet::from([Capability::Tools, Capability::Vision]);
        let mixed = request(&[
            (Capability::Tools, CapabilitySupport::Unknown),
            (Capability::Vision, CapabilitySupport::Unsupported),
        ]);
        assert_eq!(
            blocking_capability(&both, &mixed),
            Some(("capability_not_supported", Capability::Vision))
        );
    }
}
