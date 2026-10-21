pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "presence flag, not a numeric stack count",
    effect: "Targeting an enemy with a card turns the player toward it. An enemy behind the player deals 50% more attack damage. Untargeted cards do not turn the player. Actual damage uses facing and position; Back Attack is the visible marker. Ends when the surrounding pair is broken.",
    timing: "Facing changes at targeted card play; remains while surrounded.",
    round_duration: false,
};

pub const BACK_ATTACK_MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "presence flag, not a numeric stack count",
    effect: "Marks an enemy behind the Surrounded player. Its attacks deal 50% more damage. Targeted cards can change facing; actual damage follows facing, and this display marker refreshes when the card queue settles.",
    timing: "Derived from player facing while Surrounded, not a stack-based damage modifier.",
    round_duration: false,
};

use crate::content::powers::{store, PowerId};
use crate::runtime::combat::{CombatState, Power, PowerPayload};
use crate::EntityId;

pub fn monster_is_left_of_player(state: &CombatState, monster_id: EntityId) -> Option<bool> {
    state
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == monster_id)
        .map(|monster| {
            match monster.enemy_id {
                // Java positions Shield slightly left of the player and Spear
                // far to the right. Protocol imports may store absolute drawX
                // in logical_position, so keep the Act 4 pair semantic by type.
                crate::content::monsters::EnemyId::SpireShield => true,
                crate::content::monsters::EnemyId::SpireSpear => false,
                _ => monster.logical_position < 0,
            }
        })
}

fn monster_is_behind_player(monster_is_left: bool, player_facing_left: bool) -> bool {
    if player_facing_left {
        !monster_is_left
    } else {
        monster_is_left
    }
}

/// Java `AbstractMonster.applyBackAttack()` is the mechanical source of the
/// 1.5x monster-attack multiplier.  It reads player facing and monster
/// position directly; the visible `BackAttackPower` icon is only a lagging UI
/// marker maintained by `CardGroup.refreshHandLayout()`.
pub fn monster_has_back_attack_from_facing(state: &CombatState, monster_id: EntityId) -> bool {
    store::has_power(state, 0, PowerId::Surrounded)
        && monster_is_left_of_player(state, monster_id).is_some_and(|monster_is_left| {
            monster_is_behind_player(monster_is_left, state.entities.player.facing_left)
        })
}

fn back_attack_power() -> Power {
    Power {
        power_type: PowerId::BackAttack,
        instance_id: None,
        amount: -1,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: true,
    }
}

/// Java `AbstractPlayer.playCard()` flips the player toward the hovered target
/// only while Surrounded is active. In headless Rust this is the mechanical
/// substitute for that UI field mutation.
pub fn face_target_for_surrounded_if_needed(state: &mut CombatState, target: Option<EntityId>) {
    if !store::has_power(state, 0, PowerId::Surrounded) {
        return;
    }

    let Some(target_id) = target else {
        return;
    };
    let Some(target_is_left) = monster_is_left_of_player(state, target_id) else {
        return;
    };

    state.entities.player.facing_left = target_is_left;
}

