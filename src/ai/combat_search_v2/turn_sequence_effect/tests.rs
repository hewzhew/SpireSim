use super::projection::{card_identity_order_key, card_public_order_key};
use super::*;
use crate::ai::combat_search_v2::state_abstraction::{
    StateAbstractionRevealGate, StateDivergenceKind,
};
use crate::content::cards::CardId;
use crate::runtime::combat::CombatCard;

#[test]
fn classifier_prefers_public_delta_over_hidden_identity_delta() {
    let mut aggregate = TurnSequenceEffectAggregate::default();
    aggregate.public_state_keys.insert("a".to_string());
    aggregate.public_state_keys.insert("b".to_string());
    aggregate.hand_identity_order_keys.insert("c".to_string());
    aggregate.hand_identity_order_keys.insert("d".to_string());

    let divergence = aggregate.classify();

    assert_eq!(divergence.kind, StateDivergenceKind::ImmediatePublicDelta);
    assert_eq!(
        divergence.guessed_reveal_gate,
        StateAbstractionRevealGate::NextLegalActionGeneration
    );
}

#[test]
fn classifier_marks_identity_only_hand_delta_as_uuid_delta() {
    let mut aggregate = TurnSequenceEffectAggregate::default();
    aggregate.hand_public_order_keys.insert("same".to_string());
    aggregate
        .hand_identity_order_keys
        .insert("uuid-a".to_string());
    aggregate
        .hand_identity_order_keys
        .insert("uuid-b".to_string());

    let divergence = aggregate.classify();

    assert_eq!(divergence.kind, StateDivergenceKind::CardLineageDelta);
    assert_eq!(
        divergence.first_divergence_path,
        Some("combat.zones.hand.lineage_order")
    );
}

#[test]
fn card_public_key_ignores_lineage_but_identity_key_keeps_it() {
    let left = vec![CombatCard::for_scenario(CardId::Strike, 1)];
    let right = vec![CombatCard::for_scenario(CardId::Strike, 2)];

    assert_eq!(card_public_order_key(&left), card_public_order_key(&right));
    assert_ne!(
        card_identity_order_key(&left),
        card_identity_order_key(&right)
    );
}
