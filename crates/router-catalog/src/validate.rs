use std::collections::{BTreeMap, BTreeSet};

use router_types::{
    Capability, EffortLevel, ModelId, ModelSpec, SkillId, SkillSpec, SkillTrust, StageKind,
    TaskKind,
};
use serde::Deserialize;

use crate::{Catalog, CatalogError, MAX_CATALOG_MODELS, MAX_CATALOG_SKILLS};

#[derive(Deserialize)]
pub(super) struct RawCatalog {
    schema_version: u64,
    version: String,
    models: Vec<RawModelSpec>,
    skills: Vec<RawSkillSpec>,
}

#[derive(Deserialize)]
pub(super) struct RawModelSpec {
    id: String,
    stages: Vec<String>,
    efforts: Vec<String>,
    capabilities: Vec<String>,
    quality_micros: i64,
    cost_micros: i64,
    latency_micros: i64,
    source: String,
}

#[derive(Deserialize)]
pub(super) struct RawSkillSpec {
    id: String,
    description: String,
    task_kinds: Vec<String>,
    required_capabilities: Vec<String>,
    trust: String,
}

pub(super) fn validate(raw: RawCatalog) -> Result<Catalog, CatalogError> {
    if raw.schema_version != 1 {
        return Err(CatalogError::new(
            "catalog.schema.unsupported_version",
            "schema_version",
            "schema version must be exactly 1",
        ));
    }
    if raw.version.is_empty() {
        return Err(CatalogError::new(
            "catalog.version.empty",
            "version",
            "catalog version must not be empty",
        ));
    }
    if raw.models.is_empty() {
        return Err(CatalogError::new(
            "catalog.models.empty",
            "models",
            "catalog must declare at least one model",
        ));
    }
    if raw.models.len() > MAX_CATALOG_MODELS {
        return Err(CatalogError::new(
            "catalog.models.too_many",
            "models",
            format!("catalog must declare at most {MAX_CATALOG_MODELS} models"),
        ));
    }
    if raw.skills.len() > MAX_CATALOG_SKILLS {
        return Err(CatalogError::new(
            "catalog.skills.too_many",
            "skills",
            format!("catalog must declare at most {MAX_CATALOG_SKILLS} skills"),
        ));
    }

    let mut models = BTreeMap::new();
    for (index, raw_model) in raw.models.into_iter().enumerate() {
        let path = format!("models[{index}]");
        let id = parse_model_id(&raw_model.id, &path)?;
        if models.contains_key(&id) {
            return Err(CatalogError::new(
                "catalog.model.duplicate_id",
                format!("{path}.id"),
                format!("model id '{id}' is declared more than once"),
            ));
        }
        if raw_model.stages.is_empty() {
            return Err(CatalogError::new(
                "catalog.model.empty_stages",
                format!("{path}.stages"),
                "model must support at least one stage",
            ));
        }
        if raw_model.efforts.is_empty() {
            return Err(CatalogError::new(
                "catalog.model.empty_efforts",
                format!("{path}.efforts"),
                "model must support at least one effort level",
            ));
        }
        if !(0..=1_000_000).contains(&raw_model.quality_micros) {
            return Err(CatalogError::new(
                "catalog.model.quality_out_of_range",
                format!("{path}.quality_micros"),
                "relative quality must be between 0 and 1_000_000",
            ));
        }
        if raw_model.cost_micros < 0 {
            return Err(CatalogError::new(
                "catalog.model.negative_cost",
                format!("{path}.cost_micros"),
                "relative cost must be non-negative",
            ));
        }
        if raw_model.latency_micros < 0 {
            return Err(CatalogError::new(
                "catalog.model.negative_latency",
                format!("{path}.latency_micros"),
                "relative latency must be non-negative",
            ));
        }
        let stages = parse_stages(raw_model.stages, &path)?;
        let efforts = parse_efforts(raw_model.efforts, &path)?;
        let capabilities = parse_model_capabilities(raw_model.capabilities, &path)?;

        models.insert(
            id.clone(),
            ModelSpec {
                id,
                stages,
                efforts,
                capabilities,
                quality_micros: raw_model.quality_micros,
                cost_micros: raw_model.cost_micros,
                latency_micros: raw_model.latency_micros,
                source: raw_model.source,
            },
        );
    }

    let mut skills = BTreeMap::new();
    for (index, raw_skill) in raw.skills.into_iter().enumerate() {
        let path = format!("skills[{index}]");
        let id = SkillId::parse(raw_skill.id).map_err(|error| {
            CatalogError::new(
                "catalog.skill.invalid_id",
                format!("{path}.id"),
                error.to_string(),
            )
        })?;
        if skills.contains_key(&id) {
            return Err(CatalogError::new(
                "catalog.skill.duplicate_id",
                format!("{path}.id"),
                format!("skill id '{id}' is declared more than once"),
            ));
        }
        let trust = parse_skill_trust(&raw_skill.trust, &path)?;
        let task_kinds = parse_task_kinds(raw_skill.task_kinds, &path)?;
        let required_capabilities =
            parse_skill_capabilities(raw_skill.required_capabilities, &path)?;

        skills.insert(
            id.clone(),
            SkillSpec {
                id,
                description: raw_skill.description,
                task_kinds,
                required_capabilities,
                trust,
            },
        );
    }

    Ok(Catalog {
        version: raw.version,
        models,
        skills,
    })
}

