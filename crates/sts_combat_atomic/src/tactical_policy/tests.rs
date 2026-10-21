//! Contract tests for the transitional retained-policy adapter.

use super::*;
use sts_combat_tactics::{
    COMBAT_HORIZON_GUIDE_LANE_V1, COMBAT_PROGRESS_GUIDE_LANE_V1, COMBAT_SETUP_GUIDE_LANE_V1,
    COMBAT_SURVIVAL_GUIDE_LANE_V1, COMBAT_TURN_DEPTH_GUIDE_LANE_V1,
};
use sts_core::content::cards::CardId;
use sts_core::content::monsters::EnemyId;
use sts_core::runtime::combat::CombatCard;
use sts_core::sim::combat::{CombatStepper, EngineCombatStepper};
use sts_core::state::core::EngineState;

fn policy_position() -> CombatPosition {
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.entities.monsters = vec![sts_core::test_support::test_monster(EnemyId::JawWorm)];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 11)];
    combat.turn.energy = 1;
    CombatPosition::new(EngineState::CombatPlayerTurn, combat)
}

#[test]
fn adapter_preserves_retained_atomic_weights() {
    let position = policy_position();
    let inputs = EngineCombatStepper.atomic_actions(&position);
    let choices = inputs
        .iter()
        .map(CombatPolicyChoice::Atomic)
        .collect::<Vec<_>>();
    let input_refs = inputs.iter().collect::<Vec<_>>();
    let expected = oracle_action_policy::oracle_legal_atomic_action_policy_weights_for_refs(
        &position,
        &input_refs,
    );

    assert_eq!(
        RetainedCombatTacticalPolicyV1.weights(&position, &choices),
        expected
    );
}

#[test]
fn adapter_preserves_retained_guide_roles_and_ranks() {
    let position = policy_position();
    let expected = oracle_action_policy::oracle_combat_guide_bundle_v1(&position);
    let policy = RetainedCombatTacticalPolicyV1;
    let state_guides = policy.state_guides(&position);
    let generation_guides = policy.turn_generation_guides(&position);

    assert_eq!(state_guides.len(), 4);
    assert_eq!(state_guides[0].lane, COMBAT_PROGRESS_GUIDE_LANE_V1);
    assert_eq!(state_guides[0].rank.components(), expected.progress);
    assert_eq!(state_guides[1].lane, COMBAT_SURVIVAL_GUIDE_LANE_V1);
    assert_eq!(state_guides[1].rank.components(), expected.survival);
    assert_eq!(state_guides[2].lane, COMBAT_HORIZON_GUIDE_LANE_V1);
    assert_eq!(state_guides[2].rank.components(), expected.horizon);
    assert_eq!(state_guides[3].lane, COMBAT_SETUP_GUIDE_LANE_V1);
    assert_eq!(state_guides[3].rank.components(), expected.setup);

    assert_eq!(generation_guides.len(), 4);
    assert_eq!(generation_guides[0].lane, COMBAT_PROGRESS_GUIDE_LANE_V1);
    assert_eq!(generation_guides[0].rank.components(), expected.progress);
    assert_eq!(generation_guides[1].lane, COMBAT_SURVIVAL_GUIDE_LANE_V1);
    assert_eq!(generation_guides[1].rank.components(), expected.survival);
    assert_eq!(generation_guides[2].lane, COMBAT_TURN_DEPTH_GUIDE_LANE_V1);
    assert_eq!(
        generation_guides[2].rank.components(),
        expected.turn_generation
    );
    assert_eq!(generation_guides[3].lane, COMBAT_SETUP_GUIDE_LANE_V1);
    assert_eq!(generation_guides[3].rank.components(), expected.setup);
}
