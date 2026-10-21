//! Conservative equivalence classes over exact legal combat inputs.
//!
//! The simulator keeps the complete legal action surface.  This module only
//! identifies duplicate source-bound inputs, adjacent equal battle instances,
//! and unordered Retain member sets. Card attributes alone are insufficient:
//! instance identity and
//! ordered piles can affect a later draw, mutation or fixed-RNG shuffle.

use std::collections::BTreeMap;

use crate::content::cards;
use crate::runtime::combat::{CardLineageId, CardPileView, CombatCard, CombatState};
use crate::state::core::{ClientInput, EngineState, HandSelectReason, PendingChoice, PileType};
use crate::state::selection::{SelectionResolution, SelectionScope};

/// Equivalence of continuation state, not of the recorded action or events.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSelectionStateEquivalenceV1 {
    GamblingChipCancel,
}

/// Whether permutations of the same selected members have the same continuation
/// state at this source. Retain only sets flags in the existing hand: it neither
/// moves cards nor runs a hook per selected member. The SelectionResolved event
/// still records the submitted order, so witnesses must keep their actual input.
/// Other selection reasons retain their ordered enumeration.
pub fn combat_selection_members_are_unordered_v1(engine: &EngineState) -> bool {
    matches!(
        engine,
        EngineState::PendingChoice(PendingChoice::HandSelect {
            reason: HandSelectReason::Retain,
            ..
        })
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct EquivalentCombatSelectionInputV1 {
    pub input: ClientInput,
    pub basis: CombatSelectionStateEquivalenceV1,
}

/// Recognize an atomic input that stands for a legal empty selection at this
/// exact boundary. Keep the original action in every witness and event log.
///
/// GamblingChip with zero cards queues no draw or discard hooks, and Cancel
/// resumes the same pending queue. The SelectionResolved diagnostic differs.
/// Other reasons require their own proof; zero bounds alone are insufficient.
pub fn equivalent_combat_selection_input_v1(
    engine: &EngineState,
    input: &ClientInput,
) -> Option<EquivalentCombatSelectionInputV1> {
    match (engine, input) {
        (
            EngineState::PendingChoice(PendingChoice::HandSelect {
                min_cards: 0,
                can_cancel: true,
                reason: HandSelectReason::GamblingChip,
                ..
            }),
            ClientInput::Cancel,
        ) => Some(EquivalentCombatSelectionInputV1 {
            input: ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Hand,
                std::iter::empty(),
            )),
            basis: CombatSelectionStateEquivalenceV1::GamblingChipCancel,
        }),
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CombatActionEquivalenceKeyV1 {
    pub kind: CombatActionEquivalenceKindV1,
    pub signature: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CombatActionEquivalenceKindV1 {
    StarterBasicPlayCard,
    SingleCardPendingChoiceSelection,
}

/// Returns a conservative exact-state equivalence key for one legal input.
///
/// `None` means the action remains its own class. Starter plays can share a
/// class only inside one contiguous run of equal cards, including lineage.
/// Single-card selections retain the selected lineage. Non-starter plays are
/// not generalized. No class erases an ordered-pile or instance difference.
pub fn combat_action_equivalence_key_v1(
    engine: &EngineState,
    combat: &CombatState,
    input: &ClientInput,
) -> Option<CombatActionEquivalenceKeyV1> {
    match input {
        ClientInput::PlayCard { card_index, target } => {
            if !matches!(engine, EngineState::CombatPlayerTurn) {
                return None;
            }
            let card = combat.zones.hand.get(*card_index)?;
            if !cards::is_starter_basic(card.id) {
                return None;
            }
            Some(CombatActionEquivalenceKeyV1 {
                kind: CombatActionEquivalenceKindV1::StarterBasicPlayCard,
                signature: starter_basic_card_signature(combat, *card_index, card, *target),
            })
        }
        ClientInput::SubmitSelection(resolution)
            if matches!(
                resolution.scope,
                SelectionScope::Hand | SelectionScope::Grid
            ) =>
        {
            pending_single_card_selection_key(engine, combat, &resolution.selected_card_lineages())
        }
        _ => None,
    }
}

/// Maps every input ordinal to the first exact-state-equivalent input ordinal.
///
/// The returned vector is aligned one-to-one with `inputs`; an unclassified
/// action maps to itself.  The first member of a class is therefore the stable
/// canonical representative while the complete legal input list remains
/// untouched.
pub fn canonical_combat_action_representatives_v1(
    engine: &EngineState,
    combat: &CombatState,
    inputs: &[ClientInput],
) -> Vec<usize> {
    let mut seen = BTreeMap::new();
    inputs
        .iter()
        .enumerate()
        .map(|(ordinal, input)| {
            // Card-play search calls this at every expanded position. Compare
            // the small hand directly instead of serializing one String key
            // per card/target. The public diagnostic key below is unchanged.
            if let ClientInput::PlayCard { card_index, target } = input {
                if matches!(engine, EngineState::CombatPlayerTurn) {
                    if let Some(card) = combat.zones.hand.get(*card_index) {
                        if cards::is_starter_basic(card.id) {
                            return inputs[..ordinal]
                                .iter()
                                .position(|previous| {
                                    let ClientInput::PlayCard {
                                        card_index: previous_index,
                                        target: previous_target,
                                    } = previous
                                    else {
                                        return false;
                                    };
                                    if previous_target != target {
                                        return false;
                                    }
                                    combat.zones.hand.get(*previous_index).is_some_and(|other| {
                                        card == other
                                            && combat.zones.hand[(*previous_index).min(*card_index)
                                                ..=(*previous_index).max(*card_index)]
                                                .iter()
                                                .all(|between| between == card)
                                    })
                                })
                                .unwrap_or(ordinal);
                        }
                    }
                }
                return ordinal;
            }
            let Some(key) = combat_action_equivalence_key_v1(engine, combat, input) else {
                return ordinal;
            };
            *seen.entry(key).or_insert(ordinal)
        })
        .collect()
}

fn pending_single_card_selection_key(
    engine: &EngineState,
    combat: &CombatState,
    lineages: &[CardLineageId],
) -> Option<CombatActionEquivalenceKeyV1> {
    let [lineage] = lineages else {
        return None;
    };
    let EngineState::PendingChoice(choice) = engine else {
        return None;
    };

    let (scope, cards) = match choice {
        PendingChoice::GridSelect {
            source_pile,
            reason,
            candidate_lineages,
            ..
        } if candidate_lineages.contains(lineage) => (
            format!(
                "grid_select/source:{}/reason:{}",
                source_pile.vocabulary_token_v1(),
                reason.vocabulary_token_v1()
            ),
            pile_cards(combat, *source_pile),
        ),
        PendingChoice::HandSelect {
            reason,
            candidate_lineages,
            ..
        } if candidate_lineages.contains(lineage) => (
            format!("hand_select/reason:{}", reason.vocabulary_token_v1()),
            CardPileView::Contiguous(&combat.zones.hand),
        ),
        _ => return None,
    };
    let card = cards.iter().find(|card| card.lineage == *lineage)?;
    Some(CombatActionEquivalenceKeyV1 {
        kind: CombatActionEquivalenceKindV1::SingleCardPendingChoiceSelection,
        signature: format!(
            "{scope}/selected_lineage:{}/selected_card:{}",
            serde_json::to_string(lineage).expect("lineage serializes"),
            card_runtime_signature(card)
        ),
    })
}

fn pile_cards(combat: &CombatState, pile: PileType) -> CardPileView<'_> {
    match pile {
        PileType::Draw => CardPileView::Contiguous(combat.zones.draw_pile.as_ref()),
        PileType::Discard => CardPileView::Discard(&combat.zones.discard_pile),
        PileType::Exhaust => CardPileView::Contiguous(combat.zones.exhaust_pile.as_slice()),
        PileType::Hand => CardPileView::Contiguous(&combat.zones.hand),
        PileType::Limbo => CardPileView::Contiguous(&combat.zones.limbo),
        PileType::MasterDeck => CardPileView::Contiguous(&[]),
    }
}

fn starter_basic_card_signature(
    combat: &CombatState,
    card_index: usize,
    card: &CombatCard,
    target: Option<usize>,
) -> String {
    let run_start = combat.zones.hand[..card_index]
        .iter()
        .rposition(|other| other != card)
        .map_or(0, |i| i + 1);
    format!(
        "play_card/starter_basic/{}/lineage:{}/hand_run:{run_start}/target:{}",
        card_runtime_signature(card),
        serde_json::to_string(&card.lineage).expect("lineage serializes"),
        crate::sim::combat_action::target_label(combat, target),
    )
}

fn card_runtime_signature(card: &CombatCard) -> String {
    serde_json::to_string(&(
        "combat_card_equivalence_v1",
        card.id.stable_id(),
        card.upgrades,
        card.misc_value,
        card.base_damage_override,
        card.base_block_override,
        card.cost_modifier,
        card.cost_for_turn,
        card.base_damage_mut,
        card.base_block_mut,
        card.base_magic_num_mut,
        &card.multi_damage,
        card.exhaust_override,
        card.retain_override,
        card.free_to_play_once,
        card.energy_on_use,
    ))
    .expect("combat action equivalence signature should serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::runtime::combat::CombatCard;

    #[test]
    fn retain_member_permutations_replay_to_the_same_complete_turn_state() {
        use crate::content::powers::PowerId;
        use crate::runtime::combat::Power;
        use crate::sim::combat::{
            CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
        };
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.monsters = vec![crate::test_support::planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = [
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Wound,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 1))
        .collect();
        combat.zones.hand[1].upgrades = 1;
        combat.zones.hand[2].cost_for_turn = Some(0);
        crate::content::powers::store::set_powers_for(
            &mut combat,
            0,
            vec![Power {
                power_type: PowerId::RetainCards,
                amount: 2,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let step = |source: &CombatPosition, input| {
            assert!(EngineCombatStepper.is_legal_action(source, &input));
            let result = EngineCombatStepper.apply_to_stable(
                source,
                input,
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!result.truncated && !result.timed_out);
            result.position
        };
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let pending = step(&root, ClientInput::EndTurn);
        assert!(combat_selection_members_are_unordered_v1(&pending.engine));
        let candidates = pending
            .combat
            .zones
            .hand
            .iter()
            .map(|c| c.lineage)
            .collect::<Vec<_>>();
        for i in 0..candidates.len() {
            for j in i + 1..candidates.len() {
                let input = |members| {
                    ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                        SelectionScope::Hand,
                        members,
                    ))
                };
                let forward = step(&pending, input(vec![candidates[i], candidates[j]]));
                let reverse = step(&pending, input(vec![candidates[j], candidates[i]]));
                assert_eq!(
                    forward, reverse,
                    "member order must not change hand, RNG, hooks or queued turn work"
                );
                assert!(forward.combat.turn.turn_count > root.combat.turn.turn_count);
            }
        }
        for reason in [
            HandSelectReason::Exhaust,
            HandSelectReason::Discard,
            HandSelectReason::PutOnDrawPile,
            HandSelectReason::PutToBottomOfDraw,
            HandSelectReason::Setup,
            HandSelectReason::Upgrade,
            HandSelectReason::Copy { amount: 2 },
            HandSelectReason::Nightmare { amount: 3 },
            HandSelectReason::GamblingChip,
            HandSelectReason::Recycle,
            HandSelectReason::PotionExhaust {
                has_pending_use_effect: false,
            },
        ] {
            let mut engine = pending.engine.clone();
            let EngineState::PendingChoice(PendingChoice::HandSelect { reason: actual, .. }) =
                &mut engine
            else {
                unreachable!()
            };
            *actual = reason;
            assert!(!combat_selection_members_are_unordered_v1(&engine));
        }
        assert!(!combat_selection_members_are_unordered_v1(&root.engine));
    }

    #[test]
    fn separated_identical_plays_can_change_the_next_shuffle_draw() {
        use crate::sim::combat::{
            CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
        };
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.monsters = vec![crate::test_support::planned_monster(EnemyId::Cultist, 1)];
        combat.turn.energy = 1;
        combat.zones.hand = [CardId::Strike, CardId::Defend, CardId::Strike]
            .into_iter()
            .enumerate()
            .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 1))
            .collect();
        combat.zones.draw_pile = (10..14)
            .map(|i| CombatCard::for_scenario(CardId::Wound, i))
            .collect::<Vec<_>>()
            .into();
        let inputs = [0, 2].map(|card_index| ClientInput::PlayCard {
            card_index,
            target: Some(1),
        });
        let step = |p: &CombatPosition, input: ClientInput| {
            assert!(EngineCombatStepper.is_legal_action(p, &input));
            let s = EngineCombatStepper.apply_to_stable(
                p,
                input,
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!s.timed_out && !s.truncated);
            s.position
        };
        let mut differing_draw = None;
        for seed in 0..32 {
            combat.rng.shuffle_rng = crate::runtime::rng::StsRng::new(seed);
            let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat.clone());
            let a = step(&root, inputs[0].clone());
            let b = step(&root, inputs[1].clone());
            assert_eq!(
                a.combat.entities.player.current_hp,
                b.combat.entities.player.current_hp
            );
            assert_eq!(a.combat.entities.monsters, b.combat.entities.monsters);
            assert_eq!(a.combat.rng.shuffle_rng, b.combat.rng.shuffle_rng);
            let a = step(&a, ClientInput::EndTurn);
            let b = step(&b, ClientInput::EndTurn);
            assert_eq!(a.combat.rng.shuffle_rng, b.combat.rng.shuffle_rng);
            let ids =
                |p: &CombatPosition| p.combat.zones.hand.iter().map(|c| c.id).collect::<Vec<_>>();
            if ids(&a) != ids(&b) {
                differing_draw = Some((seed, ids(&a), ids(&b)));
                break;
            }
        }
        let (seed, left, right) = differing_draw
            .expect("same shuffle stream can expose the hand-removal order difference");
        eprintln!("shuffle seed {seed}: play left -> {left:?}; play right -> {right:?}");
        // Even two copies sharing a lineage cannot cross a different card.
        for same_lineage in [false, true] {
            if same_lineage {
                combat.zones.hand[2] = combat.zones.hand[0].clone();
            }
            assert_eq!(
                canonical_combat_action_representatives_v1(
                    &EngineState::CombatPlayerTurn,
                    &combat,
                    &inputs
                ),
                vec![0, 1]
            );
            assert_ne!(
                combat_action_equivalence_key_v1(
                    &EngineState::CombatPlayerTurn,
                    &combat,
                    &inputs[0]
                ),
                combat_action_equivalence_key_v1(
                    &EngineState::CombatPlayerTurn,
                    &combat,
                    &inputs[1]
                )
            );
        }

        // Playing all three cards in different orders also leaves equal
        // immediate HP/block/damage but a different fixed-RNG next draw.
        combat.turn.energy = 3;
        combat.rng.shuffle_rng = crate::runtime::rng::StsRng::new(seed);
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let play_order = |order: [CardId; 3]| {
            let mut p = root.clone();
            for id in order {
                let card_index = p.combat.zones.hand.iter().position(|c| c.id == id).unwrap();
                p = step(
                    &p,
                    ClientInput::PlayCard {
                        card_index,
                        target: (id == CardId::Strike).then_some(1),
                    },
                );
            }
            p
        };
        let a = play_order([CardId::Strike, CardId::Strike, CardId::Defend]);
        let b = play_order([CardId::Strike, CardId::Defend, CardId::Strike]);
        assert_eq!(
            a.combat.entities.player.current_hp,
            b.combat.entities.player.current_hp
        );
        assert_eq!(
            a.combat.entities.player.block,
            b.combat.entities.player.block
        );
        assert_eq!(a.combat.entities.monsters, b.combat.entities.monsters);
        let a = step(&a, ClientInput::EndTurn);
        let b = step(&b, ClientInput::EndTurn);
        assert_eq!(a.combat.rng.shuffle_rng, b.combat.rng.shuffle_rng);
        assert_ne!(
            a.combat.zones.hand.last().unwrap().id,
            b.combat.zones.hand.last().unwrap().id
        );
    }

    #[test]
    fn gambling_chip_cancel_and_empty_selection_share_state_but_keep_distinct_events() {
        use crate::sim::combat::{
            CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
        };
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.monsters = vec![crate::test_support::planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 11)];
        let candidates = vec![combat.zones.hand[0].lineage];
        let engine = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: candidates.clone(),
            min_cards: 0,
            max_cards: 1,
            can_cancel: true,
            reason: HandSelectReason::GamblingChip,
        });
        let equivalent =
            equivalent_combat_selection_input_v1(&engine, &ClientInput::Cancel).unwrap();
        let root = CombatPosition::new(engine.clone(), combat.clone());
        let settle = |input| {
            EngineCombatStepper.apply_to_stable(
                &root,
                input,
                CombatStepLimits {
                    max_engine_steps: 250,
                    deadline: None,
                },
            )
        };
        let cancelled = settle(ClientInput::Cancel);
        let empty = settle(equivalent.input.clone());
        assert!(!cancelled.truncated && !empty.truncated);
        assert!(!cancelled.timed_out && !empty.timed_out);
        assert_eq!(cancelled.position, empty.position);

        let mut cancelled_engine = engine.clone();
        let mut empty_engine = engine;
        let mut cancelled_combat = combat.clone();
        let mut empty_combat = combat;
        for (engine, combat, input) in [
            (
                &mut cancelled_engine,
                &mut cancelled_combat,
                ClientInput::Cancel,
            ),
            (&mut empty_engine, &mut empty_combat, equivalent.input),
        ] {
            crate::engine::pending_choices::handle_hand_select(
                engine,
                combat,
                &candidates,
                1,
                false,
                true,
                HandSelectReason::GamblingChip,
                input,
            )
            .unwrap();
        }
        assert!(cancelled_combat.take_emitted_events().is_empty());
        assert!(matches!(empty_combat.take_emitted_events().as_slice(),
            [crate::state::selection::DomainEvent::SelectionResolved { selected, .. }] if selected.is_empty()));
        assert_eq!(cancelled_combat, empty_combat);
        assert_eq!(cancelled_engine, empty_engine);
    }

    #[test]
    fn empty_selection_equivalence_requires_its_proven_reason_and_legal_bounds() {
        for (reason, min_cards, can_cancel) in [
            (HandSelectReason::GamblingChip, 0, true),
            (HandSelectReason::GamblingChip, 1, true),
            (HandSelectReason::GamblingChip, 0, false),
            (HandSelectReason::Copy { amount: 2 }, 0, true),
            (HandSelectReason::Exhaust, 0, true),
        ] {
            let engine = EngineState::PendingChoice(PendingChoice::HandSelect {
                candidate_lineages: vec![],
                min_cards,
                max_cards: 1,
                can_cancel,
                reason,
            });
            assert_eq!(
                equivalent_combat_selection_input_v1(&engine, &ClientInput::Cancel).is_some(),
                reason == HandSelectReason::GamblingChip && min_cards == 0 && can_cancel
            );
            assert!(equivalent_combat_selection_input_v1(&engine, &ClientInput::EndTurn).is_none());
        }
        assert!(equivalent_combat_selection_input_v1(
            &EngineState::CombatPlayerTurn,
            &ClientInput::Cancel
        )
        .is_none());
    }

    #[test]
    fn canonical_representatives_require_adjacent_equal_instances() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 1;
        combat.entities.monsters = vec![monster];
        let mut free = CombatCard::for_scenario(CardId::Defend, 12);
        free.free_to_play_once = true;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
            free,
        ];
        let inputs = vec![
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            ClientInput::PlayCard {
                card_index: 1,
                target: None,
            },
            ClientInput::PlayCard {
                card_index: 2,
                target: None,
            },
            ClientInput::EndTurn,
        ];

        assert_eq!(
            canonical_combat_action_representatives_v1(
                &EngineState::CombatPlayerTurn,
                &combat,
                &inputs,
            ),
            vec![0, 1, 2, 3]
        );
        combat.zones.hand[1] = combat.zones.hand[0].clone();
        assert_eq!(
            canonical_combat_action_representatives_v1(
                &EngineState::CombatPlayerTurn,
                &combat,
                &inputs
            ),
            vec![0, 0, 2, 3]
        );
        use crate::sim::combat::{
            CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
        };
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let successors = [0, 1].map(|i| {
            EngineCombatStepper.apply_to_stable(
                &root,
                inputs[i].clone(),
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            )
        });
        assert!(successors.iter().all(|s| !s.timed_out && !s.truncated));
        assert_eq!(successors[0].position, successors[1].position);
    }
}
