use crate::engine::targeting;
use crate::runtime::combat::CombatState;
use crate::state::core::ClientInput;
use crate::state::EngineState;

/// Returns only the finite, explicit actions at this engine boundary.
///
/// Combinatorial Hand/Grid/Scry submissions live in the typed action surface;
/// this function must stay linear in the visible combat state.
pub fn engine_atomic_actions(engine: &EngineState, combat: &CombatState) -> Vec<ClientInput> {
    let mut moves = Vec::new();

    match engine {
        EngineState::CombatPlayerTurn => {
            // Escape is an automatic transition, never another player turn.
            if combat.turn.counters.player_escaping {
                return moves;
            }
            moves.push(ClientInput::EndTurn);
            for (potion_index, maybe_potion) in combat.entities.potions.iter().enumerate() {
                let Some(potion) = maybe_potion.as_ref() else {
                    continue;
                };
                if potion.can_discard {
                    moves.push(ClientInput::DiscardPotion(potion_index));
                }
                if !crate::content::potions::potion_can_use_in_combat_like_java(potion, combat) {
                    continue;
                }
                if let Some(validation) =
                    targeting::validation_for_potion_target(potion.requires_target)
                {
                    for target in targeting::candidate_targets(combat, validation) {
                        moves.push(ClientInput::UsePotion {
                            potion_index,
                            target: Some(target),
                        });
                    }
                } else {
                    moves.push(ClientInput::UsePotion {
                        potion_index,
                        target: None,
                    });
                }
            }

            for (i, card) in combat.zones.hand.iter().enumerate() {
                if crate::content::cards::can_play_card(card, combat).is_ok() {
                    let target_type = crate::content::cards::effective_target(card);
                    if let Some(validation) = targeting::validation_for_card_target(target_type) {
                        for target in targeting::candidate_targets(combat, validation) {
                            moves.push(ClientInput::PlayCard {
                                card_index: i,
                                target: Some(target),
                            });
                        }
                    } else {
                        moves.push(ClientInput::PlayCard {
                            card_index: i,
                            target: None,
                        });
                    }
                }
            }
        }
        EngineState::PendingChoice(_) => {
            return crate::sim::combat_action_surface::combat_legal_action_surface_v2(
                engine, combat,
            )
            .atomic_actions;
        }
        _ => {
            moves.push(ClientInput::Proceed);
        }
    }

    moves
}

