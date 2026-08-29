use std::{cmp::Ordering, collections::BTreeMap};

use router_types::{
    Exclusion, ExecutionPlan, PlanStage, RiskLevel, SkillId, StageKind, TaskKind,
    VerificationIndependence,
};

use crate::preset::{PRESET_NAME, StageCandidate};
use crate::route::RouteError;

#[derive(Debug)]
pub(crate) struct RankedPlan {
    pub(crate) plan: ExecutionPlan,
    total_cost_micros: i128,
    model_key: String,
}

pub(crate) fn rank_plans(
    stages: &[Vec<StageCandidate>],
    task_kind: TaskKind,
    risk: RiskLevel,
    exclusions: &mut Vec<Exclusion>,
) -> Result<Vec<RankedPlan>, RouteError> {
    if stages.len() != 3 || stages.iter().any(Vec::is_empty) {
        return Ok(Vec::new());
    }

    let security_verify_quality = if task_kind == TaskKind::Security {
        stages[2]
            .iter()
            .map(|candidate| candidate.model.quality_micros)
            .max()
    } else {
        None
    };
    if let Some(required_quality) = security_verify_quality {
        for verify in &stages[2] {
            if verify.model.quality_micros < required_quality {
                exclusions.push(Exclusion {
                    stage: StageKind::Verify,
                    model: verify.model.id.clone(),
                    code: "verification.security.not_max_quality".into(),
                    evidence: format!(
                        "security verify model '{}' quality {} is below eligible maximum {}",
                        verify.model.id, verify.model.quality_micros, required_quality
                    ),
                });
            }
        }
    }

    let mut plans = Vec::new();
    for plan in &stages[0] {
        for execute in &stages[1] {
            for verify in &stages[2] {
                if security_verify_quality
                    .is_some_and(|quality| verify.model.quality_micros < quality)
                {
                    continue;
                }
                if matches!(risk, RiskLevel::High | RiskLevel::Critical)
                    && verify.model.quality_micros < execute.model.quality_micros
                {
                    exclusions.push(Exclusion {
                        stage: StageKind::Verify,
                        model: verify.model.id.clone(),
                        code: "verification.quality.below_execute".into(),
                        evidence: format!(
                            "verify model '{}' quality {} is below execute model '{}' quality {}",
                            verify.model.id,
                            verify.model.quality_micros,
                            execute.model.id,
                            execute.model.quality_micros
                        ),
                    });
                    continue;
                }
                plans.push(to_ranked_plan([plan, execute, verify], risk)?);
            }
        }
    }
    plans.sort_by(compare_ranked);
    Ok(plans)
}

fn to_ranked_plan(stages: [&StageCandidate; 3], risk: RiskLevel) -> Result<RankedPlan, RouteError> {
    let quality_micros = stages
        .iter()
        .map(|stage| i128::from(stage.model.quality_micros))
        .min()
        .expect("a plan has three stages");
    let total_cost_micros = checked_sum(
        stages
            .iter()
            .map(|stage| i128::from(stage.model.cost_micros)),
        "total_cost_micros",
    )?;
    let total_latency_micros = stages
        .iter()
        .map(|stage| i128::from(stage.model.latency_micros));
    let total_latency_micros = checked_sum(total_latency_micros, "total_latency_micros")?;
    let mut skills = BTreeMap::<SkillId, i64>::new();
    for stage in stages {
        for skill in &stage.skills {
            skills
                .entry(skill.id.clone())
                .and_modify(|score| *score = (*score).max(skill.score_micros))
                .or_insert(skill.score_micros);
        }
    }
    let retrieved_skill_score_micros = checked_sum(
        skills.values().map(|score| i128::from(*score)),
        "retrieved_skill_score_micros",
    )?;
    let utility_micros = quality_micros
        .checked_sub(total_cost_micros / 4)
        .and_then(|value| value.checked_sub(total_latency_micros / 10))
        .and_then(|value| value.checked_add(retrieved_skill_score_micros / 20))
        .ok_or(RouteError::ArithmeticOutOfRange {
            component: "utility_micros",
        })?;
    let utility_micros =
        i64::try_from(utility_micros).map_err(|_| RouteError::ArithmeticOutOfRange {
            component: "utility_micros",
        })?;
    let model_key = stages
        .iter()
        .map(|stage| stage.model.id.as_str())
        .collect::<Vec<_>>()
        .join("\u{0}");
    let plan_stages = stages
        .into_iter()
        .map(|stage| PlanStage {
            kind: stage.kind,
            model: stage.model.id.clone(),
            effort: stage.effort,
            skills: stage.skills.iter().map(|skill| skill.id.clone()).collect(),
            verification_independence: if stage.kind == StageKind::Verify
                && matches!(risk, RiskLevel::High | RiskLevel::Critical)
            {
                Some(VerificationIndependence::FreshInstance)
            } else {
                None
            },
        })
        .collect();

    Ok(RankedPlan {
        plan: ExecutionPlan {
            preset: PRESET_NAME.into(),
            stages: plan_stages,
            utility_micros,
        },
        total_cost_micros,
        model_key,
    })
}

fn checked_sum(
    values: impl IntoIterator<Item = i128>,
    component: &'static str,
) -> Result<i128, RouteError> {
    values
        .into_iter()
        .try_fold(0_i128, i128::checked_add)
        .ok_or(RouteError::ArithmeticOutOfRange { component })
}

fn compare_ranked(left: &RankedPlan, right: &RankedPlan) -> Ordering {
    right
        .plan
        .utility_micros
        .cmp(&left.plan.utility_micros)
        .then_with(|| left.total_cost_micros.cmp(&right.total_cost_micros))
        .then_with(|| left.model_key.cmp(&right.model_key))
}

#[cfg(test)]
mod tests {
    use super::{RankedPlan, compare_ranked};
    use router_types::ExecutionPlan;

    #[test]
    fn equal_utility_prefers_lower_cost_before_lexicographic_model_key() {
        let lower_cost_later_id = ranked(100, "z-model");
        let higher_cost_earlier_id = ranked(200, "a-model");

        assert!(compare_ranked(&lower_cost_later_id, &higher_cost_earlier_id).is_lt());
    }

    fn ranked(total_cost_micros: i128, model_key: &str) -> RankedPlan {
        RankedPlan {
            plan: ExecutionPlan {
                preset: "plan-execute-verify".into(),
                stages: vec![],
                utility_micros: 500_000,
            },
            total_cost_micros,
            model_key: model_key.into(),
        }
    }
}
