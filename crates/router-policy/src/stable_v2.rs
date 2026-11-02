// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Compiled `stable-v2` advisory scoring (ADR 0054): per-stage quality targets
//! from risk and task kind, a shortfall-first rank key and the `scoring` report.
//! Margins and tightening are declared priors, not measurements. Changing one
//! requires a new policy version and a golden update.
use std::cmp::Reverse;

use router_types::{EffortLevel, RiskLevel, StageKind, TaskKind};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    preset::StageCandidate,
    route::RouteError,
    score::{HardRules, RankedPlan, assemble_plan, checked_sum, enforce_work_limit},
};

pub(crate) const POLICY_VERSION: &str = "stable-v2";

const DOMINATED: &str = "catalog.stage.dominated";
const COST_EQUAL: &str = "catalog.cost.equal";
const UNRATED: &str = "catalog.unrated";

/// Where the chosen effort came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffortProvenance {
    /// The host offers the level the policy table asks for.
    PolicyTable,
    /// The nearest level the host supports, because the target is unavailable.
    Fallback,
}

/// Per-stage entry of the additive `scoring` object of the advise report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StageScoring {
    pub stage: StageKind,
    pub target_micros: i64,
    /// Quality of the model chosen for this stage in the selected plan.
    pub quality_micros: i64,
    pub shortfall_micros: i64,
    /// Effort chosen for this stage in the selected plan.
    pub effort: EffortLevel,
    /// Effort the policy table asks for, a declared prior.
    pub effort_target: EffortLevel,
    /// `fallback` names a missed target: the host did not offer `effort_target`.
    pub effort_provenance: EffortProvenance,
    /// Catalog data that stops this stage from varying with risk or kind.
    pub reason_codes: Vec<String>,
}

/// The `scoring` object: how `stable-v2` read the catalog for this decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdvisoryScoring {
    pub policy_version: String,
    /// SHA-256 of the declared margins, tightening and effort tables.
    pub table_digest: String,
    /// Margins, tightening and efforts are declared priors, not measurements.
    pub effort_basis: &'static str,
    /// Effort does not enter cost or the rank key.
    pub effort_cost: &'static str,
    pub stages: Vec<StageScoring>,
    /// Sorted union of the stage reason codes. These describe catalog data,
    /// not model quality.
    pub reason_codes: Vec<String>,
}

/// Margin below the best eligible quality that a stage may accept (micros).
/// Non-increasing in risk for every stage.
pub(crate) fn margin_micros(risk: RiskLevel, stage: StageKind) -> i64 {
    use RiskLevel::{Critical, High, Low, Medium};
    use StageKind::{Execute, Plan, Verify};
    match (risk, stage) {
        (Low, Plan) => 40_000,
        (Low, Execute) => 340_000,
        (Low, Verify) => 190_000,
        (Medium, Plan) => 20_000,
        (Medium, Execute) => 190_000,
        (Medium, Verify) => 140_000,
        (High, Plan) => 0,
        (High, Execute) => 140_000,
        (High, Verify) => 40_000,
        (Critical, Plan) => 0,
        (Critical, Execute) => 40_000,
        (Critical, Verify) => 0,
    }
}

/// Kind adjustment added to the target before the cap at the eligible maximum.
pub(crate) fn tightening_micros(kind: TaskKind, stage: StageKind) -> i64 {
    use StageKind::{Execute, Plan, Verify};
    use TaskKind::{Architecture, Explore, Implement, Migration, Review, Security};
    match (kind, stage) {
        (Architecture, Plan) => 20_000,
        (Architecture, Execute) => 20_000,
        (Architecture, Verify) => 40_000,
        (Migration, Plan) => 20_000,
        (Migration, Execute) => 0,
        (Migration, Verify) => 40_000,
        (Security | Review, Plan | Execute) => 0,
        (Security | Review, Verify) => 40_000,
        (Explore, Plan) => 0,
        (Explore, Execute | Verify) => -30_000,
        (Implement, _) => 0,
    }
}

