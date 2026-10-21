use super::{
    combat_dominance_key, combat_exact_state_key, combat_exact_state_key_profiled_v1,
    diagnostic_outcome_key, pending_choice::pending_choice_key, stable_dominance_bucket_key,
    stable_outcome_key,
};
use crate::content::cards::CardId;
use crate::content::monsters::EnemyId;
use crate::content::potions::{Potion, PotionId};
use crate::runtime::combat::{CombatCard, QueuedCardPlay, QueuedCardSource};
use crate::state::core::PendingChoice;
use crate::state::EngineState;
use crate::test_support::{blank_test_combat, planned_monster};

mod cards;
mod dominance;
mod monster;
mod pending_choice;
mod postcombat;
mod stable;

#[test]
fn constricted_sources_are_distinct_search_states() {
    use crate::content::powers::PowerId;
    use crate::engine::action_handlers::powers::handle_apply_power;
    let mut first = blank_test_combat();
    let mut second = first.clone();
    handle_apply_power(1, 0, PowerId::Constricted, 10, &mut first);
    handle_apply_power(2, 0, PowerId::Constricted, 10, &mut second);
    let engine = EngineState::CombatPlayerTurn;
    assert_ne!(
        combat_exact_state_key(&engine, &first),
        combat_exact_state_key(&engine, &second)
    );
    assert_ne!(
        combat_dominance_key(&engine, &first),
        combat_dominance_key(&engine, &second)
    );
    assert_ne!(
        stable_outcome_key(&engine, &first),
        stable_outcome_key(&engine, &second)
    );
}

#[test]
fn profiled_exact_key_is_identical_to_the_production_builder() {
    let combat = blank_test_combat();
    let engine = EngineState::CombatPlayerTurn;

    let ordinary = combat_exact_state_key(&engine, &combat);
    let (profiled, _) = combat_exact_state_key_profiled_v1(&engine, &combat);

    assert_eq!(profiled, ordinary);
}

#[test]
fn semantic_exact_identity_v2_has_a_fixed_cross_process_fixture() {
    let mut combat = blank_test_combat();
    combat
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::PenNib,
        ));
    combat
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::Calipers,
        ));

    assert_eq!(
        super::combat_exact_state_hash_v2(&EngineState::CombatPlayerTurn, &combat),
        "44e56393f95cf9f86569ffc9429b28a5ce5449ebee42720d40633599b1e4d01d"
    );
}

#[test]
fn exact_identity_ignores_and_rebuilds_the_derived_relic_dispatch_cache() {
    let mut combat = blank_test_combat();
    combat
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::PenNib,
        ));
    let mut inconsistent_cache = combat.clone();
    std::sync::Arc::make_mut(&mut inconsistent_cache.entities.player.relic_buses)
        .on_use_card
        .push(999);

    let engine = EngineState::CombatPlayerTurn;
    assert_eq!(
        combat_exact_state_key(&engine, &combat),
        combat_exact_state_key(&engine, &inconsistent_cache),
        "a derived dispatch cache is not independent exact combat state"
    );
    assert_eq!(
        super::combat_exact_state_hash_v2(&engine, &combat),
        super::combat_exact_state_hash_v2(&engine, &inconsistent_cache),
        "durable V2 identity must rebuild its compatibility projection from relics"
    );
}
