use serde::Serialize;
use sts_oracle_runtime::ai::combat_search_v2::{
    replay_combat_search_witness_line_v0, run_combat_turn_pool_opening_report_v0,
    CombatSearchV2Config, CombatSearchV2WitnessReplay, CombatTurnPoolOpeningReport,
    SearchTerminalLabel,
};
use sts_oracle_runtime::content::cards::{self, CardId, CardType};
use sts_oracle_runtime::content::monsters::EnemyId;
use sts_oracle_runtime::content::powers::{store, PowerId};
use sts_oracle_runtime::eval::combat_case::CombatCase;
use sts_oracle_runtime::runtime::combat::{
    CardLineageId, CardPileView, CombatCard, CombatState, Power,
};
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::state::core::ClientInput;

use super::options::ReviewOptions;
use super::quality_lanes::witness_line_from_trajectory;
use super::search_runner::{review_all_potions_profile, run_config_search};
use super::search_types::SearchReview;

#[derive(Serialize)]
pub(crate) struct PowerSetupCounterfactualProbe {
    schema: &'static str,
    contract: &'static str,
    power_cards: Vec<PowerSetupCard>,
    variants: Vec<PowerSetupVariant>,
    conclusion: &'static str,
}

#[derive(Clone, Serialize)]
struct PowerSetupCard {
    #[serde(skip)]
    card_id: CardId,
    card: String,
    lineage: CardLineageId,
    upgrades: u8,
    original_zone: &'static str,
}

#[derive(Serialize)]
struct PowerSetupVariant {
    label: &'static str,
    semantics: &'static str,
    setup: Option<PowerSetupSnapshot>,
    setup_failure: Option<String>,
    whole_combat_search: Option<SearchReview>,
    whole_combat_win_replay: Option<CombatSearchV2WitnessReplay>,
    turn_pool: Option<CombatTurnPoolOpeningReport>,
    complete_win_found: bool,
}

#[derive(Serialize)]
struct PowerSetupSnapshot {
    applied_cards: Vec<PowerSetupCard>,
    player_hp: i32,
    energy: u8,
    cards_played_this_turn: u8,
    player_vulnerable: i32,
    player_berserk: i32,
    player_feel_no_pain: i32,
    awakened_one_strength: Option<i32>,
    hand_count: usize,
    draw_count: usize,
    discard_count: usize,
    exhaust_count: usize,
}

#[derive(Clone, Copy)]
enum PowerSetupSemantics {
    FreePlay,
    FreePlayFeelNoPain,
    OptimisticPreinstalled,
}

impl PowerSetupSemantics {
    fn label(self) -> &'static str {
        match self {
            Self::FreePlay => "free_play_all_powers",
            Self::FreePlayFeelNoPain => "free_play_feel_no_pain_only",
            Self::OptimisticPreinstalled => "optimistic_preinstalled_all_powers",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::FreePlay => {
                "move every active-zone Power card to hand and play it for zero energy through normal engine semantics; Curiosity, Berserk Vulnerable, card-play counters, and card/relic triggers remain"
            }
            Self::FreePlayFeelNoPain => {
                "move only active-zone Feel No Pain cards to hand and play them for zero energy through normal engine semantics; Curiosity remains, while Berserk stays in the combat zones for search to time or skip"
            }
            Self::OptimisticPreinstalled => {
                "start from the free-play result, restore original enemy powers and player Vulnerable/card-play counters, then grant current-turn Berserk energy; this is an intentionally optimistic deck upper-bound probe"
            }
        }
    }

    fn applies(self, card: &PowerSetupCard) -> bool {
        match self {
            Self::FreePlay | Self::OptimisticPreinstalled => true,
            Self::FreePlayFeelNoPain => card.card_id == CardId::FeelNoPain,
        }
    }
}

