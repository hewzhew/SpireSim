use super::coverage::threat_coverage_from_deck_facts_v1;
use super::snapshot::build_run_strategy_snapshot_v1;
use super::threat::empty_threat_profile_v1;
use super::types::{
    DeckPlanHypothesisV1, RunStrategySnapshotV1, RunStrategySnapshotV2, StrategyDeckFactsV1,
    StrategyPackageDomainV2, StrategyPackageGapV2, StrategyPackageIdV2, StrategyPackageV2,
    StrategyPlanIdV1, StrategyPlanSupportV1, StrategyResourceFactsV2, StrategyRouteFutureV1,
    StrategyRoutePackageV1, StrategyThreatProfileV1,
};

pub fn build_run_strategy_snapshot_v2(
    deck: StrategyDeckFactsV1,
    route: Option<StrategyRouteFutureV1>,
    resources: Option<StrategyResourceFactsV2>,
) -> RunStrategySnapshotV2 {
    let v1 = build_run_strategy_snapshot_v1(deck, route);
    build_run_strategy_snapshot_v2_from_v1(v1, resources.unwrap_or_else(empty_resource_facts))
}

pub fn build_run_strategy_snapshot_v2_from_v1(
    v1: RunStrategySnapshotV1,
    resources: StrategyResourceFactsV2,
) -> RunStrategySnapshotV2 {
    build_run_strategy_snapshot_v2_from_v1_with_threat(v1, resources, empty_threat_profile_v1())
}

pub fn build_run_strategy_snapshot_v2_from_v1_with_threat(
    v1: RunStrategySnapshotV1,
    resources: StrategyResourceFactsV2,
    threats: StrategyThreatProfileV1,
) -> RunStrategySnapshotV2 {
    let threat_coverage = threat_coverage_from_deck_facts_v1(&v1.deck, &threats);
    let mut packages = Vec::new();
    packages.extend(
        v1.plans
            .iter()
            .map(|plan| archetype_package(plan, &v1.deck)),
    );
    packages.extend(v1.route_packages.iter().map(route_package));
    packages.extend(resource_packages(&resources));

    RunStrategySnapshotV2 {
        v1,
        resources,
        threats,
        threat_coverage,
        packages,
    }
}

fn archetype_package(plan: &DeckPlanHypothesisV1, deck: &StrategyDeckFactsV1) -> StrategyPackageV2 {
    StrategyPackageV2 {
        id: StrategyPackageIdV2::from_plan_v1(plan.id),
        domain: StrategyPackageDomainV2::Archetype,
        support: plan.support,
        missing_roles: archetype_missing_roles(plan, deck),
        evidence: plan.evidence.clone(),
        blockers: plan.blockers.clone(),
        risks: plan.opportunity_costs.clone(),
    }
}

fn route_package(package: &StrategyRoutePackageV1) -> StrategyPackageV2 {
    StrategyPackageV2 {
        id: StrategyPackageIdV2::from_route_package_v1(package.id),
        domain: StrategyPackageDomainV2::Route,
        support: package.support,
        missing_roles: Vec::new(),
        evidence: package.evidence.clone(),
        blockers: Vec::new(),
        risks: package.risks.clone(),
    }
}

fn resource_packages(resources: &StrategyResourceFactsV2) -> Vec<StrategyPackageV2> {
    vec![
        hp_safety_package(resources),
        gold_plan_package(resources),
        potion_capacity_package(resources),
        shop_remove_window_package(resources),
        relic_constraints_package(resources),
    ]
}

