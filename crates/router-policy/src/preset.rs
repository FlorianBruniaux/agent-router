// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
use std::collections::BTreeMap;

use router_catalog::Catalog;
use router_types::{
    Capability, CapabilitySupport, EffortLevel, Exclusion, HostSnapshot, ModelSpec, RouteRequest,
    SkillId, SkillTrust, StageKind,
};

use crate::{
    CapabilityKnowledge, ConstraintEngine, ConstraintVerdict, EligibilityContext, Support,
    is_eligible,
};

pub(crate) const PRESET_NAME: &str = "plan-execute-verify";

/// Source of a skill candidate the operator named with `--skill`.
pub const OPERATOR_DECLARED_SOURCE: &str = "operator-declared";
/// `stable-v2` attaches at most this many skills to a stage (ADR 0054).
pub const MAX_STAGE_SKILLS: usize = 2;
/// `stable-v2` attaches a non-operator candidate only from this score (ADR 0054).
pub const SKILL_FLOOR_MICROS: i64 = 300_000;

/// How compatible skills become stage skills.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SkillSelection {
    /// `stable-v1`, frozen: every compatible candidate, ordered by id.
    All,
    /// `stable-v2`: operator rows first, then score descending, then id, above
    /// the floor for non-operator rows, at most [`MAX_STAGE_SKILLS`].
    Capped,
}