pub(super) fn run_power_setup_counterfactual(
    options: &ReviewOptions,
    case: &CombatCase,
) -> PowerSetupCounterfactualProbe {
    let power_cards = collect_power_cards(&case.core.position.combat);
    let semantics = if options.power_setup_optimistic_only {
        vec![PowerSetupSemantics::OptimisticPreinstalled]
    } else {
        vec![
            PowerSetupSemantics::FreePlay,
            PowerSetupSemantics::FreePlayFeelNoPain,
            PowerSetupSemantics::OptimisticPreinstalled,
        ]
    };
    let variants = semantics
        .into_iter()
        .map(|semantics| run_variant(options, case, &power_cards, semantics))
        .collect::<Vec<_>>();
    let any_win = variants.iter().any(|variant| variant.complete_win_found);
    let all_setups_failed = variants.iter().all(|variant| variant.setup.is_none());

    PowerSetupCounterfactualProbe {
        schema: "power_setup_counterfactual_v0",
        contract: "review_only_exact_state_power_setup_counterfactual_same_rng_and_remaining_card_order_no_runner_policy_change_bounded_search_nonwin_is_not_impossibility_proof",
        power_cards,
        variants,
        conclusion: if any_win {
            "counterfactual_win_found_deck_has_a_winning_setup_under_at_least_one_bounded_probe"
        } else if all_setups_failed {
            "counterfactual_setup_failed_no_combat_claim"
        } else {
            "no_counterfactual_win_found_under_budget_strong_deck_gap_evidence_not_impossibility_proof"
        },
    }
}

fn run_variant(
    options: &ReviewOptions,
    original: &CombatCase,
    power_cards: &[PowerSetupCard],
    semantics: PowerSetupSemantics,
) -> PowerSetupVariant {
    let (case, setup) = match transform_case(original, power_cards, semantics) {
        Ok(transformed) => transformed,
        Err(error) => {
            return PowerSetupVariant {
                label: semantics.label(),
                semantics: semantics.description(),
                setup: None,
                setup_failure: Some(error),
                whole_combat_search: None,
                whole_combat_win_replay: None,
                turn_pool: None,
                complete_win_found: false,
            };
        }
    };

    let profile = review_all_potions_profile(
        semantics.label(),
        options.slow_nodes,
        options.slow_ms,
        options,
    );
    let config = existential_search_config(
        profile.to_config(),
        case.core.position.combat.entities.player.current_hp,
    );
    let (whole_combat_search, report) = run_config_search(
        semantics.label(),
        &case,
        config.clone(),
        options.action_preview_limit,
    );
    let whole_combat_win_replay = report.best_win_trajectory.as_ref().map(|trajectory| {
        let witness = witness_line_from_trajectory(semantics.label(), trajectory);
        replay_combat_search_witness_line_v0(&case.core.position, &witness)
    });
    let replayed_whole_combat_win = whole_combat_win_replay
        .as_ref()
        .is_some_and(|replay| replay.terminal == CombatTerminal::Win);
    // Once ordinary-engine replay has proved a complete win, the fallback
    // turn-pool cannot add evidence for this existential probe.
    let turn_pool = should_run_fallback_turn_pool(replayed_whole_combat_win).then(|| {
        run_combat_turn_pool_opening_report_v0(
            &case.core.position,
            options.slow_ms,
            40,
            Some(&config),
        )
    });
    let turn_pool_win = turn_pool.as_ref().is_some_and(|turn_pool| {
        turn_pool
            .lanes
            .iter()
            .any(|line| line.terminal == SearchTerminalLabel::Win)
    });
    let complete_win_found =
        (whole_combat_search.complete_win && replayed_whole_combat_win) || turn_pool_win;

    PowerSetupVariant {
        label: semantics.label(),
        semantics: semantics.description(),
        setup: Some(setup),
        setup_failure: None,
        whole_combat_search: Some(whole_combat_search),
        whole_combat_win_replay,
        turn_pool,
        complete_win_found,
    }
}

fn existential_search_config(
    mut config: CombatSearchV2Config,
    initial_hp: i32,
) -> CombatSearchV2Config {
    // This probe asks whether the transformed exact state has any replayable
    // complete win. Continuing after proof only optimizes final HP and makes
    // calibration needlessly expensive.
    let _ = initial_hp;
    config.satisfaction =
        sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2Satisfaction::FirstCompleteWin;
    config
}

