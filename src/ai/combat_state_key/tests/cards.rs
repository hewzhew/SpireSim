use super::*;
use std::collections::HashSet;

#[test]
fn exact_key_keeps_transient_multi_target_damage() {
    let mut baseline = blank_test_combat();
    let mut card = CombatCard::for_scenario(CardId::Cleave, 7);
    card.multi_damage = smallvec::smallvec![8, 8];
    baseline.zones.hand = vec![card];

    let mut changed = baseline.clone();
    changed.zones.hand[0].multi_damage[1] = 9;

    assert_ne!(
        combat_exact_state_key(&EngineState::CombatPlayerTurn, &baseline),
        combat_exact_state_key(&EngineState::CombatPlayerTurn, &changed),
        "the inline key representation must preserve exact card semantics"
    );
}

#[test]
fn cached_master_deck_hash_collision_still_compares_every_card() {
    let mut strike = blank_test_combat();
    strike.meta.master_deck_snapshot = vec![CombatCard::for_scenario(CardId::Strike, 7)].into();
    let mut defend = blank_test_combat();
    defend.meta.master_deck_snapshot = vec![CombatCard::for_scenario(CardId::Defend, 7)].into();
    strike
        .meta
        .master_deck_snapshot
        .force_structural_hash_for_test(41);
    defend
        .meta
        .master_deck_snapshot
        .force_structural_hash_for_test(41);

    let strike_key = combat_exact_state_key(&EngineState::CombatPlayerTurn, &strike);
    let defend_key = combat_exact_state_key(&EngineState::CombatPlayerTurn, &defend);
    assert_ne!(strike_key, defend_key);
    assert_ne!(
        super::super::combat_exact_state_key_hash_v2(&strike_key),
        super::super::combat_exact_state_key_hash_v2(&defend_key),
        "durable identity must serialize deck semantics instead of trusting the cached in-process hash"
    );

    let mut keys = HashSet::new();
    assert!(keys.insert(strike_key));
    assert!(keys.insert(defend_key));
    assert_eq!(keys.len(), 2, "cached hashes are buckets, never identity");
}
