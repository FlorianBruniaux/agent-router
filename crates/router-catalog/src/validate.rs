use std::collections::BTreeMap;

use router_types::{
    Capability, EffortLevel, ModelId, ModelSpec, SkillId, SkillSpec, SkillTrust, StageKind,
    TaskKind,
};
use serde::Deserialize;

use crate::{Catalog, CatalogError};

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
    stages: Vec<StageKind>,
    efforts: Vec<EffortLevel>,
    capabilities: Vec<Capability>,
    quality_micros: i64,
    cost_micros: i64,
    latency_micros: i64,
    source: String,
}

#[derive(Deserialize)]
pub(super) struct RawSkillSpec {
    id: String,
    description: String,
    task_kinds: Vec<TaskKind>,
    required_capabilities: Vec<Capability>,
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

        models.insert(
            id.clone(),
            ModelSpec {
                id,
                stages: raw_model.stages.into_iter().collect(),
                efforts: raw_model.efforts.into_iter().collect(),
                capabilities: raw_model.capabilities.into_iter().collect(),
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

        skills.insert(
            id.clone(),
            SkillSpec {
                id,
                description: raw_skill.description,
                task_kinds: raw_skill.task_kinds.into_iter().collect(),
                required_capabilities: raw_skill.required_capabilities.into_iter().collect(),
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
    serde_json::from_value(serde_json::Value::String(value.into())).map_err(|_| {
        CatalogError::new(
            "catalog.skill.invalid_trust",
            format!("{path}.trust"),
            "skill trust must be builtin, trusted_local, verified_third_party, untrusted, or blocked",
        )
    })
}
