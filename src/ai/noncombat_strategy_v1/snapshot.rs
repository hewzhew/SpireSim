use super::coverage::FRONTLOAD_SUPPORTED_TOTAL_DAMAGE_V1;
use super::formation::assess_deck_formation_v1;
use super::pressure::{pressure_from_need_v1, route_pressure_v1};
use super::route_package::assess_route_packages_v1;
use super::types::{
    DeckPlanHypothesisV1, RunStrategySnapshotV1, StrategyDeckFactsV1, StrategyPlanIdV1,
    StrategyPlanPressureV1, StrategyPlanSupportV1, StrategyRouteFutureV1,
};

pub fn build_run_strategy_snapshot_v1(
    deck: StrategyDeckFactsV1,
    route: Option<StrategyRouteFutureV1>,
) -> RunStrategySnapshotV1 {
    let plans = vec![
        frontload_survival_plan(&deck, route.as_ref()),
        weak_control_plan(&deck, route.as_ref()),
        strength_scaling_plan(&deck),
        upgrade_sink_plan(&deck, route.as_ref()),
        exhaust_engine_plan(&deck),
        block_engine_plan(&deck),
        strike_density_plan(),
        status_package_plan(&deck),
        self_damage_plan(),
        energy_draw_plan(),
    ];
    let formation = assess_deck_formation_v1(&deck, route.as_ref(), &plans);
    let route_packages = assess_route_packages_v1(route.as_ref(), &formation, &plans);

    RunStrategySnapshotV1 {
        deck,
        route,
        plans,
        formation,
        route_packages,
    }
}

fn frontload_survival_plan(
    deck: &StrategyDeckFactsV1,
    route: Option<&StrategyRouteFutureV1>,
) -> DeckPlanHypothesisV1 {
    let route_pressure = route_pressure_v1(route);
    let low_damage = deck.total_attack_damage < FRONTLOAD_SUPPORTED_TOTAL_DAMAGE_V1;
    let support = if route_pressure == StrategyPlanPressureV1::High || low_damage {
        StrategyPlanSupportV1::Strong
    } else if route_pressure == StrategyPlanPressureV1::Medium {
        StrategyPlanSupportV1::Plausible
    } else {
        StrategyPlanSupportV1::Weak
    };

    DeckPlanHypothesisV1 {
        id: StrategyPlanIdV1::FrontloadSurvival,
        support,
        evidence: vec![
            format!("total attack damage fact is {}", deck.total_attack_damage),
            format!("route pressure is {route_pressure:?}"),
        ],
        blockers: Vec::new(),
        opportunity_costs: vec!["frontload picks can delay scaling commitments".to_string()],
    }
}

fn weak_control_plan(
    deck: &StrategyDeckFactsV1,
    route: Option<&StrategyRouteFutureV1>,
) -> DeckPlanHypothesisV1 {
    let route_pressure = route_pressure_v1(route);
    let support = if deck.weak_sources > 0 {
        StrategyPlanSupportV1::Blocked
    } else if route_pressure == StrategyPlanPressureV1::High {
        StrategyPlanSupportV1::Strong
    } else if route_pressure == StrategyPlanPressureV1::Medium {
        StrategyPlanSupportV1::Plausible
    } else {
        StrategyPlanSupportV1::Weak
    };
    let blockers = if deck.weak_sources > 0 {
        vec![format!(
            "deck already has {} weak source(s)",
            deck.weak_sources
        )]
    } else {
        Vec::new()
    };

    DeckPlanHypothesisV1 {
        id: StrategyPlanIdV1::WeakControl,
        support,
        evidence: vec![format!("route pressure is {route_pressure:?}")],
        blockers,
        opportunity_costs: vec![
            "weak/frontload cards may not solve long-combat scaling".to_string()
        ],
    }
}

fn strength_scaling_plan(deck: &StrategyDeckFactsV1) -> DeckPlanHypothesisV1 {
    let support = if deck.strength_sources > 0 {
        StrategyPlanSupportV1::Strong
    } else if deck.strength_payoffs > 0 {
        StrategyPlanSupportV1::Weak
    } else {
        StrategyPlanSupportV1::Blocked
    };

    DeckPlanHypothesisV1 {
        id: StrategyPlanIdV1::StrengthScaling,
        support,
        evidence: vec![
            format!("strength sources={}", deck.strength_sources),
            format!("strength payoffs={}", deck.strength_payoffs),
        ],
        blockers: if deck.strength_sources == 0 {
            vec!["no visible strength source".to_string()]
        } else {
            Vec::new()
        },
        opportunity_costs: vec!["strength payoff cards are weak without source density".to_string()],
    }
}