#[derive(Clone, Debug)]
pub(crate) struct AttachedSkill {
    pub(crate) id: SkillId,
    pub(crate) score_micros: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct StageCandidate {
    pub(crate) kind: StageKind,
    pub(crate) model: ModelSpec,
    pub(crate) effort: EffortLevel,
    /// Effort the policy asked for; `effort` differs when the host lacks it.
    pub(crate) preferred_effort: EffortLevel,
    pub(crate) skills: Vec<AttachedSkill>,
}

#[derive(Debug)]
pub(crate) struct CandidateBuild {
    pub(crate) stages: Vec<Vec<StageCandidate>>,
    pub(crate) exclusions: Vec<Exclusion>,
    pub(crate) missing_evidence: Vec<String>,
}

/// `v1` preferred efforts, fixed for every task (`stable-v1` is frozen).
pub(crate) const V1_EFFORTS: [EffortLevel; 3] =
    [EffortLevel::High, EffortLevel::Medium, EffortLevel::High];

pub(crate) fn build_candidates(
    request: &RouteRequest,
    catalog: &Catalog,
    preferred_efforts: [EffortLevel; 3],
    selection: SkillSelection,
) -> CandidateBuild {
    let engine = ConstraintEngine::default();
    let capabilities = capability_knowledge(request);
    let host = constraint_host(request);
    let mut stages = Vec::with_capacity(3);
    let mut exclusions = Vec::new();
    let mut missing_evidence = Vec::new();

    for (stage, preferred) in [StageKind::Plan, StageKind::Execute, StageKind::Verify]
        .into_iter()
        .zip(preferred_efforts)
    {
        let mut stage_candidates = Vec::new();
        for (_, model) in catalog.models() {
            for effort in effort_preference(preferred) {
                let context = EligibilityContext {
                    task: request.task.clone(),
                    host: host.clone(),
                    model: model.clone(),
                    stage,
                    effort,
                    capabilities: capabilities.clone(),
                };
                let verdicts = engine.evaluate(&context);
                if is_eligible(&verdicts) {
                    stage_candidates.push(StageCandidate {
                        kind: stage,
                        model: model.clone(),
                        effort,
                        preferred_effort: preferred,
                        skills: compatible_skills(request, catalog, model, selection),
                    });
                    break;
                }
                preserve_non_eligible(
                    stage,
                    model,
                    verdicts,
                    &mut exclusions,
                    &mut missing_evidence,
                );
            }
        }
        stages.push(stage_candidates);
    }

    CandidateBuild {
        stages,
        exclusions,
        missing_evidence,
    }
}

fn effort_preference(preferred: EffortLevel) -> Vec<EffortLevel> {
    const LEVELS: [EffortLevel; 8] = [
        EffortLevel::None,
        EffortLevel::Minimal,
        EffortLevel::Low,
        EffortLevel::Medium,
        EffortLevel::High,
        EffortLevel::Xhigh,
        EffortLevel::Max,
        EffortLevel::Ultra,
    ];
    const REMAINING: [EffortLevel; 6] = [
        EffortLevel::Low,
        EffortLevel::Medium,
        EffortLevel::High,
        EffortLevel::Xhigh,
        EffortLevel::Max,
        EffortLevel::Ultra,
    ];

    let index = LEVELS
        .iter()
        .position(|level| *level == preferred)
        .expect("preset effort must be declared");
    let mut ordered = Vec::with_capacity(REMAINING.len());
    push_unique(&mut ordered, preferred);
    if let Some(higher) = LEVELS.get(index + 1) {
        push_unique(&mut ordered, *higher);
    }
    if index > 0 {
        push_unique(&mut ordered, LEVELS[index - 1]);
    }
    for effort in REMAINING {
        push_unique(&mut ordered, effort);
    }
    ordered
}

fn push_unique(efforts: &mut Vec<EffortLevel>, effort: EffortLevel) {
    if !efforts.contains(&effort) {
        efforts.push(effort);
    }
}

fn capability_knowledge(request: &RouteRequest) -> CapabilityKnowledge {
    request
        .task
        .required_capabilities
        .iter()
        .map(|capability| (*capability, request_support(request, *capability)))
        .collect::<BTreeMap<Capability, Support>>()
        .into()
}

fn constraint_host(request: &RouteRequest) -> HostSnapshot {
    let mut host = request.host.clone();
    for (capability, support) in &request.capability_knowledge {
        match support {
            CapabilitySupport::Supported => {
                host.capabilities.insert(*capability);
            }
            CapabilitySupport::Unsupported | CapabilitySupport::Unknown => {
                host.capabilities.remove(capability);
            }
        }
    }
    host
}

fn request_support(request: &RouteRequest, capability: Capability) -> Support {
    match request.capability_knowledge.get(&capability) {
        Some(CapabilitySupport::Supported) => Support::Supported,
        Some(CapabilitySupport::Unsupported) => Support::Unsupported,
        Some(CapabilitySupport::Unknown) => Support::Unknown,
        None if request.host.capabilities.contains(&capability) => Support::Supported,
        None => Support::Unsupported,
    }
}

fn compatible_skills(
    request: &RouteRequest,
    catalog: &Catalog,
    model: &ModelSpec,
    selection: SkillSelection,
) -> Vec<AttachedSkill> {
    // Per id: the best score and whether any row came from the operator.
    let mut compatible = BTreeMap::<SkillId, (i64, bool)>::new();
    for candidate in &request.skill_candidates {
        let Some(skill) = catalog.skill(&candidate.skill) else {
            continue;
        };
        if !matches!(
            skill.trust,
            SkillTrust::Builtin | SkillTrust::TrustedLocal | SkillTrust::VerifiedThirdParty
        ) || !skill.task_kinds.contains(&request.task.kind)
            || !skill.required_capabilities.iter().all(|capability| {
                request_support(request, *capability) == Support::Supported
                    && model.capabilities.contains(capability)
            })
        {
            continue;
        }
        let operator = candidate.source == OPERATOR_DECLARED_SOURCE;
        compatible
            .entry(candidate.skill.clone())
            .and_modify(|(score, declared)| {
                *score = (*score).max(candidate.score_micros);
                *declared |= operator;
            })
            .or_insert((candidate.score_micros, operator));
    }
    match selection {
        SkillSelection::All => compatible
            .into_iter()
            .map(|(id, (score_micros, _))| AttachedSkill { id, score_micros })
            .collect(),
        SkillSelection::Capped => {
            let mut ranked = compatible
                .into_iter()
                .filter(|(_, (score, operator))| *operator || *score >= SKILL_FLOOR_MICROS)
                .collect::<Vec<_>>();
            // The map iterates by id, and the sort is stable: id breaks ties.
            ranked.sort_by_key(|(_, (score, operator))| (!*operator, std::cmp::Reverse(*score)));
            ranked
                .into_iter()
                .take(MAX_STAGE_SKILLS)
                .map(|(id, (score_micros, _))| AttachedSkill { id, score_micros })
                .collect()
        }
    }
}

fn preserve_non_eligible(
    stage: StageKind,
    model: &ModelSpec,
    verdicts: Vec<ConstraintVerdict>,
    exclusions: &mut Vec<Exclusion>,
    missing_evidence: &mut Vec<String>,
) {
    for verdict in verdicts {
        match verdict {
            ConstraintVerdict::Eligible { .. } => {}
            ConstraintVerdict::Excluded { code, evidence } => exclusions.push(Exclusion {
                stage,
                model: model.id.clone(),
                code: code.into(),
                evidence,
            }),
            ConstraintVerdict::Unknown { code, evidence } => {
                missing_evidence.push(evidence.clone());
                exclusions.push(Exclusion {
                    stage,
                    model: model.id.clone(),
                    code: code.into(),
                    evidence,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::effort_preference;
    use router_types::EffortLevel;

    #[test]
    fn fixed_effort_order_is_nearest_first_and_deduplicated() {
        assert_eq!(
            effort_preference(EffortLevel::High),
            vec![
                EffortLevel::High,
                EffortLevel::Xhigh,
                EffortLevel::Medium,
                EffortLevel::Low,
                EffortLevel::Max,
                EffortLevel::Ultra,
            ]
        );
        assert_eq!(
            effort_preference(EffortLevel::Medium),
            vec![
                EffortLevel::Medium,
                EffortLevel::High,
                EffortLevel::Low,
                EffortLevel::Xhigh,
                EffortLevel::Max,
                EffortLevel::Ultra,
            ]
        );
    }
}
