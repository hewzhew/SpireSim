use crate::ai::card_semantics_v1::card_owned_base_output_v1;
use crate::content::cards::{get_card_definition, CardId, CardTag, CardType};
use crate::content::relics::RelicId;
use crate::state::run::RunState;

use super::coverage::threat_coverage_from_run_state_v1;
use super::snapshot::build_run_strategy_snapshot_v1;
use super::snapshot_v2::build_run_strategy_snapshot_v2_from_v1_with_threat;
use super::threat::threat_profile_from_run_state_v1;
use super::types::{
    RunStrategySnapshotV2, StrategyDeckFactsV1, StrategyResourceFactsV2, StrategyRouteFutureV1,
};

pub fn build_run_strategy_snapshot_from_run_state_v2(
    run_state: &RunState,
) -> RunStrategySnapshotV2 {
    build_run_strategy_snapshot_from_run_state_with_route_v2(
        run_state,
        route_future_from_run_state_v1(run_state),
    )
}

pub fn build_run_strategy_snapshot_from_run_state_with_route_v2(
    run_state: &RunState,
    route: Option<StrategyRouteFutureV1>,
) -> RunStrategySnapshotV2 {
    let v1 = build_run_strategy_snapshot_v1(deck_facts_from_run_state_v1(run_state), route);
    let mut snapshot = build_run_strategy_snapshot_v2_from_v1_with_threat(
        v1,
        resource_facts_from_run_state_v2(run_state),
        threat_profile_from_run_state_v1(run_state),
    );
    snapshot.threat_coverage = threat_coverage_from_run_state_v1(run_state, &snapshot.threats);
    snapshot
}

fn deck_facts_from_run_state_v1(run_state: &RunState) -> StrategyDeckFactsV1 {
    let mut facts = StrategyDeckFactsV1 {
        deck_size: run_state.master_deck.len(),
        attacks: 0,
        skills: 0,
        powers: 0,
        starter_strikes: 0,
        starter_defends: 0,
        strength_sources: 0,
        strength_payoffs: 0,
        weak_sources: 0,
        draw_sources: 0,
        energy_sources: 0,
        vulnerable_sources: 0,
        route_upgrade_payoffs: 0,
        important_cards_unupgraded: 0,
        exhaust_generators: 0,
        exhaust_payoffs: 0,
        status_generators: 0,
        status_payoffs: 0,
        block_retention_sources: 0,
        block_payoffs: 0,
        block_multipliers: 0,
        total_attack_damage: 0,
        total_block: 0,
    };

    for card in &run_state.master_deck {
        let def = get_card_definition(card.id);
        match def.card_type {
            CardType::Attack => facts.attacks = facts.attacks.saturating_add(1),
            CardType::Skill => facts.skills = facts.skills.saturating_add(1),
            CardType::Power => facts.powers = facts.powers.saturating_add(1),
            CardType::Curse | CardType::Status => {}
        }

        if def.tags.contains(&CardTag::StarterStrike) {
            facts.starter_strikes = facts.starter_strikes.saturating_add(1);
        }
        if def.tags.contains(&CardTag::StarterDefend) {
            facts.starter_defends = facts.starter_defends.saturating_add(1);
        }

        let output = card_owned_base_output_v1(card);
        facts.total_attack_damage = facts.total_attack_damage.saturating_add(
            output
                .damage
                .unconditional_targeted_base_total()
                .unwrap_or(0),
        );
        facts.total_block = facts
            .total_block
            .saturating_add(output.unconditional_block.unwrap_or(0));

        if card.upgrades == 0 && matches!(card.id, CardId::Bash) {
            facts.important_cards_unupgraded = facts.important_cards_unupgraded.saturating_add(1);
        }

        add_card_package_facts(card.id, card.upgrades, &mut facts);
    }

    facts
}

