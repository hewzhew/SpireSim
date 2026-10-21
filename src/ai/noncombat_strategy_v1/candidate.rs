use super::types::{
    RunStrategySnapshotV1, RunStrategySnapshotV2, StrategyCandidateFactsV1,
    StrategyCandidatePlanDeltaV1, StrategyPlanEffectV1, StrategyPlanIdV1, StrategyPlanSupportV1,
};

pub fn candidate_plan_delta_v2(
    facts: StrategyCandidateFactsV1,
    snapshot: &RunStrategySnapshotV2,
) -> StrategyCandidatePlanDeltaV1 {
    candidate_plan_delta_v1(facts, &snapshot.v1)
}

pub fn candidate_plan_delta_v1(
    facts: StrategyCandidateFactsV1,
    snapshot: &RunStrategySnapshotV1,
) -> StrategyCandidatePlanDeltaV1 {
    let mut effects = Vec::new();
    let mut notes = Vec::new();

    for effect in &facts.plan_effects {
        push_effect(&mut effects, *effect);
    }
    if facts.damage_total > 0 {
        push_effect(&mut effects, StrategyPlanEffectV1::FrontloadDamage);
    }
    if facts.weak > 0 {
        push_effect(&mut effects, StrategyPlanEffectV1::WeakCoverage);
        push_effect(&mut effects, StrategyPlanEffectV1::DamageMitigation);
    }
    if facts.strength_gain > 0 {
        push_effect(&mut effects, StrategyPlanEffectV1::StrengthGenerator);
    }

    let mut support = if effects
        .iter()
        .any(|effect| plan_id_for_effect(*effect).is_some())
    {
        StrategyPlanSupportV1::Blocked
    } else {
        StrategyPlanSupportV1::Weak
    };

    for effect in &effects {
        if let Some(plan_id) = plan_id_for_effect(*effect) {
            apply_plan_context(snapshot, plan_id, &mut support, &mut notes);
        }
    }

    StrategyCandidatePlanDeltaV1 {
        effects,
        support,
        notes,
    }
}

fn push_effect(effects: &mut Vec<StrategyPlanEffectV1>, effect: StrategyPlanEffectV1) {
    if !effects.contains(&effect) {
        effects.push(effect);
    }
}

fn plan_id_for_effect(effect: StrategyPlanEffectV1) -> Option<StrategyPlanIdV1> {
    match effect {
        StrategyPlanEffectV1::UpgradeSink | StrategyPlanEffectV1::UpgradeBudgetConsumer => {
            Some(StrategyPlanIdV1::UpgradeSink)
        }
        StrategyPlanEffectV1::StrengthGenerator | StrategyPlanEffectV1::StrengthPayoff => {
            Some(StrategyPlanIdV1::StrengthScaling)
        }
        StrategyPlanEffectV1::WeakCoverage => Some(StrategyPlanIdV1::WeakControl),
        StrategyPlanEffectV1::BlockRetention
        | StrategyPlanEffectV1::BlockPayoff
        | StrategyPlanEffectV1::BlockMultiplier => Some(StrategyPlanIdV1::BlockEngine),
        StrategyPlanEffectV1::ExhaustGenerator | StrategyPlanEffectV1::ExhaustPayoff => {
            Some(StrategyPlanIdV1::ExhaustEngine)
        }
        StrategyPlanEffectV1::StatusGenerator | StrategyPlanEffectV1::StatusPayoff => {
            Some(StrategyPlanIdV1::StatusPackage)
        }
        StrategyPlanEffectV1::FrontloadDamage | StrategyPlanEffectV1::DamageMitigation => None,
    }
}

fn apply_plan_context(
    snapshot: &RunStrategySnapshotV1,
    plan_id: StrategyPlanIdV1,
    support: &mut StrategyPlanSupportV1,
    notes: &mut Vec<String>,
) {
    let plan = snapshot.plan(plan_id);
    let plan_support = plan
        .map(|plan| plan.support)
        .unwrap_or(StrategyPlanSupportV1::Blocked);
    if support_priority(plan_support) > support_priority(*support) {
        *support = plan_support;
    }
    if let Some(plan) = plan {
        notes.extend(plan.evidence.clone());
        notes.extend(plan.blockers.clone());
        notes.extend(plan.opportunity_costs.clone());
    }
}

fn support_priority(support: StrategyPlanSupportV1) -> u8 {
    match support {
        StrategyPlanSupportV1::Blocked => 0,
        StrategyPlanSupportV1::Weak => 1,
        StrategyPlanSupportV1::Plausible => 2,
        StrategyPlanSupportV1::Strong => 3,
    }
}