fn upgrade_sink_plan(
    deck: &StrategyDeckFactsV1,
    route: Option<&StrategyRouteFutureV1>,
) -> DeckPlanHypothesisV1 {
    let rest_pressure = route
        .map(|route| pressure_from_need_v1(route.need_heal.max(route.avoid_damage)))
        .unwrap_or(StrategyPlanPressureV1::High);
    let (min_fires, max_fires, first_fire_floor) = route
        .map(|route| (route.min_fires, route.max_fires, route.first_fire_floor))
        .unwrap_or((0, 0, None));
    let contested = deck
        .important_cards_unupgraded
        .saturating_add(deck.route_upgrade_payoffs);

    let support = if route.is_none() {
        StrategyPlanSupportV1::Blocked
    } else if max_fires >= 4
        && min_fires >= 3
        && rest_pressure == StrategyPlanPressureV1::Low
        && contested <= 1
    {
        StrategyPlanSupportV1::Strong
    } else if max_fires >= 3 && rest_pressure != StrategyPlanPressureV1::High {
        StrategyPlanSupportV1::Plausible
    } else if max_fires >= 2 {
        StrategyPlanSupportV1::Weak
    } else {
        StrategyPlanSupportV1::Blocked
    };

    let mut blockers = Vec::new();
    if route.is_none() {
        blockers.push("route fire budget unavailable".to_string());
    }
    if rest_pressure == StrategyPlanPressureV1::High {
        blockers.push("rest pressure may consume fires".to_string());
    }

    DeckPlanHypothesisV1 {
        id: StrategyPlanIdV1::UpgradeSink,
        support,
        evidence: vec![
            format!("visible route has {min_fires}-{max_fires} fire(s), first fire {first_fire_floor:?}"),
            format!("upgrade competition count is {contested}"),
            format!("rest pressure is {rest_pressure:?}"),
        ],
        blockers,
        opportunity_costs: vec!["upgrade sink consumes smiths that could upgrade core cards".to_string()],
    }
}

fn exhaust_engine_plan(deck: &StrategyDeckFactsV1) -> DeckPlanHypothesisV1 {
    package_plan(
        StrategyPlanIdV1::ExhaustEngine,
        deck.exhaust_generators,
        deck.exhaust_payoffs,
        "exhaust",
    )
}

fn block_engine_plan(deck: &StrategyDeckFactsV1) -> DeckPlanHypothesisV1 {
    let has_retention = deck.block_retention_sources > 0;
    let has_payoff = deck.block_payoffs > 0;
    let has_multiplier = deck.block_multipliers > 0;
    let support = if has_retention && (has_payoff || has_multiplier) && deck.total_block >= 25 {
        StrategyPlanSupportV1::Strong
    } else if (has_payoff && deck.total_block >= 30)
        || (has_retention && deck.total_block >= 25)
        || deck.total_block >= 35
    {
        StrategyPlanSupportV1::Plausible
    } else if deck.total_block >= 25 || has_retention || has_payoff || has_multiplier {
        StrategyPlanSupportV1::Weak
    } else {
        StrategyPlanSupportV1::Blocked
    };
    DeckPlanHypothesisV1 {
        id: StrategyPlanIdV1::BlockEngine,
        support,
        evidence: vec![
            format!("total block fact is {}", deck.total_block),
            format!("block retention sources={}", deck.block_retention_sources),
            format!("block payoffs={}", deck.block_payoffs),
            format!("block multipliers={}", deck.block_multipliers),
        ],
        blockers: if support == StrategyPlanSupportV1::Blocked {
            vec!["block engine needs block generation plus retention/payoff evidence".to_string()]
        } else {
            Vec::new()
        },
        opportunity_costs: vec![
            "block engine needs payoff density before it can be a plan".to_string()
        ],
    }
}

fn strike_density_plan() -> DeckPlanHypothesisV1 {
    DeckPlanHypothesisV1 {
        id: StrategyPlanIdV1::StrikeDensity,
        support: StrategyPlanSupportV1::Weak,
        evidence: vec!["strike-density plan needs explicit strike count evidence".to_string()],
        blockers: Vec::new(),
        opportunity_costs: vec!["strike-density picks conflict with removals".to_string()],
    }
}

fn status_package_plan(deck: &StrategyDeckFactsV1) -> DeckPlanHypothesisV1 {
    package_plan(
        StrategyPlanIdV1::StatusPackage,
        deck.status_generators,
        deck.status_payoffs,
        "status",
    )
}

fn self_damage_plan() -> DeckPlanHypothesisV1 {
    DeckPlanHypothesisV1 {
        id: StrategyPlanIdV1::SelfDamage,
        support: StrategyPlanSupportV1::Blocked,
        evidence: Vec::new(),
        blockers: vec!["self-damage plan needs hp/relic/payoff evidence".to_string()],
        opportunity_costs: Vec::new(),
    }
}

fn energy_draw_plan() -> DeckPlanHypothesisV1 {
    DeckPlanHypothesisV1 {
        id: StrategyPlanIdV1::EnergyDraw,
        support: StrategyPlanSupportV1::Weak,
        evidence: vec!["energy/draw plan needs explicit source and payoff evidence".to_string()],
        blockers: Vec::new(),
        opportunity_costs: Vec::new(),
    }
}

fn package_plan(
    id: StrategyPlanIdV1,
    generators: u8,
    payoffs: u8,
    label: &'static str,
) -> DeckPlanHypothesisV1 {
    let support = if generators > 0 && payoffs > 0 {
        StrategyPlanSupportV1::Strong
    } else if generators > 0 || payoffs > 0 {
        StrategyPlanSupportV1::Plausible
    } else {
        StrategyPlanSupportV1::Blocked
    };

    DeckPlanHypothesisV1 {
        id,
        support,
        evidence: vec![format!(
            "{label} generators={generators}, payoffs={payoffs}"
        )],
        blockers: if generators == 0 && payoffs == 0 {
            vec![format!("no visible {label} package evidence")]
        } else {
            Vec::new()
        },
        opportunity_costs: Vec::new(),
    }
}
