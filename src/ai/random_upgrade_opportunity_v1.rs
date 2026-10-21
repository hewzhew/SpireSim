use crate::ai::upgrade_planner_v1::{
    plan_upgrades_v1, UpgradeCandidateV1, UpgradeDebtSeverityV1, UpgradeRoleV1, UpgradeVerdictV1,
};
use crate::state::run::RunState;

#[derive(Clone, Debug, PartialEq)]
pub struct RandomUpgradeOpportunityPlanV1 {
    pub source: RandomUpgradeSourceV1,
    pub eligible_count: usize,
    pub hit_distribution: RandomUpgradeHitDistributionV1,
    pub hp_cost: i32,
    pub hp_after_cost: i32,
    pub verdict: RandomUpgradeVerdictV1,
    pub blockers: Vec<RandomUpgradeBlockerV1>,
    pub score_hint: i32,
    pub evidence: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RandomUpgradeSourceV1 {
    ShiningLight { hp_cost: i32, upgrade_count: usize },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RandomUpgradeHitDistributionV1 {
    pub critical_targets: usize,
    pub important_or_better_targets: usize,
    pub useful_targets: usize,
    pub filler_or_low_targets: usize,
    pub p_hit_at_least_one_critical: ProbabilityBucketV1,
    pub p_hit_at_least_one_important_or_better: ProbabilityBucketV1,
    pub p_double_filler_or_low: ProbabilityBucketV1,
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProbabilityBucketV1 {
    #[default]
    NearZero,
    Low,
    Medium,
    High,
    Certain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RandomUpgradeVerdictV1 {
    EnterClean,
    EnterRisky,
    SplitBranch,
    Leave,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RandomUpgradeBlockerV1 {
    NoUpgradeableCards,
    LowImpactUpgradeDensity,
    TooManyFillerUpgradeableCards,
    HpCostCreatesImmediateLethalRisk,
    HpCostLikelyForcesRest,
}

pub fn evaluate_random_upgrade_opportunity_v1(
    run_state: &RunState,
    source: RandomUpgradeSourceV1,
) -> RandomUpgradeOpportunityPlanV1 {
    let hp_cost = match source {
        RandomUpgradeSourceV1::ShiningLight { hp_cost, .. } => hp_cost,
    };
    let upgrade_count = match source {
        RandomUpgradeSourceV1::ShiningLight { upgrade_count, .. } => upgrade_count,
    };
    let upgrade_plan = plan_upgrades_v1(run_state);
    let eligible_count = upgrade_plan.candidates.len();
    let hp_after_cost = run_state.current_hp.saturating_sub(hp_cost);
    let hit_distribution = hit_distribution(&upgrade_plan.candidates, upgrade_count);
    let mut blockers = Vec::new();
    let mut evidence = vec![format!(
        "random upgrade opportunity: source={source:?} eligible_count={eligible_count} upgrade_count={upgrade_count}"
    )];
    let mut risks = Vec::new();

    if eligible_count == 0 {
        blockers.push(RandomUpgradeBlockerV1::NoUpgradeableCards);
    }
    if hit_distribution.p_hit_at_least_one_important_or_better < ProbabilityBucketV1::Medium {
        blockers.push(RandomUpgradeBlockerV1::LowImpactUpgradeDensity);
    }
    if hit_distribution.p_double_filler_or_low >= ProbabilityBucketV1::Medium {
        blockers.push(RandomUpgradeBlockerV1::TooManyFillerUpgradeableCards);
    }
    if hp_after_cost <= 0 {
        blockers.push(RandomUpgradeBlockerV1::HpCostCreatesImmediateLethalRisk);
    }
    let hp_after_ratio = if run_state.max_hp > 0 {
        hp_after_cost.max(0) as f32 / run_state.max_hp as f32
    } else {
        0.0
    };
    if hp_after_cost > 0 && (hp_after_cost < 30 || hp_after_ratio < 0.35) {
        blockers.push(RandomUpgradeBlockerV1::HpCostLikelyForcesRest);
    }

    evidence.push(format!(
        "random upgrade hit distribution: critical={} important_or_better={} useful={} filler_or_low={} p_important={:?} p_double_filler={:?}",
        hit_distribution.critical_targets,
        hit_distribution.important_or_better_targets,
        hit_distribution.useful_targets,
        hit_distribution.filler_or_low_targets,
        hit_distribution.p_hit_at_least_one_important_or_better,
        hit_distribution.p_double_filler_or_low
    ));
    if let Some(best_debt) = upgrade_plan.best_smith_debt_paid {
        evidence.push(format!(
            "random upgrade can hit current upgrade debt: {}",
            best_debt.label()
        ));
    }
    if hp_cost > 0 {
        risks.push(format!(
            "random upgrade opportunity costs {hp_cost} HP, leaving {hp_after_cost}/{}",
            run_state.max_hp
        ));
    }

    let verdict = if blockers.contains(&RandomUpgradeBlockerV1::NoUpgradeableCards)
        || blockers.contains(&RandomUpgradeBlockerV1::HpCostCreatesImmediateLethalRisk)
    {
        RandomUpgradeVerdictV1::Leave
    } else if blockers.contains(&RandomUpgradeBlockerV1::HpCostLikelyForcesRest) {
        RandomUpgradeVerdictV1::SplitBranch
    } else if hit_distribution.p_hit_at_least_one_important_or_better >= ProbabilityBucketV1::High
        && hit_distribution.p_double_filler_or_low < ProbabilityBucketV1::Medium
    {
        RandomUpgradeVerdictV1::EnterClean
    } else if hit_distribution.p_hit_at_least_one_important_or_better >= ProbabilityBucketV1::Medium
    {
        RandomUpgradeVerdictV1::EnterRisky
    } else if blockers.is_empty() {
        RandomUpgradeVerdictV1::SplitBranch
    } else {
        RandomUpgradeVerdictV1::Leave
    };

    let score_hint = random_upgrade_score_hint(verdict, &hit_distribution, hp_cost);

    RandomUpgradeOpportunityPlanV1 {
        source,
        eligible_count,
        hit_distribution,
        hp_cost,
        hp_after_cost,
        verdict,
        blockers,
        score_hint,
        evidence,
        risks,
    }
}

fn hit_distribution(
    candidates: &[UpgradeCandidateV1],
    upgrade_count: usize,
) -> RandomUpgradeHitDistributionV1 {
    let critical_targets = candidates
        .iter()
        .filter(|candidate| is_critical_upgrade_target(candidate))
        .count();
    let important_or_better_targets = candidates
        .iter()
        .filter(|candidate| is_important_or_better_upgrade_target(candidate))
        .count();
    let useful_targets = candidates
        .iter()
        .filter(|candidate| candidate.verdict == UpgradeVerdictV1::Useful)
        .count();
    let filler_or_low_targets = candidates
        .len()
        .saturating_sub(important_or_better_targets)
        .saturating_sub(useful_targets);

    RandomUpgradeHitDistributionV1 {
        critical_targets,
        important_or_better_targets,
        useful_targets,
        filler_or_low_targets,
        p_hit_at_least_one_critical: hit_at_least_one_bucket(
            candidates.len(),
            critical_targets,
            upgrade_count,
        ),
        p_hit_at_least_one_important_or_better: hit_at_least_one_bucket(
            candidates.len(),
            important_or_better_targets,
            upgrade_count,
        ),
        p_double_filler_or_low: all_from_bucket(
            candidates.len(),
            filler_or_low_targets,
            upgrade_count,
        ),
    }
}

fn is_critical_upgrade_target(candidate: &UpgradeCandidateV1) -> bool {
    candidate.urgency == UpgradeDebtSeverityV1::CriticalBeforeBoss
        || (candidate.verdict == UpgradeVerdictV1::CoreDebtPayment
            && candidate.urgency >= UpgradeDebtSeverityV1::ImportantBeforeBoss)
}

fn is_important_or_better_upgrade_target(candidate: &UpgradeCandidateV1) -> bool {
    candidate.verdict >= UpgradeVerdictV1::Important
        || candidate.urgency >= UpgradeDebtSeverityV1::UsefulSoon
        || candidate.roles.contains(&UpgradeRoleV1::CoreMechanic)
        || !candidate.pays_debts.is_empty()
}

fn hit_at_least_one_bucket(total: usize, hits: usize, draws: usize) -> ProbabilityBucketV1 {
    if total == 0 || hits == 0 || draws == 0 {
        return ProbabilityBucketV1::NearZero;
    }
    if hits >= total {
        return ProbabilityBucketV1::Certain;
    }
    probability_bucket(1.0 - miss_probability(total, hits, draws))
}

fn all_from_bucket(total: usize, matching: usize, draws: usize) -> ProbabilityBucketV1 {
    if total == 0 || matching == 0 || draws == 0 || matching < draws {
        return ProbabilityBucketV1::NearZero;
    }
    let probability = (0..draws).fold(1.0, |acc, offset| {
        acc * (matching - offset) as f32 / (total - offset) as f32
    });
    probability_bucket(probability)
}

fn miss_probability(total: usize, hits: usize, draws: usize) -> f32 {
    let misses = total.saturating_sub(hits);
    if draws > total {
        return 0.0;
    }
    (0..draws).fold(1.0, |acc, offset| {
        if misses <= offset {
            0.0
        } else {
            acc * (misses - offset) as f32 / (total - offset) as f32
        }
    })
}

fn probability_bucket(value: f32) -> ProbabilityBucketV1 {
    if value >= 0.999 {
        ProbabilityBucketV1::Certain
    } else if value >= 0.65 {
        ProbabilityBucketV1::High
    } else if value >= 0.35 {
        ProbabilityBucketV1::Medium
    } else if value > 0.0 {
        ProbabilityBucketV1::Low
    } else {
        ProbabilityBucketV1::NearZero
    }
}

fn random_upgrade_score_hint(
    verdict: RandomUpgradeVerdictV1,
    hit_distribution: &RandomUpgradeHitDistributionV1,
    hp_cost: i32,
) -> i32 {
    let verdict_score = match verdict {
        RandomUpgradeVerdictV1::EnterClean => 620,
        RandomUpgradeVerdictV1::EnterRisky => 460,
        RandomUpgradeVerdictV1::SplitBranch => 180,
        RandomUpgradeVerdictV1::Leave => -260,
    };
    let hit_bonus = match hit_distribution.p_hit_at_least_one_important_or_better {
        ProbabilityBucketV1::Certain => 180,
        ProbabilityBucketV1::High => 130,
        ProbabilityBucketV1::Medium => 80,
        ProbabilityBucketV1::Low => 25,
        ProbabilityBucketV1::NearZero => 0,
    };
    verdict_score + hit_bonus - hp_cost.saturating_mul(4)
}