/// `tau_s = max(0, min(maxq_s, maxq_s - margin(risk, s) + tightening(kind, s)))`
pub(crate) fn target_micros(
    max_quality: i64,
    risk: RiskLevel,
    kind: TaskKind,
    stage: StageKind,
) -> i64 {
    let uncapped = i128::from(max_quality) - i128::from(margin_micros(risk, stage))
        + i128::from(tightening_micros(kind, stage));
    let capped = uncapped.min(i128::from(max_quality)).max(0);
    // `capped` lies in `0..=max_quality`, so it always fits.
    i64::try_from(capped).unwrap_or(max_quality)
}

const RISKS: [RiskLevel; 4] = [
    RiskLevel::Low,
    RiskLevel::Medium,
    RiskLevel::High,
    RiskLevel::Critical,
];
const TASK_KINDS: [TaskKind; 6] = [
    TaskKind::Architecture,
    TaskKind::Migration,
    TaskKind::Security,
    TaskKind::Review,
    TaskKind::Explore,
    TaskKind::Implement,
];

/// Efforts the table can name. Kind adjustments move along this ladder, never
/// below `low` and never above `max`.
const LADDER: [EffortLevel; 5] = [
    EffortLevel::Low,
    EffortLevel::Medium,
    EffortLevel::High,
    EffortLevel::Xhigh,
    EffortLevel::Max,
];

/// Preferred effort by risk (rows: low, medium, high, critical) and stage
/// (columns: plan, execute, verify). Declared priors, not measurements.
const EFFORT_TABLE: [[EffortLevel; 3]; 4] = [
    [EffortLevel::Medium, EffortLevel::Low, EffortLevel::Medium],
    [EffortLevel::High, EffortLevel::Medium, EffortLevel::Medium],
    [EffortLevel::High, EffortLevel::Medium, EffortLevel::High],
    [EffortLevel::Xhigh, EffortLevel::High, EffortLevel::Xhigh],
];

/// Ladder steps by kind (rows in `TASK_KINDS` order) and stage (plan, execute,
/// verify): architecture and migration plan +1, security and review verify +1,
/// explore verify -1.
const EFFORT_ADJUST: [[i8; 3]; 6] = [
    [1, 0, 0],
    [1, 0, 0],
    [0, 0, 1],
    [0, 0, 1],
    [0, 0, -1],
    [0, 0, 0],
];

fn risk_index(risk: RiskLevel) -> usize {
    match risk {
        RiskLevel::Low => 0,
        RiskLevel::Medium => 1,
        RiskLevel::High => 2,
        RiskLevel::Critical => 3,
    }
}

fn kind_index(kind: TaskKind) -> usize {
    match kind {
        TaskKind::Architecture => 0,
        TaskKind::Migration => 1,
        TaskKind::Security => 2,
        TaskKind::Review => 3,
        TaskKind::Explore => 4,
        TaskKind::Implement => 5,
    }
}

fn stage_index(stage: StageKind) -> usize {
    match stage {
        StageKind::Plan => 0,
        StageKind::Execute => 1,
        StageKind::Verify => 2,
    }
}

/// Preferred effort for a stage, before the host's supported levels apply.
pub(crate) fn preferred_effort(kind: TaskKind, risk: RiskLevel, stage: StageKind) -> EffortLevel {
    let base = EFFORT_TABLE[risk_index(risk)][stage_index(stage)];
    let step = EFFORT_ADJUST[kind_index(kind)][stage_index(stage)];
    let position = LADDER
        .iter()
        .position(|level| *level == base)
        .unwrap_or_default();
    let moved = position
        .saturating_add_signed(isize::from(step))
        .min(LADDER.len() - 1);
    LADDER[moved]
}

