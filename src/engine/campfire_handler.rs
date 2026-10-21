use crate::content::relics::{RelicId, RelicState};
use crate::engine::campfire_candidates::{
    campfire_candidate_for_choice, legal_campfire_candidates,
};
use crate::state::core::{CampfireChoice, ClientInput, EngineState};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

/// Returns the Rest heal shared by execution and strategic projections.
///
/// Rest is 30% of max HP at every ascension. Ascension 5 only changes
/// post-boss healing, and Ascension 14 only changes starting max HP.
pub fn campfire_rest_heal_capacity(max_hp: i32, relics: &[RelicState]) -> i32 {
    if relics
        .iter()
        .any(|relic| relic.id == RelicId::MarkOfTheBloom)
    {
        return 0;
    }
    let base = max_hp.max(0).saturating_mul(30) / 100;
    base.saturating_add(
        if relics.iter().any(|relic| relic.id == RelicId::RegalPillow) {
            15
        } else {
            0
        },
    )
}

/// Applies the deterministic HP prefix of Rest without triggering on-rest relic rewards.
pub fn apply_campfire_rest_healing(run_state: &mut RunState) {
    run_state.current_hp +=
        campfire_rest_hp_gain(run_state.current_hp, run_state.max_hp, &run_state.relics);
}

/// Actual deterministic HP gain, clipped to the current missing HP. This is
/// only the healing prefix; on-rest rewards (e.g. Dream Catcher) are separate.
pub fn campfire_rest_hp_gain(current_hp: i32, max_hp: i32, relics: &[RelicState]) -> i32 {
    campfire_rest_heal_capacity(max_hp, relics).min(max_hp.saturating_sub(current_hp).max(0))
}

/// Campfire (Rest Site) handler.
///
/// Java behaviour reference:
///   1. On room entry → `onEnterRestRoom()` for all relics (AncientTeaSet, etc.) — done in run_loop.rs
///   2. Button init: Rest (unless CoffeeDripper), Smith (unless FusionHammer or no upgradable cards),
///      then relic options (Lift/Dig/Toke), then Recall (Act 3+ & Ruby Key missing).
///   3. Execution → per-option logic + relic callbacks (`onRest`, `onSmith`).
pub fn handle(
    engine_state: &mut EngineState,
    run_state: &mut RunState,
    input: Option<ClientInput>,
) -> bool {
    if input.is_none() && get_available_options(run_state).is_empty() {
        *engine_state = EngineState::MapNavigation;
        return true;
    }
    if let Some(ClientInput::CampfireOption(choice)) = input {
        if !campfire_choice_is_available(run_state, choice) {
            return true;
        }
        match choice {
            CampfireChoice::Rest => {
                apply_campfire_rest_healing(run_state);

                // --- onRest() relic callbacks ---
                // DreamCatcher: after resting, generate a card reward screen
                if run_state
                    .relics
                    .iter()
                    .any(|r| r.id == RelicId::DreamCatcher)
                {
                    let cards = crate::state::rewards::generator::generate_card_reward(
                        run_state,
                        crate::state::rewards::generator::adjusted_card_reward_choice_count(
                            run_state, 3,
                        ),
                        false,
                        false,
                    );
                    let mut reward_state = crate::state::rewards::RewardState::new();
                    reward_state
                        .items
                        .push(crate::state::rewards::RewardItem::Card { cards });
                    *engine_state = EngineState::RewardScreen(reward_state);
                    return true;
                }

                *engine_state = EngineState::MapNavigation;
            }

            CampfireChoice::Smith(idx) => {
                // Java: SmithOption → card upgrade on master_deck
                if idx < run_state.master_deck.len() {
                    let lineage = run_state.master_deck[idx].lineage;
                    run_state.upgrade_card_with_source(lineage, DomainEventSource::CampfireSmith);
                }
                *engine_state = EngineState::MapNavigation;
            }

            CampfireChoice::Dig => {
                // Shovel: Java → Dig grants relic via reward screen (AbstractRoom.addRelicToRewards)
                let id = run_state.random_relic();
                let mut reward_state = crate::state::rewards::RewardState::new();
                reward_state
                    .items
                    .push(crate::state::rewards::RewardItem::Relic { relic_id: id });
                *engine_state = EngineState::RewardScreen(reward_state);
            }

            CampfireChoice::Lift => {
                // Girya: increment counter (capped at 3). Strength applied at battleStart.
                for relic in run_state.relics.iter_mut() {
                    if relic.id == RelicId::Girya {
                        relic.counter = (relic.counter + 1).min(3);
                    }
                }
                *engine_state = EngineState::MapNavigation;
            }

            CampfireChoice::Toke(idx) => {
                if idx < run_state.master_deck.len() {
                    let card = &run_state.master_deck[idx];
                    if crate::state::core::master_deck_card_is_purgeable(card)
                        && !crate::state::core::master_deck_card_is_bottled(card, &run_state.relics)
                    {
                        let lineage = card.lineage;
                        run_state.remove_card_from_deck_with_source(
                            lineage,
                            DomainEventSource::CampfireToke,
                        );
                    }
                }
                *engine_state = EngineState::MapNavigation;
            }

            CampfireChoice::Recall => {
                // Ruby Key: obtained by choosing Recall instead of Resting.
                run_state.keys[0] = true; // keys[0] = Ruby Key
                *engine_state = EngineState::MapNavigation;
            }
        }
    }
    true
}