fn hp_safety_package(resources: &StrategyResourceFactsV2) -> StrategyPackageV2 {
    let hp_ratio = hp_ratio(resources);
    let support = if resources.max_hp <= 0 {
        StrategyPlanSupportV1::Blocked
    } else if hp_ratio <= 0.35 {
        StrategyPlanSupportV1::Strong
    } else if hp_ratio <= 0.55 {
        StrategyPlanSupportV1::Plausible
    } else if hp_ratio < 1.0 {
        StrategyPlanSupportV1::Weak
    } else {
        StrategyPlanSupportV1::Blocked
    };

    StrategyPackageV2 {
        id: StrategyPackageIdV2::HpSafety,
        domain: StrategyPackageDomainV2::Resource,
        support,
        missing_roles: Vec::new(),
        evidence: vec![
            format!(
                "hp={}/{} ratio={:.2}",
                resources.current_hp, resources.max_hp, hp_ratio
            ),
            format!(
                "anticipated_next_combat_start_heal={} effective_next_combat_hp={}",
                resources.anticipated_next_combat_start_heal, resources.effective_next_combat_hp
            ),
        ],
        blockers: if support == StrategyPlanSupportV1::Blocked {
            vec!["hp pressure is not currently visible".to_string()]
        } else {
            Vec::new()
        },
        risks: if support == StrategyPlanSupportV1::Strong {
            vec!["low hp should gate risky route/event/boss relic choices".to_string()]
        } else {
            Vec::new()
        },
    }
}

fn gold_plan_package(resources: &StrategyResourceFactsV2) -> StrategyPackageV2 {
    let support = if resources.gold >= 150 {
        StrategyPlanSupportV1::Strong
    } else if resources.gold >= resources.estimated_purge_cost {
        StrategyPlanSupportV1::Plausible
    } else if resources.gold > 0 {
        StrategyPlanSupportV1::Weak
    } else {
        StrategyPlanSupportV1::Blocked
    };

    StrategyPackageV2 {
        id: StrategyPackageIdV2::GoldPlan,
        domain: StrategyPackageDomainV2::Resource,
        support,
        missing_roles: Vec::new(),
        evidence: vec![
            format!("gold={}", resources.gold),
            format!("estimated purge cost={}", resources.estimated_purge_cost),
        ],
        blockers: if resources.gold < resources.estimated_purge_cost {
            vec!["gold does not cover estimated card removal cost".to_string()]
        } else {
            Vec::new()
        },
        risks: Vec::new(),
    }
}

fn potion_capacity_package(resources: &StrategyResourceFactsV2) -> StrategyPackageV2 {
    let support = if resources.empty_potion_slots > 0 {
        StrategyPlanSupportV1::Strong
    } else if resources.potion_slots > 0 {
        StrategyPlanSupportV1::Blocked
    } else {
        StrategyPlanSupportV1::Weak
    };

    StrategyPackageV2 {
        id: StrategyPackageIdV2::PotionCapacity,
        domain: StrategyPackageDomainV2::Resource,
        support,
        missing_roles: Vec::new(),
        evidence: vec![format!(
            "potions={}/{} empty_slots={}",
            resources.potion_count, resources.potion_slots, resources.empty_potion_slots
        )],
        blockers: if resources.empty_potion_slots == 0 {
            vec!["no empty potion slot for safe auto-claim or potion reward preference".to_string()]
        } else {
            Vec::new()
        },
        risks: Vec::new(),
    }
}

fn shop_remove_window_package(resources: &StrategyResourceFactsV2) -> StrategyPackageV2 {
    let can_remove = resources.gold >= resources.estimated_purge_cost;
    let support = if resources.removable_curses > 0 && can_remove {
        StrategyPlanSupportV1::Strong
    } else if resources.curses > 0 {
        StrategyPlanSupportV1::Plausible
    } else if resources.starter_cards >= 6 && can_remove {
        StrategyPlanSupportV1::Weak
    } else {
        StrategyPlanSupportV1::Blocked
    };

    StrategyPackageV2 {
        id: StrategyPackageIdV2::ShopRemoveWindow,
        domain: StrategyPackageDomainV2::Resource,
        support,
        missing_roles: Vec::new(),
        evidence: vec![
            format!("curses={}", resources.curses),
            format!("removable_curses={}", resources.removable_curses),
            format!("starter_cards={}", resources.starter_cards),
            format!("gold={}", resources.gold),
            format!("estimated purge cost={}", resources.estimated_purge_cost),
        ],
        blockers: if !can_remove {
            vec!["gold does not cover estimated purge cost".to_string()]
        } else {
            Vec::new()
        },
        risks: if resources.curses == 0 && resources.starter_cards > 0 {
            vec!["starter removal can conflict with short-term frontload needs".to_string()]
        } else {
            Vec::new()
        },
    }
}