/// SHA-256 over a canonical text of every declared table, so a report names the
/// exact priors it used.
pub(crate) fn table_digest() -> String {
    let mut text = String::from("stable-v2-tables-1\n");
    for risk in RISKS {
        for stage in STAGE_KINDS {
            text.push_str(&format!(
                "margin {risk:?} {stage:?} {}\n",
                margin_micros(risk, stage)
            ));
        }
    }
    for kind in TASK_KINDS {
        for stage in STAGE_KINDS {
            text.push_str(&format!(
                "tightening {kind:?} {stage:?} {}\n",
                tightening_micros(kind, stage)
            ));
        }
    }
    for risk in RISKS {
        for stage in STAGE_KINDS {
            let level = EFFORT_TABLE[risk_index(risk)][stage_index(stage)];
            text.push_str(&format!("effort {risk:?} {stage:?} {level:?}\n"));
        }
    }
    for kind in TASK_KINDS {
        for stage in STAGE_KINDS {
            let step = EFFORT_ADJUST[kind_index(kind)][stage_index(stage)];
            text.push_str(&format!("effort_adjust {kind:?} {stage:?} {step}\n"));
        }
    }
    hex::encode(Sha256::digest(text.as_bytes()))
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct RankKey {
    shortfall: i128,
    cost: i128,
    quality_sum: Reverse<i128>,
    latency: i128,
    model_key: String,
}

struct Scored {
    key: RankKey,
    indices: [usize; 3],
}

const STAGE_KINDS: [StageKind; 3] = [StageKind::Plan, StageKind::Execute, StageKind::Verify];

pub(crate) fn rank_plans_v2(
    stages: &[Vec<StageCandidate>],
    task_kind: TaskKind,
    risk: RiskLevel,
    exclusions: &mut Vec<router_types::Exclusion>,
) -> Result<(Vec<RankedPlan>, Option<AdvisoryScoring>), RouteError> {
    if stages.len() != 3 || stages.iter().any(Vec::is_empty) {
        return Ok((Vec::new(), None));
    }
    enforce_work_limit(stages)?;
    let rules = HardRules::apply(stages, task_kind, risk, true, exclusions);

    // Models that sit in at least one feasible plan are the eligible ones: the
    // target is relative to their maximum, after the hard constraints.
    let mut participates: [Vec<bool>; 3] = [
        vec![false; stages[0].len()],
        vec![false; stages[1].len()],
        vec![false; stages[2].len()],
    ];
    for (e, execute) in stages[1].iter().enumerate() {
        for (v, verify) in stages[2].iter().enumerate() {
            if rules.allows(execute, verify) {
                participates[1][e] = true;
                participates[2][v] = true;
            }
        }
    }
    // Every plan-stage model combines with every feasible (execute, verify) pair.
    if participates[1].iter().any(|flag| *flag) {
        participates[0].fill(true);
    }
    if participates[0].iter().all(|flag| !flag) {
        return Ok((Vec::new(), None));
    }
    let eligible = |stage: usize| {
        stages[stage]
            .iter()
            .zip(&participates[stage])
            .filter(|(_, flag)| **flag)
            .map(|(candidate, _)| candidate)
            .collect::<Vec<_>>()
    };
    let eligible = [eligible(0), eligible(1), eligible(2)];
    let mut targets = [0_i64; 3];
    for (index, kind) in STAGE_KINDS.into_iter().enumerate() {
        let max_quality = eligible[index]
            .iter()
            .map(|candidate| candidate.model.quality_micros)
            .max()
            .unwrap_or(0);
        targets[index] = target_micros(max_quality, risk, task_kind, kind);
    }

    let mut top: Vec<Scored> = Vec::with_capacity(4);
    for (p, plan) in stages[0].iter().enumerate() {
        for (e, execute) in stages[1].iter().enumerate() {
            for (v, verify) in stages[2].iter().enumerate() {
                if !rules.allows(execute, verify) {
                    continue;
                }
                let members = [plan, execute, verify];
                let shortfall = checked_sum(
                    members.iter().zip(targets).map(|(member, target)| {
                        i128::from((target - member.model.quality_micros).max(0))
                    }),
                    "shortfall_micros",
                )?;
                let cost = checked_sum(
                    members.iter().map(|m| i128::from(m.model.cost_micros)),
                    "total_cost_micros",
                )?;
                let quality_sum = checked_sum(
                    members.iter().map(|m| i128::from(m.model.quality_micros)),
                    "quality_sum_micros",
                )?;
                let latency = checked_sum(
                    members.iter().map(|m| i128::from(m.model.latency_micros)),
                    "total_latency_micros",
                )?;
                let model_key = members
                    .iter()
                    .map(|m| m.model.id.as_str())
                    .collect::<Vec<_>>()
                    .join("\u{0}");
                top.push(Scored {
                    key: RankKey {
                        shortfall,
                        cost,
                        quality_sum: Reverse(quality_sum),
                        latency,
                        model_key,
                    },
                    indices: [p, e, v],
                });
                top.sort_by(|left, right| left.key.cmp(&right.key));
                top.truncate(3);
            }
        }
    }

    let mut ranked = Vec::with_capacity(top.len());
    for scored in &top {
        let [p, e, v] = scored.indices;
        let utility =
            i64::try_from(-scored.key.shortfall).map_err(|_| RouteError::ArithmeticOutOfRange {
                component: "utility_micros",
            })?;
        ranked.push(RankedPlan {
            plan: assemble_plan([&stages[0][p], &stages[1][e], &stages[2][v]], risk, utility),
            total_cost_micros: scored.key.cost,
            model_key: scored.key.model_key.clone(),
        });
    }
    let scoring = top
        .first()
        .map(|best| report(stages, &eligible, targets, best.indices));
    Ok((ranked, scoring))
}

fn report(
    stages: &[Vec<StageCandidate>],
    eligible: &[Vec<&StageCandidate>; 3],
    targets: [i64; 3],
    chosen: [usize; 3],
) -> AdvisoryScoring {
    let mut all_codes = Vec::new();
    let stage_rows = STAGE_KINDS
        .into_iter()
        .enumerate()
        .map(|(index, stage)| {
            let selected = &stages[index][chosen[index]];
            let quality = selected.model.quality_micros;
            let codes = reason_codes(&eligible[index], targets[index]);
            all_codes.extend(codes.iter().cloned());
            StageScoring {
                stage,
                target_micros: targets[index],
                quality_micros: quality,
                shortfall_micros: (targets[index] - quality).max(0),
                effort: selected.effort,
                effort_target: selected.preferred_effort,
                effort_provenance: if selected.effort == selected.preferred_effort {
                    EffortProvenance::PolicyTable
                } else {
                    EffortProvenance::Fallback
                },
                reason_codes: codes,
            }
        })
        .collect();
    all_codes.sort();
    all_codes.dedup();
    AdvisoryScoring {
        policy_version: POLICY_VERSION.into(),
        table_digest: table_digest(),
        effort_basis: "declared_policy_prior",
        effort_cost: "not_modelled",
        stages: stage_rows,
        reason_codes: all_codes,
    }
}

/// Catalog data that stops a stage from varying, over its eligible models.
fn reason_codes(eligible: &[&StageCandidate], target: i64) -> Vec<String> {
    let mut codes = Vec::new();
    let dominated = eligible.iter().any(|best| {
        eligible.iter().all(|other| {
            best.model.quality_micros >= other.model.quality_micros
                && best.model.cost_micros <= other.model.cost_micros
                && best.model.latency_micros <= other.model.latency_micros
        })
    });
    let meeting = eligible
        .iter()
        .filter(|candidate| candidate.model.quality_micros >= target)
        .collect::<Vec<_>>();
    // A single model meeting the target is not a tie: nothing is broken by quality.
    let cost_equal = meeting.len() > 1
        && meeting
            .iter()
            .all(|candidate| candidate.model.cost_micros == meeting[0].model.cost_micros);
    let unrated = eligible
        .iter()
        .all(|candidate| candidate.model.quality_micros == 0);
    if cost_equal {
        codes.push(COST_EQUAL.to_owned());
    }
    if dominated {
        codes.push(DOMINATED.to_owned());
    }
    if unrated {
        codes.push(UNRATED.to_owned());
    }
    codes
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use router_types::{
        EffortLevel, Exclusion, ModelId, ModelSpec, RiskLevel, SkillId, StageKind, TaskKind,
    };

    use super::{
        margin_micros, preferred_effort, rank_plans_v2, table_digest, target_micros,
        tightening_micros,
    };
    use crate::{
        preset::{AttachedSkill, StageCandidate},
        score::rank_plans,
    };

    const RISKS: [RiskLevel; 4] = [
        RiskLevel::Low,
        RiskLevel::Medium,
        RiskLevel::High,
        RiskLevel::Critical,
    ];
    const STAGES: [StageKind; 3] = [StageKind::Plan, StageKind::Execute, StageKind::Verify];
    const KINDS: [TaskKind; 6] = [
        TaskKind::Explore,
        TaskKind::Implement,
        TaskKind::Review,
        TaskKind::Architecture,
        TaskKind::Security,
        TaskKind::Migration,
    ];

    #[test]
    fn margins_never_increase_with_risk() {
        for stage in STAGES {
            for pair in RISKS.windows(2) {
                assert!(margin_micros(pair[0], stage) >= margin_micros(pair[1], stage));
            }
        }
    }

    #[test]
    fn targets_never_decrease_with_risk_and_never_exceed_the_maximum() {
        for kind in KINDS {
            for stage in STAGES {
                for max_quality in [0, 5_000, 300_000, 990_000, 1_000_000] {
                    let targets = RISKS.map(|risk| target_micros(max_quality, risk, kind, stage));
                    assert!(targets.windows(2).all(|pair| pair[0] <= pair[1]));
                    assert!(targets.iter().all(|t| (0..=max_quality).contains(t)));
                }
            }
        }
    }

    #[test]
    fn adr_examples_hold() {
        // Architecture plan at high risk: 990000 + 20000 would be 1010000, capped.
        assert_eq!(
            target_micros(
                990_000,
                RiskLevel::High,
                TaskKind::Architecture,
                StageKind::Plan
            ),
            990_000
        );
        // Low-risk implement execute: 990000 - 340000.
        assert_eq!(
            target_micros(
                990_000,
                RiskLevel::Low,
                TaskKind::Implement,
                StageKind::Execute
            ),
            650_000
        );
        assert_eq!(
            tightening_micros(TaskKind::Explore, StageKind::Verify),
            -30_000
        );
    }

    /// (id, quality, cost, latency)
    type Row = (&'static str, i64, i64, i64);

    fn stages(rows: &[Row], skills: usize) -> Vec<Vec<StageCandidate>> {
        let attached = (0..skills)
            .map(|index| AttachedSkill {
                id: SkillId::parse(format!("work-skill-{index:03}")).unwrap(),
                score_micros: 1_000_000,
            })
            .collect::<Vec<_>>();
        STAGES
            .into_iter()
            .map(|kind| {
                rows.iter()
                    .map(|(id, quality, cost, latency)| StageCandidate {
                        kind,
                        model: ModelSpec {
                            id: ModelId::parse(*id).unwrap(),
                            stages: BTreeSet::from(STAGES),
                            efforts: BTreeSet::from([EffortLevel::High]),
                            capabilities: BTreeSet::new(),
                            quality_micros: *quality,
                            cost_micros: *cost,
                            latency_micros: *latency,
                            source: "test-prior".into(),
                        },
                        effort: EffortLevel::High,
                        preferred_effort: EffortLevel::High,
                        skills: attached.clone(),
                    })
                    .collect()
            })
            .collect()
    }

    fn models(plans: &[crate::score::RankedPlan]) -> Vec<Vec<String>> {
        plans
            .iter()
            .map(|ranked| {
                ranked
                    .plan
                    .stages
                    .iter()
                    .map(|stage| stage.model.as_str().to_owned())
                    .collect()
            })
            .collect()
    }

    fn best(rows: &[Row], kind: TaskKind, risk: RiskLevel) -> Vec<String> {
        let (plans, _) = rank_plans_v2(&stages(rows, 0), kind, risk, &mut vec![]).unwrap();
        models(&plans).remove(0)
    }

    #[test]
    fn shortfall_outranks_cost() {
        // Low risk, implement: targets are plan 860000, execute 560000, verify 710000.
        let rows = [
            ("a-strong", 900_000, 900_000, 1),
            ("b-mid", 800_000, 100_000, 1),
            ("c-weak", 500_000, 50_000, 1),
        ];
        assert_eq!(
            best(&rows, TaskKind::Implement, RiskLevel::Low),
            ["a-strong", "b-mid", "b-mid"]
        );
    }

    #[test]
    fn equal_shortfall_and_cost_prefer_higher_quality_sum() {
        let rows = [
            ("x-high", 900_000, 100_000, 1),
            ("y-low", 800_000, 100_000, 1),
        ];
        assert_eq!(
            best(&rows, TaskKind::Implement, RiskLevel::Low),
            ["x-high", "x-high", "x-high"]
        );
    }

    #[test]
    fn then_lower_latency_then_model_key() {
        let rows = [
            ("a-slow", 900_000, 100_000, 9),
            ("b-fast", 900_000, 100_000, 1),
        ];
        assert_eq!(
            best(&rows, TaskKind::Implement, RiskLevel::Low),
            ["b-fast", "b-fast", "b-fast"]
        );
        let rows = [
            ("b-same", 900_000, 100_000, 1),
            ("a-same", 900_000, 100_000, 1),
        ];
        assert_eq!(
            best(&rows, TaskKind::Implement, RiskLevel::Low),
            ["a-same", "a-same", "a-same"]
        );
    }

    #[test]
    fn critical_keeps_only_the_strongest_verifier_and_records_the_code() {
        let rows = [
            ("a-strong", 900_000, 900_000, 1),
            ("b-mid", 800_000, 100_000, 1),
        ];
        let mut exclusions: Vec<Exclusion> = vec![];
        let (plans, _) = rank_plans_v2(
            &stages(&rows, 0),
            TaskKind::Implement,
            RiskLevel::Critical,
            &mut exclusions,
        )
        .unwrap();
        assert!(models(&plans).iter().all(|plan| plan[2] == "a-strong"));
        assert!(exclusions.iter().any(|e| {
            e.code == "verification.critical.not_max_quality" && e.model.as_str() == "b-mid"
        }));
        // v1 stays frozen: no such exclusion and the weaker verifier survives.
        let mut v1_exclusions: Vec<Exclusion> = vec![];
        let v1 = rank_plans(
            &stages(&rows, 0),
            TaskKind::Implement,
            RiskLevel::Critical,
            &mut v1_exclusions,
        )
        .unwrap();
        assert!(
            v1_exclusions
                .iter()
                .all(|e| e.code != "verification.critical.not_max_quality")
        );
        assert!(models(&v1).iter().any(|plan| plan[2] == "b-mid"));
    }

    #[test]
    fn streaming_top_three_matches_the_adr_rank_key_oracle() {
        let rows: Vec<Row> = (0..12)
            .map(|index| {
                let name: &'static str = Box::leak(format!("model-{index:02}").into_boxed_str());
                (
                    name,
                    1_000_000 - (index * 37_000) % 400_000,
                    100_000 + (index * 53_000) % 500_000,
                    1 + (index % 4),
                )
            })
            .collect();
        let candidates = stages(&rows, 0);
        let (plans, scoring) = rank_plans_v2(
            &candidates,
            TaskKind::Review,
            RiskLevel::Medium,
            &mut vec![],
        )
        .unwrap();
        let max_quality = rows.iter().map(|row| row.1).max().unwrap();
        let targets = STAGES
            .map(|stage| target_micros(max_quality, RiskLevel::Medium, TaskKind::Review, stage));
        let mut oracle = vec![];
        for p in &rows {
            for e in &rows {
                for v in &rows {
                    let members = [p, e, v];
                    let shortfall: i64 = members
                        .iter()
                        .zip(targets)
                        .map(|(m, t)| (t - m.1).max(0))
                        .sum();
                    oracle.push((
                        shortfall,
                        members.iter().map(|m| m.2).sum::<i64>(),
                        -members.iter().map(|m| m.1).sum::<i64>(),
                        members.iter().map(|m| m.3).sum::<i64>(),
                        members.iter().map(|m| m.0).collect::<Vec<_>>().join("\0"),
                    ));
                }
            }
        }
        oracle.sort();
        let expected = oracle
            .iter()
            .take(3)
            .map(|row| row.4.split('\0').map(str::to_owned).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(models(&plans), expected);
        assert_eq!(plans[0].plan.utility_micros, -oracle[0].0);
        assert_eq!(scoring.unwrap().stages.len(), 3);
    }

    #[test]
    fn work_limit_applies_to_v1_and_v2() {
        let rows: Vec<Row> = (0..64)
            .map(|index| {
                let name: &'static str = Box::leak(format!("provider-{index:02}").into_boxed_str());
                (name, 1_000_000 - index, i64::MAX, 1)
            })
            .collect();
        let candidates = stages(&rows, 256);
        let v1 = rank_plans(
            &candidates,
            TaskKind::Implement,
            RiskLevel::Medium,
            &mut vec![],
        )
        .unwrap_err();
        let v2 = rank_plans_v2(
            &candidates,
            TaskKind::Implement,
            RiskLevel::Medium,
            &mut vec![],
        )
        .unwrap_err();
        for error in [v1, v2] {
            assert_eq!(error.code(), "route.resource.work_limit_exceeded");
            assert_eq!(error.work_limit(), Some((Some(201_588_736), 1_000_000)));
        }
    }

    #[test]
    fn work_limit_boundary_is_shared_with_the_skill_payload() {
        // 1 + 3 x 2 skills per combination, as in ADR 0054, stays within budget
        // for a catalog of 20 models: 8000 x 7.
        let rows: Vec<Row> = (0..20)
            .map(|index| {
                let name: &'static str = Box::leak(format!("provider-{index:02}").into_boxed_str());
                (name, 1_000_000 - index, 1, 1)
            })
            .collect();
        let (plans, _) = rank_plans_v2(
            &stages(&rows, 2),
            TaskKind::Implement,
            RiskLevel::Medium,
            &mut vec![],
        )
        .unwrap();
        assert_eq!(plans.len(), 3);
    }

    const LADDER: [EffortLevel; 5] = [
        EffortLevel::Low,
        EffortLevel::Medium,
        EffortLevel::High,
        EffortLevel::Xhigh,
        EffortLevel::Max,
    ];

    #[test]
    fn v2_policy_table_is_total() {
        for kind in KINDS {
            for risk in RISKS {
                for stage in STAGES {
                    let effort = preferred_effort(kind, risk, stage);
                    assert!(LADDER.contains(&effort), "{kind:?} {risk:?} {stage:?}");
                }
            }
        }
    }

    #[test]
    fn stage_policy_efforts_are_monotonic_in_risk() {
        for kind in KINDS {
            for stage in STAGES {
                let efforts = RISKS.map(|risk| preferred_effort(kind, risk, stage));
                assert!(
                    efforts.windows(2).all(|pair| pair[0] <= pair[1]),
                    "{kind:?} {stage:?}: {efforts:?}"
                );
            }
        }
    }

    #[test]
    fn adr_effort_table_and_kind_adjustments_hold() {
        use EffortLevel::{High, Low, Max, Medium, Xhigh};
        let at = |kind, risk, stage| preferred_effort(kind, risk, stage);
        let (p, e, v) = (StageKind::Plan, StageKind::Execute, StageKind::Verify);
        // Base rows, with a kind (implement) that adjusts nothing.
        let rows = [
            (RiskLevel::Low, [Medium, Low, Medium]),
            (RiskLevel::Medium, [High, Medium, Medium]),
            (RiskLevel::High, [High, Medium, High]),
            (RiskLevel::Critical, [Xhigh, High, Xhigh]),
        ];
        for (risk, [plan, execute, verify]) in rows {
            assert_eq!(at(TaskKind::Implement, risk, p), plan);
            assert_eq!(at(TaskKind::Implement, risk, e), execute);
            assert_eq!(at(TaskKind::Implement, risk, v), verify);
        }
        // architecture and migration plan +1, security and review verify +1,
        // explore verify -1, floor at low and cap at max.
        assert_eq!(at(TaskKind::Architecture, RiskLevel::High, p), Xhigh);
        assert_eq!(at(TaskKind::Migration, RiskLevel::Critical, p), Max);
        assert_eq!(at(TaskKind::Security, RiskLevel::High, v), Xhigh);
        assert_eq!(at(TaskKind::Review, RiskLevel::Low, v), High);
        assert_eq!(at(TaskKind::Explore, RiskLevel::Low, v), Low);
        assert_eq!(at(TaskKind::Explore, RiskLevel::Medium, v), Low);
        assert_eq!(at(TaskKind::Explore, RiskLevel::Critical, e), High);
    }

    #[test]
    fn table_digest_is_a_stable_sha256_of_the_declared_tables() {
        let digest = table_digest();
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(digest, table_digest());
        assert_eq!(
            digest, "52c95c9a859e1b68f559cdfd1977cfcc5e8c3adab6166f265c909934a7fdf554",
            "a table change must update this pin"
        );
    }
}