pub fn campfire_choice_is_available(run_state: &RunState, choice: CampfireChoice) -> bool {
    campfire_candidate_for_choice(run_state, choice)
        .is_some_and(|candidate| legal_campfire_candidates(run_state).contains(&candidate))
}

#[cfg(test)]
mod tests {
    use super::handle;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{CampfireChoice, ClientInput, EngineState};
    use crate::state::run::RunState;
    use crate::state::selection::{DomainEvent, DomainEventSource};

    #[test]
    fn dream_catcher_reward_respects_question_card() {
        let mut engine_state = EngineState::Campfire;
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.clear();
        run_state
            .relics
            .push(RelicState::new(RelicId::DreamCatcher));
        run_state
            .relics
            .push(RelicState::new(RelicId::QuestionCard));

        assert!(handle(
            &mut engine_state,
            &mut run_state,
            Some(ClientInput::CampfireOption(CampfireChoice::Rest))
        ));

        match engine_state {
            EngineState::RewardScreen(ref reward_state) => match &reward_state.items[0] {
                crate::state::rewards::RewardItem::Card { cards } => assert_eq!(cards.len(), 4),
                other => panic!("expected card reward, got {other:?}"),
            },
            other => panic!("expected reward screen, got {other:?}"),
        }
    }

    #[test]
    fn regal_pillow_adds_to_rest_heal_but_mark_of_bloom_blocks_it() {
        let mut engine_state = EngineState::Campfire;
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.current_hp = 20;
        run_state.max_hp = 80;
        run_state.relics.clear();
        run_state.relics.push(RelicState::new(RelicId::RegalPillow));

        assert!(handle(
            &mut engine_state,
            &mut run_state,
            Some(ClientInput::CampfireOption(CampfireChoice::Rest))
        ));
        assert_eq!(run_state.current_hp, 59);
        assert!(matches!(engine_state, EngineState::MapNavigation));

        let mut blocked_engine = EngineState::Campfire;
        let mut blocked = RunState::new(1, 0, false, "Ironclad");
        blocked.current_hp = 20;
        blocked.max_hp = 80;
        blocked.relics.clear();
        blocked.relics.push(RelicState::new(RelicId::RegalPillow));
        blocked
            .relics
            .push(RelicState::new(RelicId::MarkOfTheBloom));

        assert!(handle(
            &mut blocked_engine,
            &mut blocked,
            Some(ClientInput::CampfireOption(CampfireChoice::Rest))
        ));
        assert_eq!(blocked.current_hp, 20);
        assert!(matches!(blocked_engine, EngineState::MapNavigation));
    }

