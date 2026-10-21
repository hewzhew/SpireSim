//! Event resource budgets translate route-window facts plus the current run
//! state into typed resource constraints.
//!
//! This module does not choose event options. It does not score routes. It says
//! which resources are free, budgeted for high return, reserved, unavailable, or
//! route-breaking under the currently observed route facts.

use serde::{Deserialize, Serialize};

use crate::ai::route_window_facts::{
    RouteWindowCoverageKind, RouteWindowFact, RouteWindowFacts, RouteWindowModality,
    RouteWindowPredicate, RouteWindowProvenance, RouteWindowSubject,
};
use crate::content::cards::{get_card_definition, CardId, CardTag, CardType};
use crate::content::relics::RelicId;
use crate::state::RunState;

pub const EVENT_RESOURCE_BUDGET_SCHEMA_NAME: &str = "EventResourceBudget";
pub const EVENT_RESOURCE_BUDGET_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventResourceBudget {
    pub schema_name: String,
    pub schema_version: u32,
    pub route: EventRouteBudgetContext,
    pub hp: EventHpBudget,
    pub gold: EventGoldBudget,
    pub deck: EventDeckBudget,
    pub variance: EventVarianceBudget,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventRouteBudgetContext {
    pub coverage_kind: RouteWindowCoverageKind,
    pub known_combat_before_campfire: EventModalCertainty,
    pub known_shop_within_2: EventModalCertainty,
    pub known_shop_within_3: EventModalCertainty,
    pub campfire_within_2: EventModalCertainty,
    pub campfire_within_3: EventModalCertainty,
    pub elite_present: EventModalCertainty,
    pub elite_bypass: EventModalCertainty,
    pub unknown_shop_opportunity: EventModalCertainty,
    pub unknown_combat_opportunity: EventModalCertainty,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventModalCertainty {
    Must,
    Can,
    Cannot,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSpendClass {
    FreeToSpend,
    BudgetedForHighReturn,
    Reserved,
    RouteBreaking,
    Unavailable,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventGainClass {
    UsefulSoon,
    LedgerOnly,
    Blocked,
    UnknownOpportunity,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventHpBudget {
    pub current_hp: i32,
    pub max_hp: i32,
    pub route_break_floor: i32,
    pub reserve_floor: i32,
    pub free_floor: i32,
    pub small_loss: EventSpendClass,
    pub medium_loss: EventSpendClass,
    pub large_loss: EventSpendClass,
    pub session_loss: EventSpendClass,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<EventHpBudgetSignal>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventHpBudgetSignal {
    MustFightBeforeKnownCampfire,
    CampfireReachableSoon,
    EliteObservedInWindow,
    RouteCoverageIncomplete,
    CurrentHpBelowReserve,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventGoldBudget {
    pub current_gold: i32,
    pub estimated_next_shop_purge_cost: i32,
    pub gold_gain: EventGainClass,
    pub spend_50: EventSpendClass,
    pub spend_75: EventSpendClass,
    pub spend_to_purge_floor: EventSpendClass,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<EventGoldBudgetSignal>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventGoldBudgetSignal {
    EctoplasmBlocksGoldGain,
    KnownShopReachableSoon,
    OnlyQuestionMarkShopOpportunity,
    GoldAtOrAbovePurgeReserve,
    GoldNearPurgeReserve,
    DeckHasCleanupNeed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventDeckBudget {
    pub deck_size: usize,
    pub curse_count: usize,
    pub severe_curse_count: usize,
    pub starter_strike_count: usize,
    pub starter_defend_count: usize,
    pub low_value_transform_target_count: usize,
    pub curse_intake: EventSpendClass,
    pub random_transform: EventSpendClass,
    pub filler_card_addition: EventSpendClass,
    pub functional_card_loss: EventSpendClass,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<EventDeckBudgetSignal>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventDeckBudgetSignal {
    OmamoriCanNegateCurse,
    SevereCurseAlreadyPresent,
    HeavyDeck,
    ThinDeckProtectCore,
    StarterTransformTargetsAvailable,
    NoClearLowValueTransformTargets,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventVarianceBudget {
    pub tolerance: EventVarianceTolerance,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<EventVarianceBudgetSignal>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventVarianceTolerance {
    Low,
    Normal,
    HighReturnOnly,
    HighRollNeeded,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventVarianceBudgetSignal {
    RouteCoverageIncomplete,
    HpReserveTight,
    DeckPollutionReserveTight,
    NoKnownRecoverySoon,
    BehindRequiresHighRoll,
}

pub fn build_event_resource_budget(
    run_state: &RunState,
    route_facts: &RouteWindowFacts,
) -> EventResourceBudget {
    let route = event_route_budget_context(route_facts);
    let deck = event_deck_budget(run_state);
    let hp = event_hp_budget(run_state, &route);
    let gold = event_gold_budget(run_state, &route, &deck);
    let variance = event_variance_budget(&route, &hp, &deck);

    EventResourceBudget {
        schema_name: EVENT_RESOURCE_BUDGET_SCHEMA_NAME.to_string(),
        schema_version: EVENT_RESOURCE_BUDGET_SCHEMA_VERSION,
        route,
        hp,
        gold,
        deck,
        variance,
    }
}

fn event_route_budget_context(route_facts: &RouteWindowFacts) -> EventRouteBudgetContext {
    EventRouteBudgetContext {
        coverage_kind: route_facts.coverage.kind,
        known_combat_before_campfire: certainty_for_predicate(
            route_facts,
            &RouteWindowPredicate::OccursBefore {
                subject: RouteWindowSubject::KnownCombat,
                before: RouteWindowSubject::Campfire,
            },
        ),
        known_shop_within_2: certainty_for_predicate(
            route_facts,
            &RouteWindowPredicate::ReachableWithin {
                subject: RouteWindowSubject::Shop,
                nodes: 2,
            },
        ),
        known_shop_within_3: certainty_for_predicate(
            route_facts,
            &RouteWindowPredicate::ReachableWithin {
                subject: RouteWindowSubject::Shop,
                nodes: 3,
            },
        ),
        campfire_within_2: certainty_for_predicate(
            route_facts,
            &RouteWindowPredicate::ReachableWithin {
                subject: RouteWindowSubject::Campfire,
                nodes: 2,
            },
        ),
        campfire_within_3: certainty_for_predicate(
            route_facts,
            &RouteWindowPredicate::ReachableWithin {
                subject: RouteWindowSubject::Campfire,
                nodes: 3,
            },
        ),
        elite_present: certainty_for_predicate(
            route_facts,
            &RouteWindowPredicate::PresentInWindow {
                subject: RouteWindowSubject::Elite,
            },
        ),
        elite_bypass: certainty_for_predicate(
            route_facts,
            &RouteWindowPredicate::BypassExists {
                subject: RouteWindowSubject::Elite,
            },
        ),
        unknown_shop_opportunity: certainty_for_predicate(
            route_facts,
            &RouteWindowPredicate::UnknownOpportunity {
                subject: RouteWindowSubject::Shop,
            },
        ),
        unknown_combat_opportunity: certainty_for_predicate(
            route_facts,
            &RouteWindowPredicate::UnknownOpportunity {
                subject: RouteWindowSubject::KnownCombat,
            },
        ),
    }
}

fn certainty_for_predicate(
    route_facts: &RouteWindowFacts,
    predicate: &RouteWindowPredicate,
) -> EventModalCertainty {
    route_facts
        .facts
        .iter()
        .find(|fact| &fact.predicate == predicate)
        .map(certainty_for_fact)
        .unwrap_or(EventModalCertainty::Unknown)
}

fn certainty_for_fact(fact: &RouteWindowFact) -> EventModalCertainty {
    match (fact.modality, fact.provenance) {
        (RouteWindowModality::Must, RouteWindowProvenance::AllCoveredPaths) => {
            EventModalCertainty::Must
        }
        (RouteWindowModality::Can, RouteWindowProvenance::SomeCoveredPath) => {
            EventModalCertainty::Can
        }
        (RouteWindowModality::Cannot, RouteWindowProvenance::NoCoveredPathComplete) => {
            EventModalCertainty::Cannot
        }
        _ => EventModalCertainty::Unknown,
    }
}

fn event_hp_budget(run_state: &RunState, route: &EventRouteBudgetContext) -> EventHpBudget {
    let mut signals = Vec::new();
    if route.known_combat_before_campfire == EventModalCertainty::Must {
        signals.push(EventHpBudgetSignal::MustFightBeforeKnownCampfire);
    }
    if matches!(
        route.campfire_within_2,
        EventModalCertainty::Can | EventModalCertainty::Must
    ) {
        signals.push(EventHpBudgetSignal::CampfireReachableSoon);
    }
    if matches!(
        route.elite_present,
        EventModalCertainty::Can | EventModalCertainty::Must
    ) {
        signals.push(EventHpBudgetSignal::EliteObservedInWindow);
    }
    if !matches!(
        route.coverage_kind,
        RouteWindowCoverageKind::CompleteWithinHorizon
    ) {
        signals.push(EventHpBudgetSignal::RouteCoverageIncomplete);
    }

    let route_break_floor = 1.max(run_state.max_hp * 20 / 100);
    let mut reserve_floor = 18.max(run_state.max_hp * 35 / 100);
    if route.known_combat_before_campfire == EventModalCertainty::Must {
        reserve_floor += 5;
    }
    if route.elite_present == EventModalCertainty::Must {
        reserve_floor += 8;
    } else if route.elite_present == EventModalCertainty::Can
        && route.elite_bypass != EventModalCertainty::Can
    {
        reserve_floor += 4;
    }
    if route.campfire_within_2 == EventModalCertainty::Must
        && route.known_combat_before_campfire != EventModalCertainty::Must
    {
        reserve_floor = reserve_floor.saturating_sub(4);
    }
    reserve_floor = reserve_floor.min(run_state.max_hp.saturating_sub(1));
    let free_floor = (reserve_floor + (run_state.max_hp * 15 / 100)).min(run_state.max_hp);

    if run_state.current_hp < reserve_floor {
        signals.push(EventHpBudgetSignal::CurrentHpBelowReserve);
    }

    EventHpBudget {
        current_hp: run_state.current_hp,
        max_hp: run_state.max_hp,
        route_break_floor,
        reserve_floor,
        free_floor,
        small_loss: classify_hp_loss(
            run_state.current_hp,
            route_break_floor,
            reserve_floor,
            free_floor,
            5,
        ),
        medium_loss: classify_hp_loss(
            run_state.current_hp,
            route_break_floor,
            reserve_floor,
            free_floor,
            12,
        ),
        large_loss: classify_hp_loss(
            run_state.current_hp,
            route_break_floor,
            reserve_floor,
            free_floor,
            20,
        ),
        session_loss: classify_hp_loss(
            run_state.current_hp,
            route_break_floor,
            reserve_floor,
            free_floor,
            16,
        ),
        signals,
    }
}

fn classify_hp_loss(
    current_hp: i32,
    route_break_floor: i32,
    reserve_floor: i32,
    free_floor: i32,
    loss: i32,
) -> EventSpendClass {
    let after = current_hp.saturating_sub(loss);
    if after <= 0 || after < route_break_floor {
        EventSpendClass::RouteBreaking
    } else if after < reserve_floor {
        EventSpendClass::Reserved
    } else if after < free_floor {
        EventSpendClass::BudgetedForHighReturn
    } else {
        EventSpendClass::FreeToSpend
    }
}

fn event_gold_budget(
    run_state: &RunState,
    route: &EventRouteBudgetContext,
    deck: &EventDeckBudget,
) -> EventGoldBudget {
    let estimated_next_shop_purge_cost = estimated_shop_purge_cost(run_state);
    let has_ectoplasm = has_relic(run_state, RelicId::Ectoplasm);
    let known_shop_soon = matches!(
        route.known_shop_within_3,
        EventModalCertainty::Can | EventModalCertainty::Must
    );
    let only_unknown_shop = !known_shop_soon
        && matches!(
            route.unknown_shop_opportunity,
            EventModalCertainty::Can | EventModalCertainty::Unknown
        );
    let has_cleanup_need = deck.severe_curse_count > 0
        || deck.curse_count > 0
        || deck.starter_strike_count + deck.starter_defend_count >= 3;

    let mut signals = Vec::new();
    if has_ectoplasm {
        signals.push(EventGoldBudgetSignal::EctoplasmBlocksGoldGain);
    }
    if known_shop_soon {
        signals.push(EventGoldBudgetSignal::KnownShopReachableSoon);
    } else if only_unknown_shop {
        signals.push(EventGoldBudgetSignal::OnlyQuestionMarkShopOpportunity);
    }
    if run_state.gold >= estimated_next_shop_purge_cost {
        signals.push(EventGoldBudgetSignal::GoldAtOrAbovePurgeReserve);
    } else if run_state.gold + 25 >= estimated_next_shop_purge_cost {
        signals.push(EventGoldBudgetSignal::GoldNearPurgeReserve);
    }
    if has_cleanup_need {
        signals.push(EventGoldBudgetSignal::DeckHasCleanupNeed);
    }

    let gold_gain = if has_ectoplasm {
        EventGainClass::Blocked
    } else if known_shop_soon {
        EventGainClass::UsefulSoon
    } else if only_unknown_shop {
        EventGainClass::UnknownOpportunity
    } else {
        EventGainClass::LedgerOnly
    };

    EventGoldBudget {
        current_gold: run_state.gold,
        estimated_next_shop_purge_cost,
        gold_gain,
        spend_50: classify_gold_spend(
            run_state.gold,
            50,
            estimated_next_shop_purge_cost,
            known_shop_soon,
            has_cleanup_need,
        ),
        spend_75: classify_gold_spend(
            run_state.gold,
            75,
            estimated_next_shop_purge_cost,
            known_shop_soon,
            has_cleanup_need,
        ),
        spend_to_purge_floor: classify_gold_spend(
            run_state.gold,
            run_state
                .gold
                .saturating_sub(estimated_next_shop_purge_cost),
            estimated_next_shop_purge_cost,
            known_shop_soon,
            has_cleanup_need,
        ),
        signals,
    }
}

fn classify_gold_spend(
    current_gold: i32,
    spend: i32,
    purge_cost: i32,
    known_shop_soon: bool,
    has_cleanup_need: bool,
) -> EventSpendClass {
    if spend <= 0 {
        return EventSpendClass::FreeToSpend;
    }
    if current_gold < spend {
        return EventSpendClass::Unavailable;
    }
    let after = current_gold - spend;
    if known_shop_soon && has_cleanup_need && current_gold >= purge_cost && after < purge_cost {
        EventSpendClass::Reserved
    } else if known_shop_soon && has_cleanup_need && after + 25 >= purge_cost {
        EventSpendClass::BudgetedForHighReturn
    } else {
        EventSpendClass::FreeToSpend
    }
}

fn estimated_shop_purge_cost(run_state: &RunState) -> i32 {
    if has_relic(run_state, RelicId::SmilingMask) {
        return 50;
    }
    let mut cost = 75 + run_state.shop_purge_count.max(0) * 25;
    if has_relic(run_state, RelicId::Courier) {
        cost = (cost as f32 * 0.8).round() as i32;
    }
    if has_relic(run_state, RelicId::MembershipCard) {
        cost = (cost as f32 * 0.5).round() as i32;
    }
    cost
}

fn event_deck_budget(run_state: &RunState) -> EventDeckBudget {
    let deck_size = run_state.master_deck.len();
    let curse_count = run_state
        .master_deck
        .iter()
        .filter(|card| get_card_definition(card.id).card_type == CardType::Curse)
        .count();
    let severe_curse_count = run_state
        .master_deck
        .iter()
        .filter(|card| severe_curse(card.id))
        .count();
    let starter_strike_count = run_state
        .master_deck
        .iter()
        .filter(|card| {
            get_card_definition(card.id)
                .tags
                .contains(&CardTag::StarterStrike)
        })
        .count();
    let starter_defend_count = run_state
        .master_deck
        .iter()
        .filter(|card| {
            get_card_definition(card.id)
                .tags
                .contains(&CardTag::StarterDefend)
        })
        .count();
    let low_value_transform_target_count = starter_strike_count + starter_defend_count;
    let has_omamori = has_omamori_charge(run_state);

    let mut signals = Vec::new();
    if has_omamori {
        signals.push(EventDeckBudgetSignal::OmamoriCanNegateCurse);
    }
    if severe_curse_count > 0 {
        signals.push(EventDeckBudgetSignal::SevereCurseAlreadyPresent);
    }
    if deck_size >= 28 {
        signals.push(EventDeckBudgetSignal::HeavyDeck);
    } else if deck_size <= 14 {
        signals.push(EventDeckBudgetSignal::ThinDeckProtectCore);
    }
    if low_value_transform_target_count >= 2 {
        signals.push(EventDeckBudgetSignal::StarterTransformTargetsAvailable);
    } else {
        signals.push(EventDeckBudgetSignal::NoClearLowValueTransformTargets);
    }

    EventDeckBudget {
        deck_size,
        curse_count,
        severe_curse_count,
        starter_strike_count,
        starter_defend_count,
        low_value_transform_target_count,
        curse_intake: if has_omamori {
            EventSpendClass::FreeToSpend
        } else if severe_curse_count > 0 || curse_count >= 2 || deck_size >= 28 {
            EventSpendClass::RouteBreaking
        } else if curse_count > 0 || deck_size >= 22 {
            EventSpendClass::Reserved
        } else {
            EventSpendClass::BudgetedForHighReturn
        },
        random_transform: if low_value_transform_target_count >= 2 {
            EventSpendClass::BudgetedForHighReturn
        } else if (low_value_transform_target_count == 1 && deck_size >= 18) || deck_size <= 14 {
            EventSpendClass::Reserved
        } else {
            EventSpendClass::Unknown
        },
        filler_card_addition: if deck_size >= 28 || severe_curse_count > 0 {
            EventSpendClass::RouteBreaking
        } else if deck_size >= 22 || curse_count > 0 {
            EventSpendClass::Reserved
        } else {
            EventSpendClass::BudgetedForHighReturn
        },
        functional_card_loss: if deck_size <= 14 {
            EventSpendClass::RouteBreaking
        } else if deck_size <= 20 {
            EventSpendClass::Reserved
        } else {
            EventSpendClass::BudgetedForHighReturn
        },
        signals,
    }
}

fn event_variance_budget(
    route: &EventRouteBudgetContext,
    hp: &EventHpBudget,
    deck: &EventDeckBudget,
) -> EventVarianceBudget {
    let mut signals = Vec::new();
    if !matches!(
        route.coverage_kind,
        RouteWindowCoverageKind::CompleteWithinHorizon
    ) {
        signals.push(EventVarianceBudgetSignal::RouteCoverageIncomplete);
    }
    let hp_reserve_tight = hp.current_hp < hp.reserve_floor
        || matches!(
            hp.small_loss,
            EventSpendClass::Reserved | EventSpendClass::RouteBreaking
        );
    if hp_reserve_tight {
        signals.push(EventVarianceBudgetSignal::HpReserveTight);
    }
    let deck_pollution_tight = matches!(
        deck.curse_intake,
        EventSpendClass::Reserved | EventSpendClass::RouteBreaking
    ) || matches!(
        deck.filler_card_addition,
        EventSpendClass::Reserved | EventSpendClass::RouteBreaking
    );
    if deck_pollution_tight {
        signals.push(EventVarianceBudgetSignal::DeckPollutionReserveTight);
    }
    let no_known_recovery_soon = !matches!(
        route.campfire_within_3,
        EventModalCertainty::Can | EventModalCertainty::Must
    );
    if no_known_recovery_soon {
        signals.push(EventVarianceBudgetSignal::NoKnownRecoverySoon);
    }
    let behind_requires_high_roll = hp.current_hp < hp.route_break_floor
        && no_known_recovery_soon
        && route.known_combat_before_campfire == EventModalCertainty::Must;
    if behind_requires_high_roll {
        signals.push(EventVarianceBudgetSignal::BehindRequiresHighRoll);
    }

    let tolerance = if route.coverage_kind == RouteWindowCoverageKind::UnavailableMap {
        EventVarianceTolerance::Unknown
    } else if behind_requires_high_roll {
        EventVarianceTolerance::HighRollNeeded
    } else if hp_reserve_tight || deck_pollution_tight {
        EventVarianceTolerance::Low
    } else if route.known_combat_before_campfire == EventModalCertainty::Must
        || route.elite_present == EventModalCertainty::Must
    {
        EventVarianceTolerance::HighReturnOnly
    } else {
        EventVarianceTolerance::Normal
    };

    EventVarianceBudget { tolerance, signals }
}

fn severe_curse(card_id: CardId) -> bool {
    matches!(
        card_id,
        CardId::Normality
            | CardId::Pain
            | CardId::Regret
            | CardId::Writhe
            | CardId::Parasite
            | CardId::Decay
    )
}

fn has_relic(run_state: &RunState, relic_id: RelicId) -> bool {
    run_state
        .relics
        .iter()
        .any(|relic| relic.id == relic_id && !relic.used_up)
}

fn has_omamori_charge(run_state: &RunState) -> bool {
    run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::Omamori && !relic.used_up && relic.counter > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::route_window_facts::{build_route_window_facts, RouteWindowFactsConfig};
    use crate::content::cards::CardId;
    use crate::content::relics::RelicState;
    use crate::runtime::combat::CombatCard;
    use crate::state::map::node::{MapEdge, MapRoomNode, RoomType};

    fn node(x: i32, y: i32, class: RoomType) -> MapRoomNode {
        let mut node = MapRoomNode::new(x, y);
        node.class = Some(class);
        node
    }

    fn run_with_graph(graph: Vec<Vec<MapRoomNode>>, current_x: i32, current_y: i32) -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 50;
        run_state.max_hp = 80;
        run_state.gold = 80;
        run_state.map = crate::state::map::state::MapState::new(graph);
        run_state.map.current_x = current_x;
        run_state.map.current_y = current_y;
        run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Strike, 2),
            CombatCard::for_scenario(CardId::Defend, 3),
            CombatCard::for_scenario(CardId::Defend, 4),
            CombatCard::for_scenario(CardId::Bash, 5),
        ];
        run_state
    }

    fn budget_for(run_state: &RunState) -> EventResourceBudget {
        let route_facts = build_route_window_facts(
            run_state,
            RouteWindowFactsConfig {
                horizon_nodes: 3,
                path_budget: 64,
            },
        );
        build_event_resource_budget(run_state, &route_facts)
    }

    #[test]
    fn known_shop_soon_makes_gold_gain_liquid_and_preserves_purge_reserve() {
        let mut start = node(0, 0, RoomType::EventRoom);
        start.edges.insert(MapEdge::new(0, 0, 0, 1));
        let shop = node(0, 1, RoomType::ShopRoom);
        let run_state = run_with_graph(vec![vec![start], vec![shop]], 0, 0);

        let budget = budget_for(&run_state);

        assert_eq!(budget.gold.gold_gain, EventGainClass::UsefulSoon);
        assert_eq!(budget.gold.spend_75, EventSpendClass::Reserved);
        assert!(budget
            .gold
            .signals
            .contains(&EventGoldBudgetSignal::GoldAtOrAbovePurgeReserve));
    }

    #[test]
    fn ectoplasm_blocks_event_gold_gain() {
        let mut start = node(0, 0, RoomType::EventRoom);
        start.edges.insert(MapEdge::new(0, 0, 0, 1));
        let shop = node(0, 1, RoomType::ShopRoom);
        let mut run_state = run_with_graph(vec![vec![start], vec![shop]], 0, 0);
        run_state.relics.push(RelicState::new(RelicId::Ectoplasm));

        let budget = budget_for(&run_state);

        assert_eq!(budget.gold.gold_gain, EventGainClass::Blocked);
        assert!(budget
            .gold
            .signals
            .contains(&EventGoldBudgetSignal::EctoplasmBlocksGoldGain));
    }

    #[test]
    fn forced_combat_before_campfire_reserves_hp() {
        let mut start = node(0, 0, RoomType::EventRoom);
        start.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut combat = node(0, 1, RoomType::MonsterRoom);
        combat.edges.insert(MapEdge::new(0, 1, 0, 2));
        let fire = node(0, 2, RoomType::RestRoom);
        let mut run_state = run_with_graph(vec![vec![start], vec![combat], vec![fire]], 0, 0);
        run_state.current_hp = 32;

        let budget = budget_for(&run_state);

        assert!(budget
            .hp
            .signals
            .contains(&EventHpBudgetSignal::MustFightBeforeKnownCampfire));
        assert!(matches!(
            budget.hp.medium_loss,
            EventSpendClass::Reserved | EventSpendClass::RouteBreaking
        ));
    }

    #[test]
    fn starter_targets_make_random_transform_budgeted_not_free() {
        let start = node(0, 0, RoomType::EventRoom);
        let run_state = run_with_graph(vec![vec![start]], 0, 0);

        let budget = budget_for(&run_state);

        assert_eq!(
            budget.deck.random_transform,
            EventSpendClass::BudgetedForHighReturn
        );
        assert!(budget
            .deck
            .signals
            .contains(&EventDeckBudgetSignal::StarterTransformTargetsAvailable));
    }

    #[test]
    fn question_mark_shop_is_unknown_opportunity_not_liquid_gold() {
        let mut start = node(0, 0, RoomType::EventRoom);
        start.edges.insert(MapEdge::new(0, 0, 0, 1));
        let unknown = node(0, 1, RoomType::EventRoom);
        let run_state = run_with_graph(vec![vec![start], vec![unknown]], 0, 0);

        let budget = budget_for(&run_state);

        assert_eq!(budget.gold.gold_gain, EventGainClass::UnknownOpportunity);
        assert!(budget
            .gold
            .signals
            .contains(&EventGoldBudgetSignal::OnlyQuestionMarkShopOpportunity));
    }
}