/// Refresh Java's visible `BackAttackPower` markers from the current facing.
///
/// This is deliberately separate from `face_target_for_surrounded_if_needed`:
/// `AbstractPlayer.playCard()` turns immediately, while Java does not move the
/// marker until `CardGroup.refreshHandLayout()` calls monster `applyPowers()`
/// and `removeSurroundedPower()`.  Combat damage must therefore never use this
/// marker as its source of truth.
pub fn sync_back_attack_markers(state: &mut CombatState) {
    let surrounded = store::has_power(state, 0, PowerId::Surrounded);
    let facing_left = state.entities.player.facing_left;
    let updates = state
        .entities
        .monsters
        .iter()
        .map(|monster| {
            let monster_is_left = monster_is_left_of_player(state, monster.id).unwrap_or(false);
            let should_have = surrounded
                && !monster.is_dying
                && !monster.is_escaped
                && !monster.half_dead
                && monster_is_behind_player(monster_is_left, facing_left);
            (monster.id, should_have)
        })
        .collect::<Vec<_>>();

    for (monster_id, should_have) in updates {
        let has_marker = store::has_power(state, monster_id, PowerId::BackAttack);
        match (should_have, has_marker) {
            (true, false) => {
                store::ensure_powers_for_mut(state, monster_id).push(back_attack_power());
                store::sort_powers_for_java(state, monster_id);
            }
            (false, true) => {
                store::remove_power_type(state, monster_id, PowerId::BackAttack);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        face_target_for_surrounded_if_needed, monster_has_back_attack_from_facing,
        sync_back_attack_markers,
    };
    use crate::content::monsters::EnemyId;
    use crate::content::powers::{store, PowerId};
    use crate::engine::core::tick_until_stable_turn;
    use crate::runtime::combat::{Power, PowerPayload};
    use crate::state::core::{ClientInput, EngineState, PendingChoice};

    fn sentinel_power(power_type: PowerId) -> Power {
        Power {
            power_type,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }
    }

    fn shield_and_spear_state() -> crate::runtime::combat::CombatState {
        let mut shield = crate::test_support::test_monster(EnemyId::SpireShield);
        shield.id = 1;
        shield.logical_position = -1;
        let mut spear = crate::test_support::test_monster(EnemyId::SpireSpear);
        spear.id = 2;
        spear.logical_position = 1;
        let mut state = crate::test_support::combat_with_monsters(vec![shield, spear]);
        store::set_powers_for(&mut state, 0, vec![sentinel_power(PowerId::Surrounded)]);
        state
    }

    #[test]
    fn sync_marks_initial_right_facing_left_side_back_attack_like_java() {
        let mut state = shield_and_spear_state();
        state.entities.player.facing_left = false;

        sync_back_attack_markers(&mut state);

        assert!(store::has_power(&state, 1, PowerId::BackAttack));
        assert!(!store::has_power(&state, 2, PowerId::BackAttack));
    }

    #[test]
    fn targeting_left_monster_flips_immediately_but_visible_marker_waits_for_refresh() {
        let mut state = shield_and_spear_state();
        sync_back_attack_markers(&mut state);

        face_target_for_surrounded_if_needed(&mut state, Some(1));

        assert!(state.entities.player.facing_left);
        assert!(store::has_power(&state, 1, PowerId::BackAttack));
        assert!(!store::has_power(&state, 2, PowerId::BackAttack));
        assert!(!monster_has_back_attack_from_facing(&state, 1));
        assert!(monster_has_back_attack_from_facing(&state, 2));

        sync_back_attack_markers(&mut state);

        assert!(!store::has_power(&state, 1, PowerId::BackAttack));
        assert!(store::has_power(&state, 2, PowerId::BackAttack));
    }

    #[test]
    fn settled_targeted_card_play_refreshes_marker_after_facing_changes() {
        let mut state = shield_and_spear_state();
        state.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Strike,
            41,
        )];
        sync_back_attack_markers(&mut state);

        let mut engine = EngineState::CombatPlayerTurn;
        tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        );

        assert_eq!(engine, EngineState::CombatPlayerTurn);
        assert!(state.entities.player.facing_left);
        assert!(!store::has_power(&state, 1, PowerId::BackAttack));
        assert!(store::has_power(&state, 2, PowerId::BackAttack));
    }

    #[test]
    fn headbutt_grid_choice_preserves_old_markers_until_action_queue_settles() {
        use crate::content::cards::CardId;
        use crate::runtime::combat::CombatCard;
        use crate::state::selection::{SelectionResolution, SelectionScope};

        let mut state = shield_and_spear_state();
        state.zones.hand = vec![CombatCard::for_scenario(CardId::Headbutt, 41)];
        let selected = CombatCard::for_scenario(CardId::Strike, 42);
        let selected_lineage = selected.lineage;
        state.zones.discard_pile =
            vec![selected, CombatCard::for_scenario(CardId::Defend, 43)].into();
        sync_back_attack_markers(&mut state);
        let mut engine = EngineState::CombatPlayerTurn;

        tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        );

        assert!(matches!(
            engine,
            EngineState::PendingChoice(PendingChoice::GridSelect { .. })
        ));
        assert!(state.entities.player.facing_left);
        assert!(store::has_power(&state, 1, PowerId::BackAttack));
        assert!(!store::has_power(&state, 2, PowerId::BackAttack));
        assert!(!monster_has_back_attack_from_facing(&state, 1));
        assert!(monster_has_back_attack_from_facing(&state, 2));

        tick_until_stable_turn(
            &mut engine,
            &mut state,
            ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Grid,
                [selected_lineage],
            )),
        );

        assert_eq!(engine, EngineState::CombatPlayerTurn);
        assert!(!store::has_power(&state, 1, PowerId::BackAttack));
        assert!(store::has_power(&state, 2, PowerId::BackAttack));
    }

    #[test]
    fn removing_surrounded_clears_back_attack_markers() {
        let mut state = shield_and_spear_state();
        sync_back_attack_markers(&mut state);

        store::remove_power_type(&mut state, 0, PowerId::Surrounded);
        sync_back_attack_markers(&mut state);

        assert!(!store::has_power(&state, 1, PowerId::BackAttack));
        assert!(!store::has_power(&state, 2, PowerId::BackAttack));
    }

    #[test]
    fn only_successful_draw_refreshes_markers_before_queue_settlement() {
        use crate::content::cards::CardId;
        use crate::runtime::combat::CombatCard;

        let mut state = shield_and_spear_state();
        sync_back_attack_markers(&mut state);
        face_target_for_surrounded_if_needed(&mut state, Some(1));

        crate::engine::action_handlers::cards::handle_draw_cards(1, &mut state);
        assert!(store::has_power(&state, 1, PowerId::BackAttack));
        assert!(!store::has_power(&state, 2, PowerId::BackAttack));

        state.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 51)].into();
        crate::engine::action_handlers::cards::handle_draw_cards(1, &mut state);
        assert!(!store::has_power(&state, 1, PowerId::BackAttack));
        assert!(store::has_power(&state, 2, PowerId::BackAttack));
    }

    #[test]
    fn protocol_absolute_draw_x_keeps_shield_left_and_spear_right_by_type() {
        let mut state = shield_and_spear_state();
        state.entities.monsters[0].logical_position = 440;
        state.entities.monsters[1].logical_position = 1510;
        state.entities.player.facing_left = false;

        sync_back_attack_markers(&mut state);

        assert!(store::has_power(&state, 1, PowerId::BackAttack));
        assert!(!store::has_power(&state, 2, PowerId::BackAttack));
    }
}
