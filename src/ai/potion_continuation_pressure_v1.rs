use serde::{Deserialize, Serialize};

use crate::ai::potion_continuation_context_v1::{
    PotionRunContinuationContextV1, PotionRunInventoryContextV1, PotionRunSupplyContextV1,
};
use crate::ai::route_window_facts::{
    RouteWindowCoverageKind, RouteWindowFacts, RouteWindowModality, RouteWindowPredicate,
    RouteWindowProvenance, RouteWindowSubject,
};
use crate::content::monsters::factory::EncounterId;
use crate::content::relics::RelicId;
use crate::state::RunState;

pub const POTION_CONTINUATION_PRESSURE_SCHEMA_NAME: &str = "PotionContinuationPressure";
pub const POTION_CONTINUATION_PRESSURE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionRouteCountRangeV1 {
    pub min: usize,
    pub max: usize,
    pub modality: RouteWindowModality,
    pub provenance: RouteWindowProvenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionRoutePressureV1 {
    pub coverage_kind: RouteWindowCoverageKind,
    pub horizon_nodes: usize,
    pub path_budget_exhausted: bool,
    pub observed_path_count: usize,
    pub hallway_combats: Option<PotionRouteCountRangeV1>,
    pub elites: Option<PotionRouteCountRangeV1>,
    pub campfires: Option<PotionRouteCountRangeV1>,
    pub shops: Option<PotionRouteCountRangeV1>,
    pub unknown_rooms: Option<PotionRouteCountRangeV1>,
    pub bosses: Option<PotionRouteCountRangeV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionShopContinuationFactsV1 {
    pub current_gold: i32,
    pub shop_observed_on_some_covered_path: bool,
    pub shop_observed_on_all_covered_paths: bool,
    pub future_shop_inventory_unknown: bool,
    pub future_potion_price_unknown: bool,
    pub gold_on_shop_arrival_unknown: bool,
    pub potion_affordability_at_shop_unknown: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionRecoveryContinuationFactsV1 {
    pub current_hp_deficit: i32,
    pub campfire_observed_on_some_covered_path: bool,
    pub campfire_observed_on_all_covered_paths: bool,
    pub coffee_dripper_blocks_rest: bool,
    pub rest_may_be_available_on_some_covered_path: bool,
    pub rest_may_be_available_on_all_covered_paths: bool,
    pub recovery_choice_opportunity_cost_unscored: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PotionContinuationPressureLimitationV1 {
    RouteBeyondHorizonNotObserved,
    FutureShopInventoryUnknown,
    FuturePotionPriceUnknown,
    GoldOnShopArrivalUnknown,
    CampfireChoiceOpportunityCostUnscored,
    UnknownRoomOutcomeUnresolved,
    VisibleBossEntryNotRepresentedByMapWindow,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionContinuationPressureV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub capture_boundary: String,
    pub act: u8,
    pub floor: i32,
    pub visible_boss: Option<EncounterId>,
    pub inventory: PotionRunInventoryContextV1,
    pub supply: PotionRunSupplyContextV1,
    pub route: PotionRoutePressureV1,
    pub shop: PotionShopContinuationFactsV1,
    pub recovery: PotionRecoveryContinuationFactsV1,
    pub limitations: Vec<PotionContinuationPressureLimitationV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PotionContinuationPressureInputsV1 {
    pub current_gold: i32,
    pub coffee_dripper_blocks_rest: bool,
}

pub fn potion_continuation_pressure_v1(
    run_state: &RunState,
    context: &PotionRunContinuationContextV1,
) -> PotionContinuationPressureV1 {
    potion_continuation_pressure_from_context_v1(
        context,
        PotionContinuationPressureInputsV1 {
            current_gold: run_state.gold,
            coffee_dripper_blocks_rest: run_state
                .relics
                .iter()
                .any(|relic| relic.id == RelicId::CoffeeDripper),
        },
    )
}

pub fn potion_continuation_pressure_from_context_v1(
    context: &PotionRunContinuationContextV1,
    inputs: PotionContinuationPressureInputsV1,
) -> PotionContinuationPressureV1 {
    let route = route_pressure(&context.route_window);
    let shop_on_some_path = observed_on_some_path(route.shops.as_ref());
    let shop_on_all_paths = observed_on_all_paths(route.shops.as_ref());
    let campfire_on_some_path = observed_on_some_path(route.campfires.as_ref());
    let campfire_on_all_paths = observed_on_all_paths(route.campfires.as_ref());
    let rest_may_be_available_on_some_covered_path =
        campfire_on_some_path && !inputs.coffee_dripper_blocks_rest;
    let rest_may_be_available_on_all_covered_paths =
        campfire_on_all_paths && !inputs.coffee_dripper_blocks_rest;
    let unknown_room_observed = observed_on_some_path(route.unknown_rooms.as_ref());
    let boss_in_window = observed_on_some_path(route.bosses.as_ref());
    let mut limitations =
        vec![PotionContinuationPressureLimitationV1::RouteBeyondHorizonNotObserved];
    if shop_on_some_path {
        limitations.extend([
            PotionContinuationPressureLimitationV1::FutureShopInventoryUnknown,
            PotionContinuationPressureLimitationV1::FuturePotionPriceUnknown,
            PotionContinuationPressureLimitationV1::GoldOnShopArrivalUnknown,
        ]);
    }
    if rest_may_be_available_on_some_covered_path {
        limitations
            .push(PotionContinuationPressureLimitationV1::CampfireChoiceOpportunityCostUnscored);
    }
    if unknown_room_observed {
        limitations.push(PotionContinuationPressureLimitationV1::UnknownRoomOutcomeUnresolved);
    }
    if context.visible_boss.is_some() && !boss_in_window {
        limitations.push(
            PotionContinuationPressureLimitationV1::VisibleBossEntryNotRepresentedByMapWindow,
        );
    }

    PotionContinuationPressureV1 {
        schema_name: POTION_CONTINUATION_PRESSURE_SCHEMA_NAME.to_owned(),
        schema_version: POTION_CONTINUATION_PRESSURE_SCHEMA_VERSION,
        capture_boundary: context.capture_boundary.clone(),
        act: context.act,
        floor: context.floor,
        visible_boss: context.visible_boss,
        inventory: context.inventory.clone(),
        supply: context.supply.clone(),
        route,
        shop: PotionShopContinuationFactsV1 {
            current_gold: inputs.current_gold,
            shop_observed_on_some_covered_path: shop_on_some_path,
            shop_observed_on_all_covered_paths: shop_on_all_paths,
            future_shop_inventory_unknown: shop_on_some_path,
            future_potion_price_unknown: shop_on_some_path,
            gold_on_shop_arrival_unknown: shop_on_some_path,
            potion_affordability_at_shop_unknown: shop_on_some_path,
        },
        recovery: PotionRecoveryContinuationFactsV1 {
            current_hp_deficit: context.max_hp.saturating_sub(context.current_hp),
            campfire_observed_on_some_covered_path: campfire_on_some_path,
            campfire_observed_on_all_covered_paths: campfire_on_all_paths,
            coffee_dripper_blocks_rest: inputs.coffee_dripper_blocks_rest,
            rest_may_be_available_on_some_covered_path,
            rest_may_be_available_on_all_covered_paths,
            recovery_choice_opportunity_cost_unscored: rest_may_be_available_on_some_covered_path,
        },
        limitations,
    }
}

fn route_pressure(facts: &RouteWindowFacts) -> PotionRoutePressureV1 {
    PotionRoutePressureV1 {
        coverage_kind: facts.coverage.kind,
        horizon_nodes: facts.coverage.horizon_nodes,
        path_budget_exhausted: facts.coverage.path_budget_exhausted,
        observed_path_count: facts.observed_path_count,
        hallway_combats: count_range(facts, RouteWindowSubject::HallwayCombat),
        elites: count_range(facts, RouteWindowSubject::Elite),
        campfires: count_range(facts, RouteWindowSubject::Campfire),
        shops: count_range(facts, RouteWindowSubject::Shop),
        unknown_rooms: count_range(facts, RouteWindowSubject::UnknownRoom),
        bosses: count_range(facts, RouteWindowSubject::Boss),
    }
}

fn count_range(
    facts: &RouteWindowFacts,
    expected_subject: RouteWindowSubject,
) -> Option<PotionRouteCountRangeV1> {
    facts.facts.iter().find_map(|fact| {
        let RouteWindowPredicate::CountRangeInWindow { subject, min, max } = &fact.predicate else {
            return None;
        };
        (*subject == expected_subject).then_some(PotionRouteCountRangeV1 {
            min: *min,
            max: *max,
            modality: fact.modality,
            provenance: fact.provenance,
        })
    })
}

fn observed_on_some_path(range: Option<&PotionRouteCountRangeV1>) -> bool {
    range.is_some_and(|range| range.max > 0)
}

fn observed_on_all_paths(range: Option<&PotionRouteCountRangeV1>) -> bool {
    range.is_some_and(|range| range.min > 0 && range.modality == RouteWindowModality::Must)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::potion_continuation_context_v1::potion_run_continuation_context_v1;
    use crate::ai::route_window_facts::{RouteWindowFact, RouteWindowKind, RouteWindowScope};
    use crate::content::relics::RelicState;

    fn count_fact(
        subject: RouteWindowSubject,
        min: usize,
        max: usize,
        modality: RouteWindowModality,
        provenance: RouteWindowProvenance,
    ) -> RouteWindowFact {
        RouteWindowFact {
            window: RouteWindowKind::Coverage,
            predicate: RouteWindowPredicate::CountRangeInWindow { subject, min, max },
            modality,
            scope: RouteWindowScope::PathFamily,
            horizon_nodes: 5,
            provenance,
        }
    }

    #[test]
    fn pressure_compacts_route_liquidity_and_recovery_without_scoring() {
        let mut run_state = RunState::new(7, 0, false, "Ironclad");
        run_state.act_num = 1;
        run_state.floor_num = 4;
        run_state.gold = 57;
        run_state.boss_key = Some(EncounterId::Hexaghost);
        run_state
            .relics
            .push(RelicState::new(RelicId::CoffeeDripper));
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.player.current_hp = 40;
        combat.entities.player.max_hp = 80;
        let mut context = potion_run_continuation_context_v1(&run_state, &combat);
        context.route_window.coverage.kind = RouteWindowCoverageKind::CompleteWithinHorizon;
        context.route_window.coverage.path_budget_exhausted = false;
        context.route_window.observed_path_count = 4;
        context.route_window.facts = vec![
            count_fact(
                RouteWindowSubject::HallwayCombat,
                0,
                1,
                RouteWindowModality::Can,
                RouteWindowProvenance::SomeCoveredPath,
            ),
            count_fact(
                RouteWindowSubject::Elite,
                0,
                1,
                RouteWindowModality::Can,
                RouteWindowProvenance::SomeCoveredPath,
            ),
            count_fact(
                RouteWindowSubject::Campfire,
                1,
                2,
                RouteWindowModality::Must,
                RouteWindowProvenance::AllCoveredPaths,
            ),
            count_fact(
                RouteWindowSubject::Shop,
                1,
                1,
                RouteWindowModality::Must,
                RouteWindowProvenance::AllCoveredPaths,
            ),
            count_fact(
                RouteWindowSubject::UnknownRoom,
                1,
                1,
                RouteWindowModality::Must,
                RouteWindowProvenance::AllCoveredPaths,
            ),
            count_fact(
                RouteWindowSubject::Boss,
                0,
                0,
                RouteWindowModality::Cannot,
                RouteWindowProvenance::NoCoveredPathComplete,
            ),
        ];

        let pressure = potion_continuation_pressure_v1(&run_state, &context);
        let rebuilt = potion_continuation_pressure_from_context_v1(
            &context,
            PotionContinuationPressureInputsV1 {
                current_gold: 57,
                coffee_dripper_blocks_rest: true,
            },
        );

        assert_eq!(rebuilt, pressure);
        assert_eq!(pressure.shop.current_gold, 57);
        assert!(pressure.shop.shop_observed_on_all_covered_paths);
        assert!(pressure.shop.potion_affordability_at_shop_unknown);
        assert_eq!(pressure.recovery.current_hp_deficit, 40);
        assert!(pressure.recovery.campfire_observed_on_all_covered_paths);
        assert!(pressure.recovery.coffee_dripper_blocks_rest);
        assert!(!pressure.recovery.rest_may_be_available_on_some_covered_path);
        assert_eq!(pressure.route.observed_path_count, 4);
        assert_eq!(pressure.route.elites.as_ref().unwrap().max, 1);
        assert!(pressure
            .limitations
            .contains(&PotionContinuationPressureLimitationV1::FutureShopInventoryUnknown));
        assert!(pressure
            .limitations
            .contains(&PotionContinuationPressureLimitationV1::UnknownRoomOutcomeUnresolved));
        assert!(pressure.limitations.contains(
            &PotionContinuationPressureLimitationV1::VisibleBossEntryNotRepresentedByMapWindow
        ));
        assert!(!pressure.limitations.contains(
            &PotionContinuationPressureLimitationV1::CampfireChoiceOpportunityCostUnscored
        ));

        run_state
            .relics
            .retain(|relic| relic.id != RelicId::CoffeeDripper);
        let rest_pressure = potion_continuation_pressure_v1(&run_state, &context);
        assert!(
            rest_pressure
                .recovery
                .rest_may_be_available_on_all_covered_paths
        );
        assert!(rest_pressure.limitations.contains(
            &PotionContinuationPressureLimitationV1::CampfireChoiceOpportunityCostUnscored
        ));

        let payload = serde_json::to_value(&pressure).expect("serialize pressure");
        assert_eq!(
            payload["schema_name"],
            POTION_CONTINUATION_PRESSURE_SCHEMA_NAME
        );
        let restored: PotionContinuationPressureV1 =
            serde_json::from_value(payload).expect("deserialize pressure");
        assert_eq!(restored, pressure);
    }

    #[test]
    fn unavailable_map_does_not_invent_shop_or_recovery_pressure() {
        let run_state = RunState::new(11, 0, false, "Ironclad");
        let combat = crate::test_support::blank_test_combat();
        let mut context = potion_run_continuation_context_v1(&run_state, &combat);
        context.route_window.coverage.kind = RouteWindowCoverageKind::UnavailableMap;
        context.route_window.observed_path_count = 0;
        context.route_window.facts.clear();

        let pressure = potion_continuation_pressure_v1(&run_state, &context);

        assert_eq!(
            pressure.route.coverage_kind,
            RouteWindowCoverageKind::UnavailableMap
        );
        assert!(!pressure.shop.shop_observed_on_some_covered_path);
        assert!(!pressure.shop.future_shop_inventory_unknown);
        assert!(!pressure.recovery.campfire_observed_on_some_covered_path);
        assert!(!pressure.recovery.recovery_choice_opportunity_cost_unscored);
        assert!(!pressure
            .limitations
            .contains(&PotionContinuationPressureLimitationV1::FutureShopInventoryUnknown));
        assert!(!pressure.limitations.contains(
            &PotionContinuationPressureLimitationV1::CampfireChoiceOpportunityCostUnscored
        ));
    }
}
