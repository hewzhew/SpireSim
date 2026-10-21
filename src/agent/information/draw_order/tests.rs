use super::*;
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::relics::{RelicId, RelicState};
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, Intent};
use crate::sim::combat::{
    apply_combat_input_to_stable_observed_v1, CombatPosition, CombatStepLimits, CombatStepper,
    EngineCombatStepper,
};
use crate::state::core::{ClientInput, EngineState};
use crate::state::selection::{SelectionResolution, SelectionScope};

fn kind(card_id: CardId) -> DrawCardKind {
    DrawCardKind {
        card_id,
        upgrades: 0,
    }
}

fn root() -> CombatPosition {
    let mut combat = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 7;
    monster.current_hp = 100;
    monster.max_hp = 100;
    combat.entities.monsters.push(monster);
    combat.set_monster_protocol_visible_intent(
        7,
        Intent::Attack {
            damage: 11,
            hits: 1,
        },
    );
    combat.zones.hand = vec![
        CombatCard::for_scenario(CardId::Headbutt, 10),
        CombatCard::for_scenario(CardId::Strike, 11),
        CombatCard::for_scenario(CardId::ShrugItOff, 12),
    ];
    combat.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Bash, 20)].into();
    combat.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::Defend, 30),
        CombatCard::for_scenario(CardId::Strike, 31),
    ]
    .into();
    CombatPosition::new(EngineState::CombatPlayerTurn, combat)
}

fn observed(position: &CombatPosition, input: ClientInput) -> CombatObservedStepResultV1 {
    assert!(EngineCombatStepper.is_legal_action(position, &input));
    let limits = CombatStepLimits {
        max_engine_steps: 512,
        deadline: None,
    };
    let exact = EngineCombatStepper.apply_to_stable(position, input.clone(), limits);
    let receipt = apply_combat_input_to_stable_observed_v1(position, input, limits);
    assert!(!receipt.step.truncated && !receipt.step.timed_out);
    assert_eq!(
        receipt.step.position, exact.position,
        "observation must not change gameplay or RNG"
    );
    assert_eq!(
        serde_json::to_value(&receipt.step.position).unwrap(),
        serde_json::to_value(&exact.position).unwrap()
    );
    receipt
}

#[test]
fn headbutt_memory_survives_unrelated_attack_and_is_consumed_by_draw() {
    let position = root();
    let knowledge = DrawOrderKnowledge::from_observation(&public_combat_state_v1(&position.combat));
    assert!(knowledge.known_prefix().is_empty());
    let put = observed(
        &position,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(7),
        },
    );
    let knowledge = knowledge.after_observed_step(&put).unwrap();
    assert_eq!(knowledge.known_prefix(), [kind(CardId::Bash)]);
    assert!(knowledge.permits_kind_order(&[
        kind(CardId::Bash),
        kind(CardId::Strike),
        kind(CardId::Defend)
    ]));
    assert!(!knowledge.permits_kind_order(&[
        kind(CardId::Defend),
        kind(CardId::Bash),
        kind(CardId::Strike)
    ]));
    let attack = observed(
        &put.step.position,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(7),
        },
    );
    let after_attack = knowledge.after_observed_step(&attack).unwrap();
    assert_eq!(after_attack, knowledge);
    let draw = observed(
        &attack.step.position,
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
    );
    assert_eq!(draw.drawn_cards[0].id, CardId::Bash);
    let after_draw = after_attack.after_observed_step(&draw).unwrap();
    assert!(after_draw.known_prefix().is_empty());
    assert_eq!(after_draw.composition().len(), 2);
}

#[test]
fn selected_headbutt_binds_the_chosen_public_kind() {
    let mut position = root();
    position
        .combat
        .zones
        .discard_pile
        .push(CombatCard::for_scenario(CardId::Carnage, 21));
    let knowledge = DrawOrderKnowledge::from_observation(&public_combat_state_v1(&position.combat));
    let put = observed(
        &position,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(7),
        },
    );
    assert!(matches!(
        put.step.position.engine,
        EngineState::PendingChoice(_)
    ));
    let knowledge = knowledge.after_observed_step(&put).unwrap();
    assert!(knowledge.known_prefix().is_empty());
    let selected = observed(
        &put.step.position,
        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Grid,
            [position.combat.zones.discard_pile[1].lineage],
        )),
    );
    assert_eq!(
        knowledge
            .after_observed_step(&selected)
            .unwrap()
            .known_prefix(),
        [kind(CardId::Carnage)]
    );
}