fn should_run_fallback_turn_pool(replayed_whole_combat_win: bool) -> bool {
    !replayed_whole_combat_win
}

fn transform_case(
    original: &CombatCase,
    power_cards: &[PowerSetupCard],
    semantics: PowerSetupSemantics,
) -> Result<(CombatCase, PowerSetupSnapshot), String> {
    if power_cards.is_empty() {
        return Err("no active-zone Power cards found".to_string());
    }
    if !matches!(
        original.core.position.engine,
        sts_oracle_runtime::state::core::EngineState::CombatPlayerTurn
    ) {
        return Err("power setup counterfactual requires a player-turn combat root".to_string());
    }

    let applied_cards = power_cards
        .iter()
        .filter(|power| semantics.applies(power))
        .cloned()
        .collect::<Vec<_>>();
    if applied_cards.is_empty() {
        return Err(format!(
            "no active-zone Power cards matched {}",
            semantics.label()
        ));
    }

    let mut case = original.clone();
    for power in &applied_cards {
        free_play_power(&mut case.core.position, power)?;
    }
    if matches!(semantics, PowerSetupSemantics::OptimisticPreinstalled) {
        remove_free_play_costs(original, &mut case);
    }
    case.refresh_derived_summaries_and_clear_production_context();
    let setup = setup_snapshot(&case.core.position.combat, applied_cards);
    Ok((case, setup))
}