fn relic_constraints_package(resources: &StrategyResourceFactsV2) -> StrategyPackageV2 {
    let support = if resources.relic_constraints.is_empty() {
        StrategyPlanSupportV1::Blocked
    } else {
        StrategyPlanSupportV1::Strong
    };
    StrategyPackageV2 {
        id: StrategyPackageIdV2::RelicConstraints,
        domain: StrategyPackageDomainV2::Resource,
        support,
        missing_roles: Vec::new(),
        evidence: resources.relic_constraints.clone(),
        blockers: Vec::new(),
        risks: resources
            .relic_constraints
            .iter()
            .map(|constraint| format!("relic constraint active: {constraint}"))
            .collect(),
    }
}

fn archetype_missing_roles(
    plan: &DeckPlanHypothesisV1,
    deck: &StrategyDeckFactsV1,
) -> Vec<StrategyPackageGapV2> {
    match plan.id {
        StrategyPlanIdV1::UpgradeSink => upgrade_sink_missing_roles(plan, deck),
        StrategyPlanIdV1::WeakControl => {
            if deck.weak_sources == 0 && plan.support != StrategyPlanSupportV1::Blocked {
                vec![StrategyPackageGapV2::Generator]
            } else {
                Vec::new()
            }
        }
        StrategyPlanIdV1::StrengthScaling => {
            generator_payoff_missing_roles(deck.strength_sources, deck.strength_payoffs)
        }
        StrategyPlanIdV1::BlockEngine => block_engine_missing_roles(deck),
        StrategyPlanIdV1::ExhaustEngine => {
            generator_payoff_missing_roles(deck.exhaust_generators, deck.exhaust_payoffs)
        }
        StrategyPlanIdV1::StatusPackage => {
            generator_payoff_missing_roles(deck.status_generators, deck.status_payoffs)
        }
        _ => Vec::new(),
    }
}

fn upgrade_sink_missing_roles(
    plan: &DeckPlanHypothesisV1,
    deck: &StrategyDeckFactsV1,
) -> Vec<StrategyPackageGapV2> {
    let mut roles = Vec::new();
    if deck.route_upgrade_payoffs == 0 {
        roles.push(StrategyPackageGapV2::UpgradeConsumer);
    }
    if matches!(
        plan.support,
        StrategyPlanSupportV1::Blocked | StrategyPlanSupportV1::Weak
    ) {
        roles.push(StrategyPackageGapV2::UpgradeBudget);
    }
    roles
}

fn generator_payoff_missing_roles(generators: u8, payoffs: u8) -> Vec<StrategyPackageGapV2> {
    let mut roles = Vec::new();
    if generators == 0 {
        roles.push(StrategyPackageGapV2::Generator);
    }
    if payoffs == 0 {
        roles.push(StrategyPackageGapV2::Payoff);
    }
    roles
}

fn block_engine_missing_roles(deck: &StrategyDeckFactsV1) -> Vec<StrategyPackageGapV2> {
    let mut roles = Vec::new();
    if deck.block_retention_sources == 0 {
        roles.push(StrategyPackageGapV2::BlockRetention);
    }
    if deck.block_payoffs == 0 {
        roles.push(StrategyPackageGapV2::BlockPayoff);
    }
    if deck.block_multipliers == 0 {
        roles.push(StrategyPackageGapV2::BlockMultiplier);
    }
    roles
}

fn hp_ratio(resources: &StrategyResourceFactsV2) -> f32 {
    if resources.max_hp > 0 {
        (resources.current_hp.max(0) as f32 / resources.max_hp as f32).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn empty_resource_facts() -> StrategyResourceFactsV2 {
    StrategyResourceFactsV2 {
        current_hp: 0,
        max_hp: 0,
        anticipated_next_combat_start_heal: 0,
        effective_next_combat_hp: 0,
        gold: 0,
        estimated_purge_cost: 75,
        potion_slots: 0,
        potion_count: 0,
        empty_potion_slots: 0,
        curses: 0,
        removable_curses: 0,
        starter_cards: 0,
        relic_constraints: Vec::new(),
    }
}