    #[test]
    fn peace_pipe_toke_option_uses_java_non_bottled_purgeable_gate() {
        let mut blocked = RunState::new(1, 0, false, "Ironclad");
        blocked.master_deck = vec![
            CombatCard::for_scenario(CardId::AscendersBane, 11),
            CombatCard::for_scenario(CardId::Defend, 12),
        ];
        blocked.relics.clear();
        blocked.relics.push(RelicState::new(RelicId::PeacePipe));
        let mut bottle = RelicState::new(RelicId::BottledLightning);
        bottle.amount = crate::test_support::scenario_lineage(12).to_runtime_i32_payload();
        blocked.relics.push(bottle);

        assert!(
            !super::get_available_options(&blocked)
                .iter()
                .any(|choice| matches!(choice, CampfireChoice::Toke(_))),
            "Java PeacePipe.addCampfireOption disables Toke when getGroupWithoutBottledCards(getPurgeableCards()) is empty"
        );

        let mut allowed = blocked;
        allowed
            .master_deck
            .push(CombatCard::for_scenario(CardId::Strike, 13));

        assert!(super::get_available_options(&allowed)
            .iter()
            .any(|choice| matches!(choice, CampfireChoice::Toke(_))));
    }

    #[test]
    fn fusion_hammer_rejects_direct_smith_input() {
        let mut engine_state = EngineState::Campfire;
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![CombatCard::for_scenario(CardId::SwordBoomerang, 10)];
        run_state
            .relics
            .push(RelicState::new(RelicId::FusionHammer));

        assert!(handle(
            &mut engine_state,
            &mut run_state,
            Some(ClientInput::CampfireOption(CampfireChoice::Smith(0)))
        ));

        assert_eq!(
            run_state.master_deck[0].upgrades, 0,
            "Fusion Hammer must veto direct Smith commands, not just hide the UI option"
        );
        assert!(
            matches!(engine_state, EngineState::Campfire),
            "invalid campfire input should leave the player at the campfire"
        );
    }

    #[test]
    fn no_usable_campfire_options_proceeds_like_java_ui() {
        let mut engine_state = EngineState::Campfire;
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.clear();
        run_state
            .relics
            .push(RelicState::new(RelicId::CoffeeDripper));
        run_state
            .relics
            .push(RelicState::new(RelicId::FusionHammer));

        assert!(handle(&mut engine_state, &mut run_state, None));

        assert!(matches!(engine_state, EngineState::MapNavigation));
    }

    #[test]
    fn recall_only_campfire_remains_a_real_choice() {
        let mut engine_state = EngineState::Campfire;
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.relics.clear();
        run_state
            .relics
            .push(RelicState::new(RelicId::CoffeeDripper));
        run_state
            .relics
            .push(RelicState::new(RelicId::FusionHammer));
        run_state.is_final_act_available = true;
        run_state.keys[0] = false;

        assert!(handle(&mut engine_state, &mut run_state, None));

        assert!(matches!(engine_state, EngineState::Campfire));
    }

