use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::Brimstone,
};

pub struct Brimstone;

impl Brimstone {
    pub fn at_turn_start(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();

        // Java calls addToTop for the player strength, then addToTop once per
        // monster in monster-list order. The shared queue helper performs the
        // Java addToTop reversal when these ActionInfo records are inserted.
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: state.entities.player.id,
                target: state.entities.player.id,
                power_id: crate::content::powers::PowerId::Strength,
                amount: 2,
            },
            insertion_mode: AddTo::Top,
        });

        for monster in &state.entities.monsters {
            actions.push(ActionInfo {
                action: Action::ApplyPower {
                    source: monster.id,
                    target: monster.id,
                    power_id: crate::content::powers::PowerId::Strength,
                    amount: 1,
                },
                insertion_mode: AddTo::Top,
            });
        }

        actions
    }
}
