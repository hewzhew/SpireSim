use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use crate::runtime::combat::{CombatState, MonsterEntity, Power, PowerPayload};
use crate::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepResult, CombatStepper, EngineCombatStepper,
};
use crate::state::core::{ClientInput, EngineState};
pub(super) use crate::test_support::blank_test_combat;
use crate::test_support::test_monster;

pub(super) fn step_limits() -> CombatStepLimits {
    CombatStepLimits {
        max_engine_steps: 128,
        deadline: None,
    }
}

pub(super) fn player_turn_position(combat: CombatState) -> CombatPosition {
    CombatPosition::new(EngineState::CombatPlayerTurn, combat)
}

pub(super) fn apply_from_player_turn(combat: CombatState, input: ClientInput) -> CombatStepResult {
    apply(&player_turn_position(combat), input)
}

pub(super) fn apply(position: &CombatPosition, input: ClientInput) -> CombatStepResult {
    let stepper = EngineCombatStepper;
    stepper.apply_to_stable(position, input, step_limits())
}

pub(super) fn legal_actions(position: &CombatPosition) -> Vec<ClientInput> {
    let stepper = EngineCombatStepper;
    stepper.atomic_actions(position)
}

pub(super) fn is_legal_action(position: &CombatPosition, input: &ClientInput) -> bool {
    EngineCombatStepper.is_legal_action(position, input)
}

pub(super) fn assert_stable_player_turn(step: &CombatStepResult) {
    assert!(!step.truncated);
    assert_eq!(step.position.engine, EngineState::CombatPlayerTurn);
}

pub(super) fn power(power_type: PowerId, amount: i32) -> Power {
    Power {
        power_type,
        instance_id: None,
        amount,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    }
}

pub(super) fn monster(enemy_id: EnemyId, id: usize, slot: u8, hp: i32) -> MonsterEntity {
    let mut monster = test_monster(enemy_id);
    monster.id = id;
    monster.slot = slot;
    monster.current_hp = hp;
    monster.max_hp = hp.max(monster.max_hp);
    monster
}