fn parse_model_id(value: &str, path: &str) -> Result<ModelId, CatalogError> {
    ModelId::parse(value).map_err(|error| {
        let code = if matches!(value, "sol" | "terra" | "luna" | "mini" | "spark" | "deep") {
            "catalog.model.non_canonical_id"
        } else {
            "catalog.model.invalid_id"
        };
        CatalogError::new(code, format!("{path}.id"), error.to_string())
    })
}

fn parse_skill_trust(value: &str, path: &str) -> Result<SkillTrust, CatalogError> {
    match value {
        "builtin" => Ok(SkillTrust::Builtin),
        "trusted_local" => Ok(SkillTrust::TrustedLocal),
        "verified_third_party" => Ok(SkillTrust::VerifiedThirdParty),
        "untrusted" => Ok(SkillTrust::Untrusted),
        "blocked" => Ok(SkillTrust::Blocked),
        _ => Err(CatalogError::new(
            "catalog.skill.invalid_trust",
            format!("{path}.trust"),
            "skill trust must be builtin, trusted_local, verified_third_party, untrusted, or blocked",
        )),
    }
}

fn parse_stages(values: Vec<String>, path: &str) -> Result<BTreeSet<StageKind>, CatalogError> {
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| match value.as_str() {
            "plan" => Ok(StageKind::Plan),
            "execute" => Ok(StageKind::Execute),
            "verify" => Ok(StageKind::Verify),
            _ => Err(CatalogError::new(
                "catalog.model.invalid_stage",
                format!("{path}.stages[{index}]"),
                "model stage must be plan, execute, or verify",
            )),
        })
        .collect()
}

fn parse_efforts(values: Vec<String>, path: &str) -> Result<BTreeSet<EffortLevel>, CatalogError> {
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| match value.as_str() {
            "none" => Ok(EffortLevel::None),
            "minimal" => Ok(EffortLevel::Minimal),
            "low" => Ok(EffortLevel::Low),
            "medium" => Ok(EffortLevel::Medium),
            "high" => Ok(EffortLevel::High),
            "xhigh" => Ok(EffortLevel::Xhigh),
            "max" => Ok(EffortLevel::Max),
            "ultra" => Ok(EffortLevel::Ultra),
            _ => Err(CatalogError::new(
                "catalog.model.invalid_effort",
                format!("{path}.efforts[{index}]"),
                "model effort must be none, minimal, low, medium, high, xhigh, max, or ultra",
            )),
        })
        .collect()
}

fn parse_model_capabilities(
    values: Vec<String>,
    path: &str,
) -> Result<BTreeSet<Capability>, CatalogError> {
    parse_capabilities(
        values,
        path,
        "capabilities",
        "catalog.model.invalid_capability",
    )
}

fn parse_skill_capabilities(
    values: Vec<String>,
    path: &str,
) -> Result<BTreeSet<Capability>, CatalogError> {
    parse_capabilities(
        values,
        path,
        "required_capabilities",
        "catalog.skill.invalid_capability",
    )
}

fn parse_capabilities(
    values: Vec<String>,
    path: &str,
    field: &str,
    code: &str,
) -> Result<BTreeSet<Capability>, CatalogError> {
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| match value.as_str() {
            "tools" => Ok(Capability::Tools),
            "subagents" => Ok(Capability::Subagents),
            "vision" => Ok(Capability::Vision),
            "network" => Ok(Capability::Network),
            "workspace_write" => Ok(Capability::WorkspaceWrite),
            _ => Err(CatalogError::new(
                code,
                format!("{path}.{field}[{index}]"),
                "capability must be tools, subagents, vision, network, or workspace_write",
            )),
        })
        .collect()
}

fn parse_task_kinds(values: Vec<String>, path: &str) -> Result<BTreeSet<TaskKind>, CatalogError> {
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| match value.as_str() {
            "explore" => Ok(TaskKind::Explore),
            "implement" => Ok(TaskKind::Implement),
            "review" => Ok(TaskKind::Review),
            "architecture" => Ok(TaskKind::Architecture),
            "security" => Ok(TaskKind::Security),
            "migration" => Ok(TaskKind::Migration),
            _ => Err(CatalogError::new(
                "catalog.skill.invalid_task_kind",
                format!("{path}.task_kinds[{index}]"),
                "skill task kind must be explore, implement, review, architecture, security, or migration",
            )),
        })
        .collect()
}