fn free_play_power(position: &mut CombatPosition, evidence: &PowerSetupCard) -> Result<(), String> {
    let mut card = position
        .combat
        .take_card_by_lineage_from_anywhere(evidence.lineage)
        .ok_or_else(|| {
            format!(
                "Power card lineage {} disappeared before setup",
                evidence.lineage
            )
        })?;
    if cards::get_card_definition(card.id).card_type != CardType::Power {
        return Err(format!(
            "card lineage {} is no longer a Power card",
            evidence.lineage
        ));
    }
    card.set_cost_for_turn_java(0);
    position.combat.zones.hand.push(card);
    let card_index = position.combat.zones.hand.len() - 1;
    let input = ClientInput::PlayCard {
        card_index,
        target: None,
    };
    let stepper = EngineCombatStepper;
    if !stepper.is_legal_action(position, &input) {
        return Err(format!(
            "free Power play was not legal for {}",
            evidence.card
        ));
    }
    let step = stepper.apply_to_stable(
        position,
        input,
        CombatStepLimits {
            max_engine_steps: 500,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err(format!(
            "free Power play did not reach a stable boundary for {}",
            evidence.card
        ));
    }
    if !step.alive || step.terminal == CombatTerminal::Loss {
        return Err(format!(
            "free Power play killed the player while applying {}",
            evidence.card
        ));
    }
    *position = step.position;
    Ok(())
}

fn remove_free_play_costs(original: &CombatCase, transformed: &mut CombatCase) {
    restore_monster_powers(
        &original.core.position.combat,
        &mut transformed.core.position.combat,
    );
    restore_player_vulnerable(
        &original.core.position.combat,
        &mut transformed.core.position.combat,
    );
    transformed.core.position.combat.turn.counters =
        original.core.position.combat.turn.counters.clone();
    let berserk = store::power_amount(
        &transformed.core.position.combat,
        transformed.core.position.combat.entities.player.id,
        PowerId::Berserk,
    )
    .max(0);
    transformed.core.position.combat.turn.energy = original
        .core
        .position
        .combat
        .turn
        .energy
        .saturating_add(berserk.min(u8::MAX as i32) as u8);
}

fn restore_monster_powers(original: &CombatState, transformed: &mut CombatState) {
    let monster_ids = transformed
        .entities
        .monsters
        .iter()
        .map(|monster| monster.id)
        .collect::<Vec<_>>();
    for monster_id in monster_ids {
        store::set_powers_for(
            transformed,
            monster_id,
            store::powers_snapshot_for(original, monster_id),
        );
    }
}

fn restore_player_vulnerable(original: &CombatState, transformed: &mut CombatState) {
    let player_id = transformed.entities.player.id;
    let original_vulnerable = store::powers_snapshot_for(original, player_id)
        .into_iter()
        .filter(|power| power.power_type == PowerId::Vulnerable)
        .collect::<Vec<Power>>();
    let mut powers = store::powers_snapshot_for(transformed, player_id)
        .into_iter()
        .filter(|power| power.power_type != PowerId::Vulnerable)
        .collect::<Vec<_>>();
    powers.extend(original_vulnerable);
    store::set_powers_for(transformed, player_id, powers);
}

fn collect_power_cards(combat: &CombatState) -> Vec<PowerSetupCard> {
    let zones = [
        (
            "hand",
            CardPileView::Contiguous(combat.zones.hand.as_slice()),
        ),
        (
            "draw",
            CardPileView::Contiguous(combat.zones.draw_pile.as_ref()),
        ),
        ("discard", CardPileView::Discard(&combat.zones.discard_pile)),
    ];
    zones
        .into_iter()
        .flat_map(|(zone, cards)| {
            cards
                .iter()
                .filter(|&card| cards::get_card_definition(card.id).card_type == CardType::Power)
                .map(|card| power_card_evidence(card, zone))
        })
        .collect()
}

fn power_card_evidence(card: &CombatCard, original_zone: &'static str) -> PowerSetupCard {
    let definition = cards::get_card_definition(card.id);
    PowerSetupCard {
        card_id: card.id,
        card: format!("{}+{}", definition.name, card.upgrades),
        lineage: card.lineage,
        upgrades: card.upgrades,
        original_zone,
    }
}

fn setup_snapshot(combat: &CombatState, applied_cards: Vec<PowerSetupCard>) -> PowerSetupSnapshot {
    let player_id = combat.entities.player.id;
    let awakened_one_strength = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.enemy_id == EnemyId::AwakenedOne)
        .map(|monster| store::power_amount(combat, monster.id, PowerId::Strength));
    PowerSetupSnapshot {
        applied_cards,
        player_hp: combat.entities.player.current_hp,
        energy: combat.turn.energy,
        cards_played_this_turn: combat.turn.counters.cards_played_this_turn,
        player_vulnerable: store::power_amount(combat, player_id, PowerId::Vulnerable),
        player_berserk: store::power_amount(combat, player_id, PowerId::Berserk),
        player_feel_no_pain: store::power_amount(combat, player_id, PowerId::FeelNoPain),
        awakened_one_strength,
        hand_count: combat.zones.hand.len(),
        draw_count: combat.zones.draw_pile.len(),
        discard_count: combat.zones.discard_pile.len(),
        exhaust_count: combat.zones.exhaust_pile.len(),
    }
}

#[cfg(test)]
mod tests {
    use sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2Config;
    use sts_oracle_runtime::content::cards::CardId;
    use sts_oracle_runtime::content::monsters::EnemyId;
    use sts_oracle_runtime::content::powers::{store, PowerId};
    use sts_oracle_runtime::runtime::combat::{CombatCard, Power, PowerPayload};
    use sts_oracle_runtime::sim::combat::CombatPosition;
    use sts_oracle_runtime::state::core::EngineState;
    use sts_oracle_runtime::test_support::{blank_test_combat, test_monster};

    use super::{
        collect_power_cards, existential_search_config, free_play_power, restore_monster_powers,
        restore_player_vulnerable, should_run_fallback_turn_pool, PowerSetupSemantics,
    };

