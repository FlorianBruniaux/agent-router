#![forbid(unsafe_code)]

mod error;
mod validate;

use std::collections::BTreeMap;

use router_types::{ModelId, ModelSpec, SkillId, SkillSpec};

pub use error::CatalogError;

#[derive(Debug)]
pub struct Catalog {
    version: String,
    models: BTreeMap<ModelId, ModelSpec>,
    skills: BTreeMap<SkillId, SkillSpec>,
}

impl Catalog {
    pub fn from_json(bytes: &[u8]) -> Result<Self, CatalogError> {
        let raw = serde_json::from_slice(bytes)
            .map_err(|error| CatalogError::new("catalog.json.invalid", "$", error.to_string()))?;
        validate::validate(raw)
    }

    pub fn model(&self, id: &ModelId) -> Option<&ModelSpec> {
        self.models.get(id)
    }

    pub fn skill(&self, id: &SkillId) -> Option<&SkillSpec> {
        self.skills.get(id)
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}
