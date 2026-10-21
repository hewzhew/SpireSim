use super::pressure::route_pressure_v1;
use super::types::{
    DeckPlanHypothesisV1, StrategyDeckFormationNeedV1, StrategyDeckFormationStageV1,
    StrategyDeckFormationV1, StrategyPlanIdV1, StrategyPlanPressureV1, StrategyPlanSupportV1,
    StrategyRouteFutureV1, StrategyRoutePackageIdV1, StrategyRoutePackageV1,
};

pub fn assess_route_packages_v1(
    route: Option<&StrategyRouteFutureV1>,
    formation: &StrategyDeckFormationV1,
    plans: &[DeckPlanHypothesisV1],
) -> Vec<StrategyRoutePackageV1> {
    vec![
        combat_patch_window(route, formation),
        upgrade_commitment(route, formation, plans),
        core_plan_protection(route, formation),
        recovery_pressure(route),
    ]
}

fn combat_patch_window(
    route: Option<&StrategyRouteFutureV1>,
    formation: &StrategyDeckFormationV1,
) -> StrategyRoutePackageV1 {
    let pressure = route_pressure_v1(route);
    let needs_patch = formation
        .needs
        .contains(&StrategyDeckFormationNeedV1::Frontload)
        || formation
            .needs
            .contains(&StrategyDeckFormationNeedV1::Block);
    let support = if route.is_none() {
        StrategyPlanSupportV1::Blocked
    } else if needs_patch && pressure == StrategyPlanPressureV1::High {
        StrategyPlanSupportV1::Strong
    } else if needs_patch && pressure == StrategyPlanPressureV1::Medium {
        StrategyPlanSupportV1::Plausible
    } else if needs_patch {
        StrategyPlanSupportV1::Weak
    } else {
        StrategyPlanSupportV1::Blocked
    };

    StrategyRoutePackageV1 {
        id: StrategyRoutePackageIdV1::CombatPatchWindow,
        support,
        evidence: vec![
            format!("formation stage is {:?}", formation.stage),
            format!("formation needs are {:?}", formation.needs),
            format!("route pressure is {pressure:?}"),
        ],
        risks: if formation.strengths.is_empty() {
            Vec::new()
        } else {
            vec![format!(
                "patch picks may dilute committed plan(s): {:?}",
                formation.strengths
            )]
        },
    }
}

fn upgrade_commitment(
    route: Option<&StrategyRouteFutureV1>,
    formation: &StrategyDeckFormationV1,
    plans: &[DeckPlanHypothesisV1],
) -> StrategyRoutePackageV1 {
    let plan_support = plans
        .iter()
        .find(|plan| plan.id == StrategyPlanIdV1::UpgradeSink)
        .map(|plan| plan.support)
        .unwrap_or(StrategyPlanSupportV1::Blocked);
    let support = if route.is_none() {
        StrategyPlanSupportV1::Blocked
    } else {
        plan_support
    };

    let mut risks = Vec::new();
    if formation
        .needs
        .contains(&StrategyDeckFormationNeedV1::Frontload)
    {
        risks.push("upgrade commitment still competes with immediate frontload need".to_string());
    }
    if formation
        .needs
        .contains(&StrategyDeckFormationNeedV1::Block)
    {
        risks.push("upgrade commitment still competes with immediate block need".to_string());
    }

    StrategyRoutePackageV1 {
        id: StrategyRoutePackageIdV1::UpgradeCommitment,
        support,
        evidence: vec![
            format!("UpgradeSink plan support is {plan_support:?}"),
            route
                .map(|route| {
                    format!(
                        "visible fire budget is {}-{} with first fire {:?}",
                        route.min_fires, route.max_fires, route.first_fire_floor
                    )
                })
                .unwrap_or_else(|| "route future unavailable".to_string()),
        ],
        risks,
    }
}

fn core_plan_protection(
    route: Option<&StrategyRouteFutureV1>,
    formation: &StrategyDeckFormationV1,
) -> StrategyRoutePackageV1 {
    let support = if formation.strengths.is_empty() {
        StrategyPlanSupportV1::Blocked
    } else if matches!(
        formation.stage,
        StrategyDeckFormationStageV1::PlanCommitted | StrategyDeckFormationStageV1::Mature
    ) {
        StrategyPlanSupportV1::Strong
    } else {
        StrategyPlanSupportV1::Plausible
    };
    let pressure = route_pressure_v1(route);

    StrategyRoutePackageV1 {
        id: StrategyRoutePackageIdV1::CorePlanProtection,
        support,
        evidence: vec![
            format!("formation stage is {:?}", formation.stage),
            format!("committed strengths are {:?}", formation.strengths),
        ],
        risks: if pressure == StrategyPlanPressureV1::High {
            vec!["high route pressure can still require short-term survival patches".to_string()]
        } else {
            Vec::new()
        },
    }
}

fn recovery_pressure(route: Option<&StrategyRouteFutureV1>) -> StrategyRoutePackageV1 {
    let Some(route) = route else {
        return StrategyRoutePackageV1 {
            id: StrategyRoutePackageIdV1::RecoveryPressure,
            support: StrategyPlanSupportV1::Blocked,
            evidence: vec!["route future unavailable".to_string()],
            risks: Vec::new(),
        };
    };
    let pressure = route.need_heal.max(route.avoid_damage);
    let support = if pressure >= 0.65 {
        StrategyPlanSupportV1::Strong
    } else if pressure >= 0.30 {
        StrategyPlanSupportV1::Plausible
    } else if pressure > 0.0 {
        StrategyPlanSupportV1::Weak
    } else {
        StrategyPlanSupportV1::Blocked
    };

    StrategyRoutePackageV1 {
        id: StrategyRoutePackageIdV1::RecoveryPressure,
        support,
        evidence: vec![
            format!("need_heal={}", route.need_heal),
            format!("avoid_damage={}", route.avoid_damage),
        ],
        risks: Vec::new(),
    }
}