    fn awakened_power_fixture() -> CombatPosition {
        let mut combat = blank_test_combat();
        let mut awakened = test_monster(EnemyId::AwakenedOne);
        awakened.awakened_one.form1 = true;
        awakened.awakened_one.protocol_seeded = true;
        combat.entities.monsters = vec![awakened];
        combat.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::Curiosity,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Berserk, 10)];
        let mut first_fnp = CombatCard::for_scenario(CardId::FeelNoPain, 11);
        first_fnp.upgrades = 1;
        let mut second_fnp = CombatCard::for_scenario(CardId::FeelNoPain, 12);
        second_fnp.upgrades = 1;
        combat.zones.draw_pile = (vec![first_fnp, second_fnp]).into();
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }

    #[test]
    fn power_setup_search_stops_after_first_replayable_win() {
        let config = existential_search_config(CombatSearchV2Config::default(), 87);

        assert_eq!(
            config.satisfaction,
            sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2Satisfaction::FirstCompleteWin
        );
    }

    #[test]
    fn replayed_win_skips_redundant_turn_pool() {
        assert!(!should_run_fallback_turn_pool(true));
        assert!(should_run_fallback_turn_pool(false));
    }

    #[test]
    fn free_play_all_powers_preserves_curiosity_and_berserk_costs() {
        let mut position = awakened_power_fixture();
        let powers = collect_power_cards(&position.combat);
        for power in &powers {
            free_play_power(&mut position, power).expect("free Power play should resolve");
        }

        assert_eq!(powers.len(), 3);
        assert_eq!(
            store::power_amount(&position.combat, 0, PowerId::Berserk),
            1
        );
        assert_eq!(
            store::power_amount(&position.combat, 0, PowerId::FeelNoPain),
            8
        );
        assert_eq!(
            store::power_amount(&position.combat, 0, PowerId::Vulnerable),
            2
        );
        assert_eq!(
            store::power_amount(&position.combat, 1, PowerId::Strength),
            3
        );
        assert_eq!(position.combat.turn.energy, 3);
        assert_eq!(position.combat.turn.counters.cards_played_this_turn, 3);
    }

    #[test]
    fn optimistic_restore_removes_curiosity_strength_and_berserk_vulnerable() {
        let original = awakened_power_fixture();
        let mut position = original.clone();
        let powers = collect_power_cards(&position.combat);
        for power in &powers {
            free_play_power(&mut position, power).expect("free Power play should resolve");
        }
        restore_monster_powers(&original.combat, &mut position.combat);
        restore_player_vulnerable(&original.combat, &mut position.combat);

        assert_eq!(
            store::power_amount(&position.combat, 1, PowerId::Strength),
            0
        );
        assert_eq!(
            store::power_amount(&position.combat, 1, PowerId::Curiosity),
            1
        );
        assert_eq!(
            store::power_amount(&position.combat, 0, PowerId::Vulnerable),
            0
        );
        assert_eq!(
            store::power_amount(&position.combat, 0, PowerId::FeelNoPain),
            8
        );
        assert_eq!(
            store::power_amount(&position.combat, 0, PowerId::Berserk),
            1
        );
    }

    #[test]
    fn feel_no_pain_only_setup_keeps_berserk_unplayed() {
        let mut position = awakened_power_fixture();
        let powers = collect_power_cards(&position.combat);
        let selected = powers
            .iter()
            .filter(|power| PowerSetupSemantics::FreePlayFeelNoPain.applies(power))
            .collect::<Vec<_>>();
        for power in selected {
            free_play_power(&mut position, power).expect("free Feel No Pain play should resolve");
        }

        assert_eq!(
            store::power_amount(&position.combat, 0, PowerId::FeelNoPain),
            8
        );
        assert_eq!(
            store::power_amount(&position.combat, 0, PowerId::Berserk),
            0
        );
        assert_eq!(
            store::power_amount(&position.combat, 0, PowerId::Vulnerable),
            0
        );
        assert_eq!(
            store::power_amount(&position.combat, 1, PowerId::Strength),
            2
        );
        assert!(position
            .combat
            .zones
            .hand
            .iter()
            .any(|card| card.id == CardId::Berserk));
        assert_eq!(position.combat.turn.energy, 3);
        assert_eq!(position.combat.turn.counters.cards_played_this_turn, 2);
    }
}