#[test]
fn draw_receipt_keeps_shuffle_between_old_and_new_draws() {
    let mut position = root();
    position.combat.zones.hand = vec![CombatCard::for_scenario(CardId::BattleTrance, 10)];
    position.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Defend, 30)].into();
    position
        .combat
        .zones
        .discard_pile
        .push(CombatCard::for_scenario(CardId::Strike, 21));
    let knowledge = DrawOrderKnowledge::from_observation(&public_combat_state_v1(&position.combat));
    let draw = observed(
        &position,
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
    );
    assert_eq!(draw.draw_pile_changes.len(), 4);
    assert_eq!(
        draw.draw_pile_changes[0],
        DrawPileChange::TakeTop {
            card: kind(CardId::Defend)
        }
    );
    assert_eq!(
        draw.draw_pile_changes[1],
        DrawPileChange::PrependShuffled { count: 2 }
    );
    assert_eq!(
        knowledge
            .after_observed_step(&draw)
            .unwrap()
            .composition()
            .len(),
        0
    );
}

#[test]
fn shuffled_discard_does_not_erase_known_untouched_draw_suffix() {
    let mut position = root();
    position
        .combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::FrozenEye));
    let knowledge = DrawOrderKnowledge::from_observation(&public_combat_state_v1(&position.combat));
    position.combat.entities.player.relics.clear();
    position.combat.queue_action_back(Action::EmptyDeckShuffle);
    let change = observed(
        &position,
        ClientInput::PlayCard {
            card_index: 1,
            target: Some(7),
        },
    );
    let after = knowledge.after_observed_step(&change).unwrap();
    assert!(after.known_prefix().is_empty());
    assert_eq!(
        after.known_suffix(),
        [kind(CardId::Defend), kind(CardId::Strike)]
    );
    assert_eq!(after.composition().len(), 3);
}

#[test]
fn frozen_eye_reveals_current_order_but_not_future_shuffle() {
    let mut position = root();
    position
        .combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::FrozenEye));
    let knowledge = DrawOrderKnowledge::from_observation(&public_combat_state_v1(&position.combat));
    position.combat.queue_action_back(Action::ShuffleDrawPile {
        trigger_relics: false,
    });
    let receipt = observed(
        &position,
        ClientInput::PlayCard {
            card_index: 1,
            target: Some(7),
        },
    );
    let after = knowledge.after_observed_step(&receipt).unwrap();
    let visible = receipt
        .step
        .position
        .combat
        .zones
        .draw_pile
        .iter()
        .map(DrawCardKind::from)
        .collect::<Vec<_>>();
    assert_eq!(after.known_prefix(), visible);
    assert!(receipt
        .draw_pile_changes
        .iter()
        .any(|c| matches!(c, DrawPileChange::OrderUnknown { .. })));
}

#[test]
fn random_insertion_keeps_relative_order_without_disclosing_index() {
    let position = root();
    let mut public = public_combat_state_v1(&position.combat);
    public.cards.draw.evidence = ObservationEvidenceKindV1::PublicOrderedCollection;
    // This fixture explicitly provides a visible order; later observations hide it.
    let knowledge = DrawOrderKnowledge::from_observation(&public);
    public.cards.draw.evidence = ObservationEvidenceKindV1::PublicUnorderedCollection;
    let added = public.cards.hand.cards[0].clone();
    public.cards.draw.cards.push(added);
    let inserted = kind(CardId::Headbutt);
    let after = knowledge
        .after_changes(
            &[DrawPileChange::InsertUnpositioned { card: inserted }],
            &public,
        )
        .unwrap();
    assert!(after.known_prefix().is_empty() && after.known_suffix().is_empty());
    let old = knowledge.known_prefix();
    for index in 0..=2 {
        let mut order = old.to_vec();
        order.insert(index, inserted);
        assert!(after.permits_kind_order(&order));
    }
    assert!(!after.permits_kind_order(&[old[1], old[0], inserted]));
}

