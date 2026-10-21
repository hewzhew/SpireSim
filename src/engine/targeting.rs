use crate::content::cards::CardTarget;
use crate::runtime::combat::CombatState;
use crate::state::TargetValidation;
use crate::EntityId;

pub fn validation_for_card_target(target: CardTarget) -> Option<TargetValidation> {
    match target {
        CardTarget::Enemy | CardTarget::SelfAndEnemy => Some(TargetValidation::AnyEnemy),
        _ => None,
    }
}

pub fn validation_for_potion_target(target_required: bool) -> Option<TargetValidation> {
    if target_required {
        Some(TargetValidation::AnyMonster)
    } else {
        None
    }
}

pub fn candidate_targets(state: &CombatState, validation: TargetValidation) -> Vec<EntityId> {
    match validation {
        TargetValidation::AnyEnemy | TargetValidation::AnyMonster => state
            .entities
            .monsters
            .iter()
            .filter(|m| m.current_hp > 0 && !m.is_dying && !m.is_escaped && !m.half_dead)
            .map(|m| m.id)
            .collect(),
    }
}

fn random_target_candidates(state: &CombatState, validation: TargetValidation) -> Vec<EntityId> {
    match validation {
        TargetValidation::AnyEnemy | TargetValidation::AnyMonster => state
            .entities
            .monsters
            .iter()
            .filter(|m| m.is_random_target_candidate())
            .map(|m| m.id)
            .collect(),
    }
}

pub fn resolve_target_request(
    state: &CombatState,
    validation: Option<TargetValidation>,
    requested: Option<EntityId>,
) -> Result<Option<EntityId>, &'static str> {
    let Some(validation) = validation else {
        return Ok(None);
    };

    let targetable = candidate_targets(state, validation);
    match requested {
        Some(target) if targetable.contains(&target) => Ok(Some(target)),
        Some(_) => Err("Invalid or untargetable monster selected."),
        None if targetable.is_empty() => Err("No valid targets available."),
        None if targetable.len() == 1 => Ok(Some(targetable[0])),
        None => Err("Multiple targets available. Must specify a target."),
    }
}

pub fn pick_random_target(
    state: &mut CombatState,
    validation: TargetValidation,
) -> Option<EntityId> {
    let targetable = random_target_candidates(state, validation);
    if targetable.is_empty() {
        return None;
    }
    let idx = state
        .rng
        .card_random_rng
        .random_range(0, targetable.len() as i32 - 1) as usize;
    targetable.get(idx).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn random_target_records_java_inclusive_range_even_for_one_enemy() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};
        let mut state = crate::test_support::blank_test_combat();
        state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
        let trace = begin_sts_rng_trace().unwrap();
        assert!(pick_random_target(&mut state, TargetValidation::AnyEnemy).is_some());
        let calls = trace.finish().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].call,
            StsRngCall::RandomIntRange {
                start: 0,
                end: 0,
                result: 0,
            }
        );
        state.entities.monsters.clear();
        let trace = begin_sts_rng_trace().unwrap();
        assert_eq!(
            pick_random_target(&mut state, TargetValidation::AnyEnemy),
            None
        );
        assert!(trace.finish().unwrap().is_empty());
    }

    #[test]
    fn manual_target_candidates_exclude_zero_hp_like_java_player_targeting() {
        let mut state = crate::test_support::blank_test_combat();
        let mut zero_hp_not_dying = crate::test_support::test_monster(EnemyId::JawWorm);
        zero_hp_not_dying.id = 701;
        zero_hp_not_dying.current_hp = 0;
        zero_hp_not_dying.is_dying = false;
        zero_hp_not_dying.is_escaped = false;
        let mut alive = crate::test_support::test_monster(EnemyId::Cultist);
        alive.id = 702;
        alive.current_hp = 12;
        state.entities.monsters = vec![zero_hp_not_dying, alive];

        assert_eq!(
            candidate_targets(&state, TargetValidation::AnyEnemy),
            vec![702]
        );
        assert_eq!(
            resolve_target_request(&state, Some(TargetValidation::AnyEnemy), Some(701)),
            Err("Invalid or untargetable monster selected.")
        );
    }

    #[test]
    fn random_target_candidates_do_not_exclude_zero_hp_like_java_get_random_monster() {
        let mut state = crate::test_support::blank_test_combat();
        let mut zero_hp_not_dying = crate::test_support::test_monster(EnemyId::JawWorm);
        zero_hp_not_dying.id = 703;
        zero_hp_not_dying.current_hp = 0;
        zero_hp_not_dying.is_dying = false;
        zero_hp_not_dying.is_escaped = false;
        zero_hp_not_dying.half_dead = false;
        state.entities.monsters = vec![zero_hp_not_dying];

        assert_eq!(
            pick_random_target(&mut state, TargetValidation::AnyEnemy),
            Some(703)
        );
    }
}