    #[test]
    fn campfire_toke_rejects_unpurgeable_and_bottled_direct_indices() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::AscendersBane, 11),
            CombatCard::for_scenario(CardId::Defend, 12),
        ];
        run_state.relics.clear();
        let mut bottle = RelicState::new(RelicId::BottledLightning);
        bottle.amount = crate::test_support::scenario_lineage(12).to_runtime_i32_payload();
        run_state.relics.push(bottle);
        run_state.relics.push(RelicState::new(RelicId::PeacePipe));
        run_state.emitted_events.clear();

        let mut engine_state = EngineState::Campfire;
        assert!(handle(
            &mut engine_state,
            &mut run_state,
            Some(ClientInput::CampfireOption(CampfireChoice::Toke(1)))
        ));
        assert_eq!(run_state.master_deck.len(), 3);

        engine_state = EngineState::Campfire;
        assert!(handle(
            &mut engine_state,
            &mut run_state,
            Some(ClientInput::CampfireOption(CampfireChoice::Toke(2)))
        ));
        assert_eq!(run_state.master_deck.len(), 3);

        engine_state = EngineState::Campfire;
        assert!(handle(
            &mut engine_state,
            &mut run_state,
            Some(ClientInput::CampfireOption(CampfireChoice::Toke(0)))
        ));
        assert_eq!(
            run_state
                .master_deck
                .iter()
                .map(|card| (card.id, card.lineage))
                .collect::<Vec<_>>(),
            vec![
                (
                    CardId::AscendersBane,
                    crate::test_support::scenario_lineage(11)
                ),
                (CardId::Defend, crate::test_support::scenario_lineage(12)),
            ]
        );
        assert!(matches!(engine_state, EngineState::MapNavigation));
        assert!(run_state.emitted_events.iter().any(|event| matches!(
            event,
            DomainEvent::CardRemoved {
                card,
                source: DomainEventSource::CampfireToke,
            } if card.id == CardId::Strike && card.lineage == crate::runtime::combat::CardLineageId::constructed_scenario(10)
        )));
    }

    #[test]
    fn available_options_collapse_complete_target_sets_to_family_placeholders() {
        let mut run = RunState::new(19, 0, false, "Ironclad");
        run.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 301),
            CombatCard::for_scenario(CardId::Defend, 302),
        ];
        run.relics = vec![RelicState::new(RelicId::PeacePipe)];

        let candidates = crate::engine::campfire_candidates::legal_campfire_candidates(&run);
        assert_eq!(
            candidates
                .iter()
                .filter(|candidate| matches!(
                    candidate,
                    crate::engine::campfire_candidates::CampfireCandidate::Smith { .. }
                ))
                .count(),
            2
        );
        assert_eq!(
            candidates
                .iter()
                .filter(|candidate| matches!(
                    candidate,
                    crate::engine::campfire_candidates::CampfireCandidate::Toke { .. }
                ))
                .count(),
            2
        );
        assert_eq!(
            super::get_available_options(&run),
            vec![
                CampfireChoice::Rest,
                CampfireChoice::Smith(0),
                CampfireChoice::Toke(0),
            ]
        );
    }

    #[test]
    fn available_options_do_not_offer_smith_for_only_nonupgradable_cards() {
        let mut run = RunState::new(23, 0, false, "Ironclad");
        run.master_deck = vec![CombatCard::for_scenario(CardId::AscendersBane, 401)];

        assert_eq!(
            super::get_available_options(&run),
            vec![CampfireChoice::Rest]
        );
    }

    #[test]
    fn rest_healing_kernel_matches_rest_execution_before_reward_side_effects() {
        let mut kernel_run = RunState::new(29, 0, false, "Ironclad");
        kernel_run.current_hp = 20;
        kernel_run.relics = vec![RelicState::new(RelicId::RegalPillow)];
        let mut handler_run = kernel_run.clone();

        super::apply_campfire_rest_healing(&mut kernel_run);
        let mut engine = EngineState::Campfire;
        assert!(super::handle(
            &mut engine,
            &mut handler_run,
            Some(ClientInput::CampfireOption(CampfireChoice::Rest)),
        ));

        assert_eq!(kernel_run.current_hp, handler_run.current_hp);
        assert_eq!(kernel_run.current_hp, 59);
        assert!(matches!(engine, EngineState::MapNavigation));
    }

    #[test]
    fn a20_rest_still_heals_thirty_percent_of_max_hp() {
        let mut run = RunState::new(31, 20, false, "Ironclad");
        run.current_hp = 55;
        run.max_hp = 78;
        run.relics.clear();

        assert_eq!(super::campfire_rest_heal_capacity(78, &run.relics), 23);
        super::apply_campfire_rest_healing(&mut run);

        assert_eq!(run.current_hp, 78);
    }

    #[test]
    fn actual_rest_gain_clips_and_obeys_healing_relics() {
        use super::campfire_rest_hp_gain as gain;
        assert_eq!(gain(78, 78, &[]), 0);
        assert_eq!(gain(76, 78, &[]), 2);
        assert_eq!(gain(1, 78, &[]), 23);
        let pillow = RelicState::new(RelicId::RegalPillow);
        let bloom = RelicState::new(RelicId::MarkOfTheBloom);
        assert_eq!(gain(1, 78, &[pillow.clone()]), 38);
        assert_eq!(gain(70, 78, &[pillow.clone()]), 8);
        assert_eq!(gain(1, 78, &[bloom.clone()]), 0);
        assert_eq!(gain(1, 78, &[pillow, bloom]), 0);
    }
}