#[test]
fn duplicates_never_acquire_private_instance_identity() {
    let mut position = root();
    position.combat.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::Strike, 30),
        CombatCard::for_scenario(CardId::Defend, 31),
        CombatCard::for_scenario(CardId::Strike, 32),
    ]
    .into();
    position
        .combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::FrozenEye));
    let knowledge = DrawOrderKnowledge::from_observation(&public_combat_state_v1(&position.combat));
    position.combat.entities.player.relics.clear();
    position.combat.zones.draw_pile.remove(0);
    let after = knowledge
        .after_changes(
            &[DrawPileChange::RemoveUnpositioned {
                card: kind(CardId::Strike),
            }],
            &public_combat_state_v1(&position.combat),
        )
        .unwrap();
    assert!(after.permits_kind_order(&[kind(CardId::Defend), kind(CardId::Strike)]));
    assert!(after.permits_kind_order(&[kind(CardId::Strike), kind(CardId::Defend)]));
    let text = serde_json::to_string(&after).unwrap();
    assert!(!text.contains("lineage") && !text.contains("rng"));
}

#[test]
fn interrupted_transition_cannot_publish_settled_knowledge() {
    let position = root();
    let knowledge = DrawOrderKnowledge::from_observation(&public_combat_state_v1(&position.combat));
    let receipt = apply_combat_input_to_stable_observed_v1(
        &position,
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(7),
        },
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: Some(std::time::Instant::now()),
        },
    );
    assert_eq!(
        knowledge.after_observed_step(&receipt),
        Err(DrawKnowledgeError::UnsettledTransition)
    );
    assert!(receipt.draw_pile_changes.is_empty());
}

#[test]
fn uncertain_insertion_and_removal_never_exclude_the_real_duplicate_order() {
    for ids in [
        [CardId::Strike, CardId::Defend, CardId::Strike],
        [CardId::Strike, CardId::Strike, CardId::Defend],
        [CardId::Strike, CardId::Strike, CardId::Strike],
    ] {
        let mut position = root();
        position.combat.zones.draw_pile = ids
            .iter()
            .enumerate()
            .map(|(i, id)| CombatCard::for_scenario(*id, i as u32 + 20))
            .collect();
        position
            .combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::FrozenEye));
        let known = DrawOrderKnowledge::from_observation(&public_combat_state_v1(&position.combat));
        position.combat.entities.player.relics.clear();
        for inserted_id in [CardId::Strike, CardId::Bash] {
            for insert_at in 0..=3 {
                let mut inserted = position.clone();
                inserted
                    .combat
                    .zones
                    .draw_pile
                    .insert(insert_at, CombatCard::for_scenario(inserted_id, 50));
                let knowledge = known
                    .after_changes(
                        &[DrawPileChange::InsertUnpositioned {
                            card: kind(inserted_id),
                        }],
                        &public_combat_state_v1(&inserted.combat),
                    )
                    .unwrap();
                for remove_at in 0..4 {
                    let mut removed = inserted.clone();
                    let card = removed.combat.zones.draw_pile.remove(remove_at);
                    let actual = removed
                        .combat
                        .zones
                        .draw_pile
                        .iter()
                        .map(DrawCardKind::from)
                        .collect::<Vec<_>>();
                    let snapshot = public_combat_state_v1(&removed.combat);
                    let next = knowledge
                        .after_changes(
                            &[DrawPileChange::RemoveUnpositioned {
                                card: (&card).into(),
                            }],
                            &snapshot,
                        )
                        .unwrap();
                    assert!(next.permits_kind_order(&actual));
                    if remove_at == 0 {
                        let next = knowledge
                            .after_changes(
                                &[DrawPileChange::TakeTop {
                                    card: (&card).into(),
                                }],
                                &snapshot,
                            )
                            .unwrap();
                        assert!(next.permits_kind_order(&actual));
                    }
                }
            }
        }
    }
}
