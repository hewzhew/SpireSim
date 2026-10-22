use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::Orichalcum,
};

/// Orichalcum: If you end your turn without Block, gain 6 Block.
pub fn at_end_of_turn(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    // We must check if the player currently has NO block at the end of the turn.
    if state.entities.player.block == 0 {
        actions.push(ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: 6,
            },
            insertion_mode: AddTo::Top,
        });
    }

    actions
}

#[cfg(test)]
mod tests {
    use crate::content::powers::PowerId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::{Power, PowerPayload};

    #[test]
    fn orichalcum_checks_before_metallicize_and_both_gains_settle() {
        for (initial_block, metallicize, expected_block) in
            [(0, 0, 6), (0, 3, 9), (0, 4, 10), (1, 3, 4), (5, 4, 9)]
        {
            let mut state = crate::test_support::blank_test_combat();
            state.entities.player.block = initial_block;
            state
                .entities
                .player
                .add_relic(RelicState::new(RelicId::Orichalcum));
            if metallicize > 0 {
                state.entities.power_db.insert(
                    0,
                    vec![Power {
                        power_type: PowerId::Metallicize,
                        instance_id: None,
                        amount: metallicize,
                        extra_data: 0,
                        payload: PowerPayload::None,
                        just_applied: false,
                    }],
                );
            }
            crate::engine::action_handlers::cards::handle_end_turn_trigger(&mut state);
            while let Some(action) = state.pop_next_action() {
                crate::engine::action_handlers::execute_action(action, &mut state);
            }
            assert_eq!(
                state.entities.player.block, expected_block,
                "starting Block {initial_block}, Metallicize {metallicize}"
            );
        }
    }
}