fn resource_facts_from_run_state_v2(run_state: &RunState) -> StrategyResourceFactsV2 {
    let potion_slots = run_state.potions.len();
    let potion_count = run_state
        .potions
        .iter()
        .filter(|slot| slot.is_some())
        .count();
    let empty_potion_slots = potion_slots.saturating_sub(potion_count);
    let estimated_purge_cost = estimated_purge_cost(run_state);
    let mut curses = 0usize;
    let mut removable_curses = 0usize;
    let mut starter_cards = 0usize;

    for card in &run_state.master_deck {
        let def = get_card_definition(card.id);
        if def.card_type == CardType::Curse {
            curses += 1;
            if crate::state::core::master_deck_card_is_purgeable(card)
                && !crate::state::core::master_deck_card_is_bottled(card, &run_state.relics)
            {
                removable_curses += 1;
            }
        }
        if def.tags.contains(&CardTag::StarterStrike) || def.tags.contains(&CardTag::StarterDefend)
        {
            starter_cards += 1;
        }
    }
    let anticipated_next_combat_start_heal = anticipated_next_combat_start_heal(run_state);
    let effective_next_combat_hp = run_state
        .current_hp
        .saturating_add(anticipated_next_combat_start_heal)
        .min(run_state.max_hp)
        .max(0);

    StrategyResourceFactsV2 {
        current_hp: run_state.current_hp,
        max_hp: run_state.max_hp,
        anticipated_next_combat_start_heal,
        effective_next_combat_hp,
        gold: run_state.gold,
        estimated_purge_cost,
        potion_slots,
        potion_count,
        empty_potion_slots,
        curses,
        removable_curses,
        starter_cards,
        relic_constraints: relic_constraints(run_state),
    }
}

fn anticipated_next_combat_start_heal(run_state: &RunState) -> i32 {
    if !run_state.map.boss_node_available_now() {
        return 0;
    }
    if run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::MarkOfTheBloom)
    {
        return 0;
    }
    if run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::Pantograph)
    {
        return 25;
    }
    0
}

fn estimated_purge_cost(run_state: &RunState) -> i32 {
    if run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::SmilingMask)
    {
        50
    } else {
        75 + run_state.shop_purge_count.saturating_mul(25)
    }
}

fn relic_constraints(run_state: &RunState) -> Vec<String> {
    let mut constraints = Vec::new();
    for relic in &run_state.relics {
        match relic.id {
            RelicId::Sozu => constraints.push("Sozu: cannot obtain potions".to_string()),
            RelicId::Ectoplasm => constraints.push("Ectoplasm: cannot gain gold".to_string()),
            RelicId::CoffeeDripper => {
                constraints.push("Coffee Dripper: cannot rest at campfires".to_string())
            }
            RelicId::MarkOfTheBloom => {
                constraints.push("Mark of the Bloom: healing is disabled".to_string())
            }
            _ => {}
        }
    }
    constraints
}