/// Dispatch card pool by player class and rarity.
pub fn card_pool_for_class(
    player_class: &str,
    rarity: crate::content::cards::CardRarity,
) -> &'static [crate::content::cards::CardId] {
    match player_class {
        "Ironclad" => crate::content::cards::ironclad_pool_for_rarity(rarity),
        "Silent" => crate::content::cards::silent_pool_for_rarity(rarity),
        "Defect" => crate::content::cards::defect_pool_for_rarity(rarity),
        "Watcher" => crate::content::cards::watcher_pool_for_rarity(rarity),
        _ => crate::content::cards::ironclad_pool_for_rarity(rarity), // fallback
    }
}

/// Reconstructs one original-game `src*CardPool` rarity segment.
///
/// `AbstractDungeon.initializeCardPools` iterates the current rarity pool and
/// copies every card with `CardGroup.addToBottom`, which inserts at index zero.
/// Source pools therefore have exactly the reverse order of current reward
/// pools; mechanisms must choose the family that their Java call site reads.
pub fn source_card_pool_for_class(
    player_class: &str,
    rarity: crate::content::cards::CardRarity,
) -> Vec<crate::content::cards::CardId> {
    card_pool_for_class(player_class, rarity)
        .iter()
        .rev()
        .copied()
        .collect()
}

pub fn nonempty_card_pool_for_class(
    player_class: &str,
    rarity: crate::content::cards::CardRarity,
) -> &'static [crate::content::cards::CardId] {
    use crate::content::cards::CardRarity;

    let fallbacks = match rarity {
        CardRarity::Rare => [CardRarity::Rare, CardRarity::Uncommon, CardRarity::Common],
        CardRarity::Uncommon => [CardRarity::Uncommon, CardRarity::Common, CardRarity::Rare],
        CardRarity::Common => [CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare],
        _ => [CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare],
    };

    for candidate_rarity in fallbacks {
        let pool = card_pool_for_class(player_class, candidate_rarity);
        if !pool.is_empty() {
            return pool;
        }
    }

    &[]
}

/// Returns the list of available campfire options for the current run state.
/// Java: CampfireUI.initializeButtons() order:
///   1. Rest (unless CoffeeDripper)
///   2. Smith (unless FusionHammer or no upgradable cards)
///   3. Relic options: Lift (Girya counter < 3), Dig (Shovel owned), Toke (PeacePipe owned)
///   4. Recall (Act 3+ AND Ruby Key missing)
pub fn get_available_options(run_state: &RunState) -> Vec<CampfireChoice> {
    let mut options = Vec::new();

    for candidate in legal_campfire_candidates(run_state) {
        let option = candidate.family_placeholder_choice();
        if !options.contains(&option) {
            options.push(option);
        }
    }

    options
}