/// Checks membership without requiring a complete eager candidate list.
///
/// Pending-choice membership is validated from the frozen domain state rather
/// than by pretending the atomic action list is the complete input language.
pub fn is_legal_move(engine: &EngineState, combat: &CombatState, input: &ClientInput) -> bool {
    let EngineState::PendingChoice(choice) = engine else {
        return engine_atomic_actions(engine, combat).contains(input);
    };
    crate::sim::combat_action_surface::pending_choice_input_is_legal(choice, combat, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::content::potions::{Potion, PotionId};
    use crate::sim::combat::{CombatPosition, CombatStepper, EngineCombatStepper};
    use crate::state::core::PendingChoice;
    use crate::state::selection::{SelectionResolution, SelectionScope};
    use crate::test_support::{blank_test_combat, test_monster};

    fn build_fixture_combat() -> CombatState {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        combat
    }

    #[test]
    fn engine_atomic_actions_skip_unusable_potions() {
        let mut combat = build_fixture_combat();
        combat.entities.potions = vec![
            Some(Potion::with_affordance_truth(
                PotionId::FairyPotion,
                1,
                false,
                true,
                false,
            )),
            None,
            None,
        ];
        let inputs = engine_atomic_actions(&EngineState::CombatPlayerTurn, &combat);
        assert!(
            !inputs
                .iter()
                .any(|input| matches!(input, ClientInput::UsePotion { .. })),
            "engine-local enumeration should not emit can_use=false potion actions"
        );
        assert!(
            inputs
                .iter()
                .any(|input| matches!(input, ClientInput::DiscardPotion(0))),
            "Java PotionPopUp allows discarding an owned potion through canDiscard even when canUse is false"
        );
        assert!(
            !inputs
                .iter()
                .any(|input| matches!(input, ClientInput::DiscardPotion(1 | 2))),
            "empty potion slots are not discardable actions"
        );
    }

    #[test]
    fn engine_atomic_actions_skip_passive_fairy_potion_even_if_local_affordance_is_stale() {
        let mut combat = build_fixture_combat();
        combat.entities.potions = vec![
            Some(Potion::with_affordance_truth(
                PotionId::FairyPotion,
                1,
                true,
                true,
                false,
            )),
            None,
            None,
        ];

        let inputs = engine_atomic_actions(&EngineState::CombatPlayerTurn, &combat);
        assert!(
            !inputs.iter().any(|input| matches!(
                input,
                ClientInput::UsePotion {
                    potion_index: 0,
                    ..
                }
            )),
            "Fairy in a Bottle is passive and should not be a manual root action"
        );
    }

    #[test]
    fn engine_atomic_actions_skip_smoke_bomb_during_boss_combat() {
        let mut combat = build_fixture_combat();
        combat.meta.is_boss_fight = true;
        combat.entities.potions = vec![
            Some(Potion::with_affordance_truth(
                PotionId::SmokeBomb,
                1,
                true,
                true,
                false,
            )),
            None,
            None,
        ];

        let inputs = engine_atomic_actions(&EngineState::CombatPlayerTurn, &combat);
        assert!(
            !inputs.iter().any(|input| matches!(
                input,
                ClientInput::UsePotion {
                    potion_index: 0,
                    ..
                }
            )),
            "Smoke Bomb is not usable in boss combat"
        );
    }

    #[test]
    fn smoke_escape_in_progress_exposes_no_player_input() {
        let mut combat = build_fixture_combat();
        combat.turn.counters.player_escaping = true;
        combat.entities.potions = vec![
            Some(Potion::with_affordance_truth(
                PotionId::EnergyPotion,
                1,
                true,
                true,
                false,
            )),
            None,
            None,
        ];

        assert_eq!(
            engine_atomic_actions(&EngineState::CombatPlayerTurn, &combat),
            Vec::<ClientInput>::new()
        );
    }

    #[test]
    fn engine_atomic_actions_skip_smoke_bomb_when_visible_monster_is_boss() {
        let mut combat = build_fixture_combat();
        combat.meta.is_boss_fight = false;
        combat.entities.monsters = vec![test_monster(EnemyId::SlimeBoss)];
        combat.entities.potions = vec![
            Some(Potion::with_affordance_truth(
                PotionId::SmokeBomb,
                1,
                true,
                true,
                false,
            )),
            None,
            None,
        ];

        let inputs = engine_atomic_actions(&EngineState::CombatPlayerTurn, &combat);
        assert!(
            !inputs.iter().any(|input| matches!(
                input,
                ClientInput::UsePotion {
                    potion_index: 0,
                    ..
                }
            )),
            "Java SmokeBomb.canUse blocks by monster EnemyType.BOSS even when a fixture did not set a room boss flag"
        );
    }

    #[test]
    fn engine_atomic_actions_keep_liquid_memories_with_empty_discard_pile() {
        let mut combat = build_fixture_combat();
        combat.zones.discard_pile.clear();
        combat.entities.potions = vec![
            Some(Potion::with_affordance_truth(
                PotionId::LiquidMemories,
                1,
                true,
                true,
                false,
            )),
            None,
            None,
        ];

        let inputs = engine_atomic_actions(&EngineState::CombatPlayerTurn, &combat);
        assert!(
            inputs.iter().any(|input| matches!(
                input,
                ClientInput::UsePotion {
                    potion_index: 0,
                    ..
                }
            )),
            "Liquid Memories is Java-usable with an empty discard pile; the action fizzles after consuming it"
        );
    }

    #[test]
    fn engine_fizzles_liquid_memories_empty_discard_after_consuming_potion() {
        let mut combat = build_fixture_combat();
        combat.zones.discard_pile.clear();
        combat.entities.potions = vec![
            Some(Potion::with_affordance_truth(
                PotionId::LiquidMemories,
                1,
                true,
                true,
                false,
            )),
            None,
            None,
        ];

        let mut engine = EngineState::CombatPlayerTurn;
        let alive = crate::engine::core::tick_until_stable_turn(
            &mut engine,
            &mut combat,
            ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
        );

        assert!(alive);
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        assert!(combat.entities.potions[0].is_none());
        let diagnostics = combat.take_engine_diagnostics();
        assert!(
            diagnostics.iter().all(|diagnostic| diagnostic.severity
                != crate::state::selection::EngineDiagnosticSeverity::Error),
            "empty Liquid Memories should not emit an engine error: {diagnostics:?}"
        );
    }

    #[test]
    fn engine_fizzles_empty_warcry_hand_select_without_error() {
        let mut combat = build_fixture_combat();
        combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Warcry,
            90_001,
        )];
        combat.zones.draw_pile.clear();
        combat.zones.discard_pile.clear();
        combat.turn.energy = 1;

        let mut engine = EngineState::CombatPlayerTurn;
        let alive = crate::engine::core::tick_until_stable_turn(
            &mut engine,
            &mut combat,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
        );

        assert!(alive);
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        let diagnostics = combat.take_engine_diagnostics();
        assert!(
            diagnostics.iter().all(|diagnostic| diagnostic.severity
                != crate::state::selection::EngineDiagnosticSeverity::Error),
            "empty Warcry with no card to put back should not emit an engine error: {diagnostics:?}"
        );
    }

    #[test]
    fn pending_scry_stays_symbolic_instead_of_materializing_subsets() {
        let mut combat = build_fixture_combat();
        combat.zones.draw_pile = (vec![
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::Strike,
                10,
            ),
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::Defend,
                20,
            ),
        ])
        .into();
        let engine = EngineState::PendingChoice(PendingChoice::ScrySelect {
            cards: vec![
                crate::content::cards::CardId::Strike,
                crate::content::cards::CardId::Defend,
            ],
            card_lineages: [10, 20].map(crate::test_support::scenario_lineage).to_vec(),
        });

        let surface =
            crate::sim::combat_action_surface::combat_legal_action_surface_v2(&engine, &combat);

        assert!(surface.atomic_actions.is_empty());
        assert_eq!(surface.selection_families.len(), 1);
        assert!(is_legal_move(
            &engine,
            &combat,
            &ClientInput::SubmitScryDiscard(vec![])
        ));
        assert!(is_legal_move(
            &engine,
            &combat,
            &ClientInput::SubmitScryDiscard(vec![0, 1])
        ));
    }

    #[test]
    fn pending_selection_membership_accepts_the_full_symbolic_domain() {
        let mut combat = build_fixture_combat();
        combat.zones.hand = (0..10)
            .map(|index| {
                crate::runtime::combat::CombatCard::for_scenario(
                    crate::content::cards::CardId::Strike,
                    100 + index,
                )
            })
            .collect();
        let engine = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: (100..110)
                .map(crate::test_support::scenario_lineage)
                .collect(),
            min_cards: 1,
            max_cards: 1,
            can_cancel: true,
            reason: crate::state::HandSelectReason::Discard,
        });
        let last_candidate = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Hand,
            [crate::test_support::scenario_lineage(109)],
        ));

        assert!(is_legal_move(&engine, &combat, &last_candidate));
        assert!(is_legal_move(&engine, &combat, &ClientInput::Cancel));
    }

    #[test]
    fn pending_membership_rejects_scry_indices_that_alias_one_card_lineage() {
        let mut combat = build_fixture_combat();
        combat.zones.draw_pile = (vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Strike,
            42,
        )])
        .into();
        let engine = EngineState::PendingChoice(PendingChoice::ScrySelect {
            cards: vec![
                crate::content::cards::CardId::Strike,
                crate::content::cards::CardId::Strike,
            ],
            card_lineages: vec![crate::test_support::scenario_lineage(42); 2],
        });

        assert!(!is_legal_move(
            &engine,
            &combat,
            &ClientInput::SubmitScryDiscard(vec![0, 1]),
        ));
    }

    #[test]
    fn pending_membership_rejects_grid_reason_source_mismatch() {
        let mut combat = build_fixture_combat();
        combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Strike,
            42,
        )];
        let engine = EngineState::PendingChoice(PendingChoice::GridSelect {
            source_pile: crate::state::core::PileType::Hand,
            candidate_lineages: vec![crate::runtime::combat::CardLineageId::constructed_scenario(
                42,
            )],
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: crate::state::core::GridSelectReason::MoveToDrawPile,
        });
        let input = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Grid,
            [crate::test_support::scenario_lineage(42)],
        ));

        assert!(!is_legal_move(&engine, &combat, &input));
    }

    #[test]
    fn pending_choice_surfaces_never_use_fake_proceed_fallback() {
        let mut combat = build_fixture_combat();
        combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Strike,
            10,
        )];
        combat.zones.discard_pile = vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Defend,
            20,
        )]
        .into();
        combat.zones.draw_pile = (vec![
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::Strike,
                30,
            ),
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::Defend,
                40,
            ),
        ])
        .into();
        let choices = vec![
            PendingChoice::HandSelect {
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(10),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: crate::state::HandSelectReason::Discard,
            },
            PendingChoice::GridSelect {
                source_pile: crate::state::PileType::Discard,
                candidate_lineages: vec![
                    crate::runtime::combat::CardLineageId::constructed_scenario(20),
                ],
                min_cards: 1,
                max_cards: 1,
                can_cancel: false,
                reason: crate::state::GridSelectReason::MoveToDrawPile,
            },
            PendingChoice::DiscoverySelect(crate::state::DiscoveryChoiceState {
                cards: vec![crate::content::cards::CardId::Strike],
                colorless: false,
                card_type: None,
                amount: 1,
                can_skip: true,
            }),
            PendingChoice::ScrySelect {
                cards: vec![
                    crate::content::cards::CardId::Strike,
                    crate::content::cards::CardId::Defend,
                ],
                card_lineages: [30, 40].map(crate::test_support::scenario_lineage).to_vec(),
            },
            PendingChoice::CardRewardSelect {
                cards: vec![crate::content::cards::CardId::ShrugItOff],
                destination: crate::runtime::action::CardDestination::Hand,
                can_skip: true,
            },
            PendingChoice::ForeignInfluenceSelect {
                cards: vec![crate::content::cards::CardId::Headbutt],
                upgraded: false,
            },
            PendingChoice::ChooseOneSelect {
                choices: vec![crate::state::ChooseOneCardChoice {
                    card_id: crate::content::cards::CardId::InfernalBlade,
                    upgrades: 0,
                }],
            },
            PendingChoice::StanceChoice,
        ];

        for choice in choices {
            let position =
                CombatPosition::new(EngineState::PendingChoice(choice.clone()), combat.clone());
            let surface = EngineCombatStepper.legal_action_surface(&position);
            assert!(
                !surface.atomic_actions.is_empty() || !surface.selection_families.is_empty(),
                "pending choice {choice:?} should expose an atomic action or a symbolic family"
            );
            assert!(
                !surface.atomic_actions.contains(&ClientInput::Proceed),
                "pending choice {choice:?} must not fall back to fake proceed: {surface:?}"
            );
        }
    }

    #[test]
    fn engine_atomic_actions_skip_cards_when_velvet_choker_locked() {
        let mut combat = build_fixture_combat();
        combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Apparition,
            90_001,
        )];
        combat.turn.energy = 3;
        combat.turn.counters.cards_played_this_turn = 6;
        combat
            .entities
            .player
            .relics
            .push(crate::content::relics::RelicState::new(
                crate::content::relics::RelicId::VelvetChoker,
            ));
        combat.entities.player.relics.last_mut().unwrap().counter = 6;

        let inputs = engine_atomic_actions(&EngineState::CombatPlayerTurn, &combat);
        assert!(
            !inputs
                .iter()
                .any(|input| matches!(input, ClientInput::PlayCard { .. })),
            "Velvet Choker prevents playing more than six cards in a turn, so card actions must not enter the legal mask"
        );
        assert!(
            inputs
                .iter()
                .any(|input| matches!(input, ClientInput::EndTurn)),
            "EndTurn should remain legal under Velvet Choker"
        );
    }
}
