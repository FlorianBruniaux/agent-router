use router_catalog::Catalog;
use router_types::{DecisionEnvelope, Exclusion, RouteDecision, RouteRequest};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{preset::build_candidates, score::rank_plans};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyConfig {
    schema_version: String,
    policy_version: String,
}

impl PolicyConfig {
    pub fn stable_v1() -> Self {
        Self {
            schema_version: "1".into(),
            policy_version: "stable-v1".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Router {
    config: PolicyConfig,
}

impl Router {
    pub fn new(config: PolicyConfig) -> Self {
        Self { config }
    }

    pub fn stable_v1() -> Self {
        Self::new(PolicyConfig::stable_v1())
    }

    pub fn route(
        &self,
        request: &RouteRequest,
        catalog: &Catalog,
    ) -> Result<DecisionEnvelope, RouteError> {
        let canonical_request =
            serde_json::to_vec(request).map_err(RouteError::SerializeRequest)?;
        let decision_id = decision_id(
            &self.config.schema_version,
            &canonical_request,
            catalog.version(),
            &self.config.policy_version,
        );
        let mut build = build_candidates(request, catalog);
        let mut ranked = rank_plans(&build.stages, request.task.risk, &mut build.exclusions);
        normalize_exclusions(&mut build.exclusions);
        build.missing_evidence.sort();
        build.missing_evidence.dedup();

        let decision = if ranked.is_empty() {
            RouteDecision::NoRoute {
                exclusions: build.exclusions,
                missing_evidence: build.missing_evidence,
            }
        } else {
            let selected = ranked.remove(0).plan;
            let alternatives = ranked
                .into_iter()
                .take(2)
                .map(|ranked| ranked.plan)
                .collect();
            RouteDecision::Selected {
                plan: selected,
                alternatives,
                exclusions: build.exclusions,
                missing_evidence: build.missing_evidence,
            }
        };

        Ok(DecisionEnvelope {
            schema_version: self.config.schema_version.clone(),
            decision_id,
            catalog_version: catalog.version().into(),
            policy_version: self.config.policy_version.clone(),
            decision,
        })
    }
}

#[derive(Debug, Error)]
pub enum RouteError {
    #[error("route.request.serialize: {0}")]
    SerializeRequest(serde_json::Error),
}

fn decision_id(
    schema_version: &str,
    canonical_request: &[u8],
    catalog_version: &str,
    policy_version: &str,
) -> String {
    let mut hasher = Sha256::new();
    for component in [
        schema_version.as_bytes(),
        canonical_request,
        catalog_version.as_bytes(),
        policy_version.as_bytes(),
    ] {
        hasher.update((component.len() as u64).to_be_bytes());
        hasher.update(component);
    }
    hex::encode(hasher.finalize())
}

fn normalize_exclusions(exclusions: &mut [Exclusion]) {
    exclusions.sort_by(|left, right| {
        left.stage
            .cmp(&right.stage)
            .then_with(|| left.model.cmp(&right.model))
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.evidence.cmp(&right.evidence))
    });
}

#[cfg(test)]
mod tests {
    use super::decision_id;

    #[test]
    fn decision_id_components_are_length_prefixed() {
        let left = decision_id("ab", b"c", "d", "e");
        let right = decision_id("a", b"bc", "d", "e");
        assert_ne!(left, right);
    }
}
