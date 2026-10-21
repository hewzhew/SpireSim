use super::coverage::FRONTLOAD_SUPPORTED_TOTAL_DAMAGE_V1;
use super::pressure::route_pressure_v1;
use super::types::{
    DeckPlanHypothesisV1, StrategyDeckFactsV1, StrategyDeckFormationNeedV1,
    StrategyDeckFormationStageV1, StrategyDeckFormationV1, StrategyPlanIdV1,
    StrategyPlanPressureV1, StrategyPlanSupportV1, StrategyRouteFutureV1,
};

pub fn assess_deck_formation_v1(
    deck: &StrategyDeckFactsV1,
    route: Option<&StrategyRouteFutureV1>,
    plans: &[DeckPlanHypothesisV1],
) -> StrategyDeckFormationV1 {
    let route_pressure = route_pressure_v1(route);
    let strengths = committed_plan_strengths(plans);
    let seeded_plan_count = plans
        .iter()
        .filter(|plan| is_core_formation_plan(plan.id))
        .filter(|plan| {
            matches!(
                plan.support,
                StrategyPlanSupportV1::Plausible | StrategyPlanSupportV1::Strong
            )
        })
        .count();

    let mut needs = Vec::new();
    if needs_frontload(deck, route, route_pressure) {
        push_unique(&mut needs, StrategyDeckFormationNeedV1::Frontload);
    }
    if needs_block(deck, route, route_pressure) {
        push_unique(&mut needs, StrategyDeckFormationNeedV1::Block);
    }
    if strengths.is_empty() {
        push_unique(&mut needs, StrategyDeckFormationNeedV1::Scaling);
    }
    if deck.deck_size >= 15 && deck.draw_sources == 0 && deck.energy_sources == 0 {
        push_unique(&mut needs, StrategyDeckFormationNeedV1::DrawEnergy);
    }
    if deck.deck_size >= 20 || deck.starter_strikes.saturating_add(deck.starter_defends) >= 7 {
        push_unique(&mut needs, StrategyDeckFormationNeedV1::Consistency);
    }

    let stage = formation_stage(deck, strengths.len(), seeded_plan_count, needs.len());
    let blockers = formation_blockers(deck, route, &strengths, &needs);
    let notes = formation_notes(deck, route, route_pressure, seeded_plan_count);

    StrategyDeckFormationV1 {
        stage,
        needs,
        strengths,
        blockers,
        notes,
    }
}

fn committed_plan_strengths(plans: &[DeckPlanHypothesisV1]) -> Vec<StrategyPlanIdV1> {
    plans
        .iter()
        .filter(|plan| is_core_formation_plan(plan.id))
        .filter(|plan| plan.support == StrategyPlanSupportV1::Strong)
        .map(|plan| plan.id)
        .collect()
}

fn is_core_formation_plan(id: StrategyPlanIdV1) -> bool {
    matches!(
        id,
        StrategyPlanIdV1::StrengthScaling
            | StrategyPlanIdV1::UpgradeSink
            | StrategyPlanIdV1::ExhaustEngine
            | StrategyPlanIdV1::BlockEngine
            | StrategyPlanIdV1::StrikeDensity
            | StrategyPlanIdV1::StatusPackage
            | StrategyPlanIdV1::SelfDamage
            | StrategyPlanIdV1::EnergyDraw
    )
}

fn formation_stage(
    deck: &StrategyDeckFactsV1,
    committed_plan_count: usize,
    seeded_plan_count: usize,
    need_count: usize,
) -> StrategyDeckFormationStageV1 {
    if committed_plan_count >= 2 && need_count <= 1 {
        StrategyDeckFormationStageV1::Mature
    } else if committed_plan_count >= 1 {
        StrategyDeckFormationStageV1::PlanCommitted
    } else if seeded_plan_count >= 1 {
        StrategyDeckFormationStageV1::PlanSeeded
    } else if deck.deck_size <= 12 {
        StrategyDeckFormationStageV1::StarterShell
    } else {
        StrategyDeckFormationStageV1::Transitional
    }
}

fn needs_block(
    deck: &StrategyDeckFactsV1,
    route: Option<&StrategyRouteFutureV1>,
    route_pressure: StrategyPlanPressureV1,
) -> bool {
    if deck.total_block >= 25 {
        return false;
    }
    if route_pressure == StrategyPlanPressureV1::High {
        return true;
    }
    let Some(route) = route else {
        return true;
    };
    route.avoid_damage >= 0.30 || route.max_early_pressure >= 2 || route.need_heal >= 0.30
}

fn needs_frontload(
    deck: &StrategyDeckFactsV1,
    route: Option<&StrategyRouteFutureV1>,
    route_pressure: StrategyPlanPressureV1,
) -> bool {
    if deck.total_attack_damage < FRONTLOAD_SUPPORTED_TOTAL_DAMAGE_V1
        || route_pressure == StrategyPlanPressureV1::High
    {
        return true;
    }
    let Some(route) = route else {
        return false;
    };
    route.max_early_pressure >= 2 && route.max_fires <= 1 && deck.total_attack_damage < 60
}

fn formation_blockers(
    deck: &StrategyDeckFactsV1,
    route: Option<&StrategyRouteFutureV1>,
    strengths: &[StrategyPlanIdV1],
    needs: &[StrategyDeckFormationNeedV1],
) -> Vec<String> {
    let mut blockers = Vec::new();
    if strengths.is_empty() {
        blockers.push("no committed core plan is supported by current deck facts".to_string());
    }
    if needs.contains(&StrategyDeckFormationNeedV1::Frontload) {
        blockers.push(format!(
            "frontload damage fact is low or route pressure is high: total_attack_damage={}",
            deck.total_attack_damage
        ));
    }
    if needs.contains(&StrategyDeckFormationNeedV1::Consistency) {
        blockers.push(format!(
            "deck still has starter density or size pressure: deck_size={}, starter_strikes={}, starter_defends={}",
            deck.deck_size, deck.starter_strikes, deck.starter_defends
        ));
    }
    if route.is_none() {
        blockers.push(
            "route future unavailable, formation cannot judge fire/rest pressure".to_string(),
        );
    }
    blockers
}

fn formation_notes(
    deck: &StrategyDeckFactsV1,
    route: Option<&StrategyRouteFutureV1>,
    route_pressure: StrategyPlanPressureV1,
    seeded_plan_count: usize,
) -> Vec<String> {
    let mut notes = vec![
        format!(
            "deck mix: attacks={}, skills={}, powers={}, size={}",
            deck.attacks, deck.skills, deck.powers, deck.deck_size
        ),
        format!(
            "resource facts: draw_sources={}, energy_sources={}, vulnerable_sources={}, weak_sources={}",
            deck.draw_sources, deck.energy_sources, deck.vulnerable_sources, deck.weak_sources
        ),
        format!("route pressure is {route_pressure:?}"),
        format!("seeded core plan count is {seeded_plan_count}"),
    ];

    if let Some(route) = route {
        notes.push(format!(
            "visible route fire budget is {}-{} with first fire {:?}",
            route.min_fires, route.max_fires, route.first_fire_floor
        ));
    }

    notes
}

fn push_unique<T: Copy + Eq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