fn add_card_package_facts(card_id: CardId, upgrades: u8, facts: &mut StrategyDeckFactsV1) {
    if matches!(
        card_id,
        CardId::Inflame | CardId::SpotWeakness | CardId::DemonForm
    ) {
        facts.strength_sources = facts.strength_sources.saturating_add(1);
    }
    if matches!(
        card_id,
        CardId::HeavyBlade | CardId::LimitBreak | CardId::Reaper
    ) {
        facts.strength_payoffs = facts.strength_payoffs.saturating_add(1);
    }
    if matches!(
        card_id,
        CardId::Clothesline
            | CardId::Uppercut
            | CardId::Shockwave
            | CardId::Blind
            | CardId::SuckerPunch
            | CardId::GoForTheEyes
    ) {
        facts.weak_sources = facts.weak_sources.saturating_add(1);
    }
    if matches!(
        card_id,
        CardId::Bash
            | CardId::Uppercut
            | CardId::Shockwave
            | CardId::Terror
            | CardId::ThunderClap
            | CardId::Trip
            | CardId::BeamCell
    ) {
        facts.vulnerable_sources = facts.vulnerable_sources.saturating_add(1);
    }
    if matches!(
        card_id,
        CardId::PommelStrike
            | CardId::ShrugItOff
            | CardId::Finesse
            | CardId::FlashOfSteel
            | CardId::DeepBreath
            | CardId::QuickSlash
            | CardId::SweepingBeam
            | CardId::BurningPact
            | CardId::BattleTrance
            | CardId::Offering
            | CardId::MasterOfStrategy
            | CardId::Acrobatics
            | CardId::Backflip
            | CardId::Skim
            | CardId::WheelKick
    ) {
        facts.draw_sources = facts.draw_sources.saturating_add(1);
    }
    if card_id == CardId::Warcry && upgrades > 0 {
        facts.draw_sources = facts.draw_sources.saturating_add(1);
    }
    if matches!(
        card_id,
        CardId::Offering | CardId::SeeingRed | CardId::Bloodletting | CardId::Turbo
    ) {
        facts.energy_sources = facts.energy_sources.saturating_add(1);
    }
    if matches!(card_id, CardId::SearingBlow) {
        facts.route_upgrade_payoffs = facts.route_upgrade_payoffs.saturating_add(1);
    }
    if matches!(
        card_id,
        CardId::BurningPact
            | CardId::Corruption
            | CardId::TrueGrit
            | CardId::SecondWind
            | CardId::SeverSoul
            | CardId::FiendFire
            | CardId::Recycle
    ) {
        facts.exhaust_generators = facts.exhaust_generators.saturating_add(1);
    }
    if matches!(card_id, CardId::FeelNoPain | CardId::DarkEmbrace) {
        facts.exhaust_payoffs = facts.exhaust_payoffs.saturating_add(1);
    }
    if matches!(
        card_id,
        CardId::WildStrike | CardId::RecklessCharge | CardId::PowerThrough | CardId::Immolate
    ) {
        facts.status_generators = facts.status_generators.saturating_add(1);
    }
    if matches!(card_id, CardId::Evolve | CardId::FireBreathing) {
        facts.status_payoffs = facts.status_payoffs.saturating_add(1);
    }
    if matches!(card_id, CardId::Barricade) {
        facts.block_retention_sources = facts.block_retention_sources.saturating_add(1);
    }
    if matches!(card_id, CardId::BodySlam | CardId::Juggernaut) {
        facts.block_payoffs = facts.block_payoffs.saturating_add(1);
    }
    if matches!(card_id, CardId::Entrench) {
        facts.block_multipliers = facts.block_multipliers.saturating_add(1);
    }
}

fn route_future_from_run_state_v1(run_state: &RunState) -> Option<StrategyRouteFutureV1> {
    let x = run_state.map.current_x;
    let y = run_state.map.current_y;
    if x < 0 || y < 0 {
        return None;
    }

    let summary =
        crate::ai::route_window_facts::summarize_route_from_target(run_state, x, y, 2_000);
    if summary.path_count == 0 {
        return None;
    }

    let hp_ratio = if run_state.max_hp > 0 {
        run_state.current_hp as f32 / run_state.max_hp as f32
    } else {
        0.0
    };
    let missing_hp_ratio = if run_state.max_hp > 0 {
        (run_state.max_hp - run_state.current_hp).max(0) as f32 / run_state.max_hp as f32
    } else {
        1.0
    };
    let avoid_damage = if hp_ratio <= 0.30 {
        0.85
    } else if hp_ratio <= 0.45 {
        0.60
    } else if hp_ratio <= 0.65 {
        0.35
    } else {
        0.05
    };

    Some(StrategyRouteFutureV1 {
        min_fires: summary.min_campfires,
        max_fires: summary.max_campfires,
        first_fire_floor: summary.first_campfire_floor,
        max_early_pressure: summary.max_early_pressure,
        need_heal: missing_hp_ratio.clamp(0.0, 1.0),
        avoid_damage,
        first_elite_forced: summary.first_elite.forced,
        max_hallways_before_first_elite: summary.first_elite.max_hallway_fights_before,
        can_bail_to_rest_before_first_elite: summary.first_elite.can_bail_to_rest_before,
        can_bail_to_shop_before_first_elite: summary.first_elite.can_bail_to_shop_before,
    })
}
